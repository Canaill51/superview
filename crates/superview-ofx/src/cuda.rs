//! The warp on the GPU, for a host that hands over CUDA memory (DEC-66).
//!
//! Resolve keeps its frames on the GPU; a CPU-only effect makes it copy each one to the CPU and
//! back, which cost about 150 ms a 4K frame on a laptop. In CUDA mode the effect receives device
//! pointers and enqueues one kernel on the host's stream instead.
//!
//! The CUDA driver (`nvcuda.dll`, `libcuda.so.1`) is loaded at run time: it comes with the NVIDIA
//! display driver, so nothing is installed with the plugin and a machine without it still loads
//! the plugin, which then renders on the CPU. The kernel is PTX text, compiled by the driver for
//! the GPU it runs on; no CUDA toolkit is needed to build or to run.
//!
//! The kernel reads the same column map as the CPU warp ([`crate::warp::column_map`]), so both
//! paths give the same pixels; `tests/cuda.rs` compares them on a real GPU.

use std::ffi::{CStr, c_char, c_int, c_uint, c_void};
use std::sync::OnceLock;

type CuResult = c_int;
type CuDevice = c_int;
pub type CuContext = *mut c_void;
type CuModule = *mut c_void;
type CuFunction = *mut c_void;
pub type CuStream = *mut c_void;
pub type CuDevicePtr = u64;

const CUDA_SUCCESS: CuResult = 0;
const CU_JIT_ERROR_LOG_BUFFER: c_uint = 5;
const CU_JIT_ERROR_LOG_BUFFER_SIZE_BYTES: c_uint = 6;

/// Pixels are RGBA float, 16 bytes: the only format the host uses in CUDA mode. Any other goes
/// back to the CPU path.
pub const PIXEL_BYTES: usize = 16;

/// One thread per output pixel: `dst[y][x] = src[y][map[x]]`, 16 bytes moved as one vector.
/// `src` and `dst` point at the first row to write; the pitches are in bytes.
const WARP_PTX: &CStr = c"
.version 6.0
.target sm_50
.address_size 64

.visible .entry superview_warp(
    .param .u64 p_src,
    .param .u64 p_dst,
    .param .u64 p_map,
    .param .u32 p_width,
    .param .u32 p_rows,
    .param .u64 p_src_pitch,
    .param .u64 p_dst_pitch
)
{
    .reg .pred %p<4>;
    .reg .b32 %r<16>;
    .reg .b64 %rd<16>;

    ld.param.u64 %rd1, [p_src];
    ld.param.u64 %rd2, [p_dst];
    ld.param.u64 %rd3, [p_map];
    ld.param.u32 %r1, [p_width];
    ld.param.u32 %r2, [p_rows];
    ld.param.u64 %rd4, [p_src_pitch];
    ld.param.u64 %rd5, [p_dst_pitch];

    mov.u32 %r3, %ctaid.x;
    mov.u32 %r4, %ntid.x;
    mov.u32 %r5, %tid.x;
    mad.lo.s32 %r6, %r3, %r4, %r5;
    mov.u32 %r7, %ctaid.y;
    mov.u32 %r8, %ntid.y;
    mov.u32 %r9, %tid.y;
    mad.lo.s32 %r10, %r7, %r8, %r9;
    setp.ge.u32 %p1, %r6, %r1;
    setp.ge.u32 %p2, %r10, %r2;
    or.pred %p3, %p1, %p2;
    @%p3 bra DONE;

    cvta.to.global.u64 %rd6, %rd1;
    cvta.to.global.u64 %rd7, %rd2;
    cvta.to.global.u64 %rd8, %rd3;

    mul.wide.u32 %rd9, %r6, 4;
    add.s64 %rd9, %rd8, %rd9;
    ld.global.nc.u32 %r11, [%rd9];

    cvt.u64.u32 %rd10, %r10;
    mad.lo.s64 %rd11, %rd10, %rd4, %rd6;
    mul.wide.u32 %rd12, %r11, 16;
    add.s64 %rd11, %rd11, %rd12;

    mad.lo.s64 %rd13, %rd10, %rd5, %rd7;
    mul.wide.u32 %rd14, %r6, 16;
    add.s64 %rd13, %rd13, %rd14;

    ld.global.nc.v4.u32 {%r12, %r13, %r14, %r15}, [%rd11];
    st.global.v4.u32 [%rd13], {%r12, %r13, %r14, %r15};
DONE:
    ret;
}
";

const KERNEL_NAME: &CStr = c"superview_warp";
const BLOCK_WIDTH: c_uint = 128;

#[cfg(windows)]
const DRIVER_LIBRARY: &str = "nvcuda.dll";
#[cfg(not(windows))]
const DRIVER_LIBRARY: &str = "libcuda.so.1";

/// The entry points of the CUDA driver this module uses. The library stays loaded as long as
/// the pointers live.
pub struct Driver {
    _library: libloading::Library,
    init: unsafe extern "C" fn(c_uint) -> CuResult,
    device_get: unsafe extern "C" fn(*mut CuDevice, c_int) -> CuResult,
    primary_ctx_retain: unsafe extern "C" fn(*mut CuContext, CuDevice) -> CuResult,
    ctx_get_current: unsafe extern "C" fn(*mut CuContext) -> CuResult,
    ctx_set_current: unsafe extern "C" fn(CuContext) -> CuResult,
    ctx_push_current: unsafe extern "C" fn(CuContext) -> CuResult,
    ctx_pop_current: unsafe extern "C" fn(*mut CuContext) -> CuResult,
    module_load_data_ex: unsafe extern "C" fn(
        *mut CuModule,
        *const c_void,
        c_uint,
        *mut c_uint,
        *mut *mut c_void,
    ) -> CuResult,
    module_get_function: unsafe extern "C" fn(*mut CuFunction, CuModule, *const c_char) -> CuResult,
    module_unload: unsafe extern "C" fn(CuModule) -> CuResult,
    mem_alloc: unsafe extern "C" fn(*mut CuDevicePtr, usize) -> CuResult,
    mem_free: unsafe extern "C" fn(CuDevicePtr) -> CuResult,
    memcpy_htod: unsafe extern "C" fn(CuDevicePtr, *const c_void, usize) -> CuResult,
    memcpy_dtoh: unsafe extern "C" fn(*mut c_void, CuDevicePtr, usize) -> CuResult,
    launch_kernel: unsafe extern "C" fn(
        CuFunction,
        c_uint,
        c_uint,
        c_uint,
        c_uint,
        c_uint,
        c_uint,
        c_uint,
        CuStream,
        *mut *mut c_void,
        *mut *mut c_void,
    ) -> CuResult,
    stream_synchronize: unsafe extern "C" fn(CuStream) -> CuResult,
    get_error_name: unsafe extern "C" fn(CuResult, *mut *const c_char) -> CuResult,
}

impl Driver {
    /// The driver of this process, loaded once; `None` where there is none (no NVIDIA driver).
    pub fn get() -> Option<&'static Driver> {
        static DRIVER: OnceLock<Option<Driver>> = OnceLock::new();
        DRIVER.get_or_init(|| Driver::load().ok()).as_ref()
    }

    fn load() -> Result<Driver, libloading::Error> {
        // SAFETY: loading the CUDA driver runs its initialisers, which have no precondition;
        // each symbol is cast to the signature of the CUDA driver API (cuda.h), whose `_v2`
        // names are the ones the 64-bit headers map the plain names to.
        unsafe {
            let library = libloading::Library::new(DRIVER_LIBRARY)?;
            macro_rules! symbol {
                ($name:literal) => {
                    *library.get($name)?
                };
            }
            Ok(Driver {
                init: symbol!(b"cuInit\0"),
                device_get: symbol!(b"cuDeviceGet\0"),
                primary_ctx_retain: symbol!(b"cuDevicePrimaryCtxRetain\0"),
                ctx_get_current: symbol!(b"cuCtxGetCurrent\0"),
                ctx_set_current: symbol!(b"cuCtxSetCurrent\0"),
                ctx_push_current: symbol!(b"cuCtxPushCurrent_v2\0"),
                ctx_pop_current: symbol!(b"cuCtxPopCurrent_v2\0"),
                module_load_data_ex: symbol!(b"cuModuleLoadDataEx\0"),
                module_get_function: symbol!(b"cuModuleGetFunction\0"),
                module_unload: symbol!(b"cuModuleUnload\0"),
                mem_alloc: symbol!(b"cuMemAlloc_v2\0"),
                mem_free: symbol!(b"cuMemFree_v2\0"),
                memcpy_htod: symbol!(b"cuMemcpyHtoD_v2\0"),
                memcpy_dtoh: symbol!(b"cuMemcpyDtoH_v2\0"),
                launch_kernel: symbol!(b"cuLaunchKernel\0"),
                stream_synchronize: symbol!(b"cuStreamSynchronize\0"),
                get_error_name: symbol!(b"cuGetErrorName\0"),
                _library: library,
            })
        }
    }

    fn check(&self, result: CuResult, call: &str) -> Result<(), String> {
        if result == CUDA_SUCCESS {
            return Ok(());
        }
        let mut name: *const c_char = std::ptr::null();
        // SAFETY: cuGetErrorName writes a pointer to a static string, or leaves it null.
        let known = unsafe { (self.get_error_name)(result, &mut name) } == CUDA_SUCCESS;
        let name = if known && !name.is_null() {
            // SAFETY: a non-null name from cuGetErrorName is a static NUL-terminated string.
            unsafe { CStr::from_ptr(name) }
                .to_string_lossy()
                .into_owned()
        } else {
            format!("error {result}")
        };
        Err(format!("{call}: {name}"))
    }

    /// The context the host made current on this thread, if any.
    pub fn current_context(&self) -> Result<Option<CuContext>, String> {
        let mut context: CuContext = std::ptr::null_mut();
        // SAFETY: cuCtxGetCurrent only writes the out pointer.
        self.check(
            unsafe { (self.ctx_get_current)(&mut context) },
            "cuCtxGetCurrent",
        )?;
        Ok((!context.is_null()).then_some(context))
    }

    /// Makes the primary context of the first GPU current on this thread: for the tests, which
    /// have no host to set one up.
    pub fn use_first_device(&self) -> Result<CuContext, String> {
        let mut device: CuDevice = 0;
        let mut context: CuContext = std::ptr::null_mut();
        // SAFETY: each call only writes its out pointer; the context is retained for good.
        unsafe {
            self.check((self.init)(0), "cuInit")?;
            self.check((self.device_get)(&mut device, 0), "cuDeviceGet")?;
            self.check(
                (self.primary_ctx_retain)(&mut context, device),
                "cuDevicePrimaryCtxRetain",
            )?;
            self.check((self.ctx_set_current)(context), "cuCtxSetCurrent")?;
        }
        Ok(context)
    }

    /// `bytes` of device memory in the current context.
    pub fn alloc(&self, bytes: usize) -> Result<CuDevicePtr, String> {
        let mut pointer: CuDevicePtr = 0;
        // SAFETY: cuMemAlloc only writes the out pointer.
        self.check(
            unsafe { (self.mem_alloc)(&mut pointer, bytes.max(1)) },
            "cuMemAlloc",
        )?;
        Ok(pointer)
    }

    /// # Safety
    /// `pointer` was returned by [`Driver::alloc`] in the current context and is not freed yet.
    pub unsafe fn free(&self, pointer: CuDevicePtr) -> Result<(), String> {
        // SAFETY: the caller's contract.
        self.check(unsafe { (self.mem_free)(pointer) }, "cuMemFree")
    }

    /// # Safety
    /// `to` is device memory of the current context with room for `from`.
    pub unsafe fn upload<T: Copy>(&self, to: CuDevicePtr, from: &[T]) -> Result<(), String> {
        // SAFETY: the caller's contract; `from` is readable for its whole length.
        self.check(
            unsafe { (self.memcpy_htod)(to, from.as_ptr().cast(), size_of_val(from)) },
            "cuMemcpyHtoD",
        )
    }

    /// # Safety
    /// `from` is device memory of the current context holding at least `to`'s length.
    pub unsafe fn download<T: Copy>(&self, to: &mut [T], from: CuDevicePtr) -> Result<(), String> {
        // SAFETY: the caller's contract; `to` is writable for its whole length.
        self.check(
            unsafe { (self.memcpy_dtoh)(to.as_mut_ptr().cast(), from, size_of_val(to)) },
            "cuMemcpyDtoH",
        )
    }

    /// Waits for `stream` (null: the default stream).
    ///
    /// # Safety
    /// `stream` is null or a live stream of the current context.
    pub unsafe fn synchronize(&self, stream: CuStream) -> Result<(), String> {
        // SAFETY: the caller's contract.
        self.check(
            unsafe { (self.stream_synchronize)(stream) },
            "cuStreamSynchronize",
        )
    }
}

/// The warp kernel loaded in one context, and the column map uploaded to it.
pub struct Warp {
    driver: &'static Driver,
    context: CuContext,
    module: CuModule,
    function: CuFunction,
    map: CuDevicePtr,
    map_len: usize,
}

impl Warp {
    /// Loads the kernel in `context`, which must be current on this thread.
    pub fn new(driver: &'static Driver, context: CuContext) -> Result<Warp, String> {
        let mut log = [0u8; 4096];
        let mut options = [CU_JIT_ERROR_LOG_BUFFER, CU_JIT_ERROR_LOG_BUFFER_SIZE_BYTES];
        // The JIT options take their values as pointers, the size cast to one.
        let mut values: [*mut c_void; 2] = [log.as_mut_ptr().cast(), log.len() as *mut c_void];
        let mut module: CuModule = std::ptr::null_mut();
        // SAFETY: the PTX is NUL-terminated; the log buffer outlives the call and its size is
        // the one passed.
        let loaded = unsafe {
            (driver.module_load_data_ex)(
                &mut module,
                WARP_PTX.as_ptr().cast(),
                options.len() as c_uint,
                options.as_mut_ptr(),
                values.as_mut_ptr(),
            )
        };
        if let Err(err) = driver.check(loaded, "cuModuleLoadDataEx") {
            let log = CStr::from_bytes_until_nul(&log)
                .map(|log| log.to_string_lossy().into_owned())
                .unwrap_or_default();
            return Err(format!("{err} {log}").trim_end().to_string());
        }
        let mut function: CuFunction = std::ptr::null_mut();
        // SAFETY: `module` was just loaded; the name is NUL-terminated.
        let found =
            unsafe { (driver.module_get_function)(&mut function, module, KERNEL_NAME.as_ptr()) };
        if let Err(err) = driver.check(found, "cuModuleGetFunction") {
            // SAFETY: `module` is loaded and used by nothing else.
            unsafe { (driver.module_unload)(module) };
            return Err(err);
        }
        Ok(Warp {
            driver,
            context,
            module,
            function,
            map: 0,
            map_len: 0,
        })
    }

    /// The context the kernel lives in.
    pub fn context(&self) -> CuContext {
        self.context
    }

    /// Uploads the column map, every entry below its length. Call with the context current.
    pub fn set_map(&mut self, map: &[usize]) -> Result<(), String> {
        let columns: Vec<u32> = map
            .iter()
            .map(|&from| u32::try_from(from).map_err(|_| "a column beyond u32".to_string()))
            .collect::<Result<_, _>>()?;
        if map.iter().any(|&from| from >= map.len()) {
            return Err("the map points outside the row".to_string());
        }
        if columns.len() != self.map_len {
            if self.map != 0 {
                // SAFETY: `self.map` came from `alloc` in this context, which is current.
                unsafe { self.driver.free(self.map) }?;
                self.map = 0;
                self.map_len = 0;
            }
            self.map = self.driver.alloc(size_of_val(columns.as_slice()))?;
            self.map_len = columns.len();
        }
        // SAFETY: `self.map` holds `map_len` = `columns.len()` entries of u32.
        unsafe { self.driver.upload(self.map, &columns) }
    }

    /// Enqueues the warp of `rows` rows of `map`'s width on `stream`.
    ///
    /// # Safety
    /// The context is current; `src` and `dst` are device memory of it, each row `pitch` bytes
    /// apart and holding `width` RGBA float pixels; the map was set with [`Warp::set_map`] for
    /// this width; nothing else writes `dst` until the stream has run the kernel.
    pub unsafe fn launch(
        &self,
        src: CuDevicePtr,
        dst: CuDevicePtr,
        rows: usize,
        src_pitch: usize,
        dst_pitch: usize,
        stream: CuStream,
    ) -> Result<(), String> {
        if self.map_len == 0 || rows == 0 {
            return Ok(());
        }
        let width = c_uint::try_from(self.map_len).map_err(|_| "frame too wide")?;
        let rows = c_uint::try_from(rows).map_err(|_| "frame too tall")?;
        let (mut src, mut dst, mut map) = (src, dst, self.map);
        let (mut width_arg, mut rows_arg) = (width, rows);
        let (mut src_pitch, mut dst_pitch) = (src_pitch as u64, dst_pitch as u64);
        let mut params: [*mut c_void; 7] = [
            (&raw mut src).cast(),
            (&raw mut dst).cast(),
            (&raw mut map).cast(),
            (&raw mut width_arg).cast(),
            (&raw mut rows_arg).cast(),
            (&raw mut src_pitch).cast(),
            (&raw mut dst_pitch).cast(),
        ];
        // SAFETY: the parameters match the kernel's, in order and in size; the caller's
        // contract covers the memory.
        let launched = unsafe {
            (self.driver.launch_kernel)(
                self.function,
                width.div_ceil(BLOCK_WIDTH),
                rows,
                1,
                BLOCK_WIDTH,
                1,
                1,
                0,
                stream,
                params.as_mut_ptr(),
                std::ptr::null_mut(),
            )
        };
        self.driver.check(launched, "cuLaunchKernel")
    }
}

impl Drop for Warp {
    fn drop(&mut self) {
        let driver = self.driver;
        // The instance may be destroyed on a thread where the host's context is not current.
        // SAFETY: the context is the one everything here was created in; a context the host has
        // already destroyed makes these calls fail, which is ignored.
        unsafe {
            if (driver.ctx_push_current)(self.context) != CUDA_SUCCESS {
                return;
            }
            if self.map != 0 {
                let _ = driver.free(self.map);
            }
            (driver.module_unload)(self.module);
            let mut popped: CuContext = std::ptr::null_mut();
            (driver.ctx_pop_current)(&mut popped);
        }
    }
}
