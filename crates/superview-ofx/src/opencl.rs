//! The warp on the GPU, for a host that hands over OpenCL buffers (AMD, Intel, or Resolve set to
//! OpenCL on NVIDIA).
//!
//! What Resolve hands over was measured by a throwaway probe (`docs/TESTS-MANUELS.md`, *Sonde
//! OpenCL*): `cl_mem` **buffers**, never OpenCL images; RGBA float rows `row_bytes` apart; the
//! host's command queue, in the context of those buffers; buffers taken from a pool, larger than
//! the frame. A frame that returns `FAILED` is done again on the CPU, but the next one is asked
//! in OpenCL again, so a kernel that does not build is remembered per context
//! ([`crate::gpu::PerContext`]).
//!
//! The OpenCL loader (`OpenCL.dll`, `libOpenCL.so.1`) is loaded at run time: it comes with the
//! display driver, so nothing is installed with the plugin and a machine without it still loads
//! the plugin. The kernel is OpenCL C, compiled by the driver for the device of the host's queue.
//! Only OpenCL 1.2 entry points are used.
//!
//! The kernel reads the same column map as the CPU warp ([`crate::warp::column_map`]), so both
//! paths give the same pixels; `tests/opencl.rs` compares them on every GPU of the machine.

use std::ffi::{CStr, c_char, c_void};
use std::sync::OnceLock;

type ClInt = i32;
type ClUint = u32;
type ClBitfield = u64;
type ClPlatform = *mut c_void;
pub type ClDevice = *mut c_void;
pub type ClContext = *mut c_void;
pub type ClQueue = *mut c_void;
pub type ClMem = *mut c_void;
type ClProgram = *mut c_void;
type ClKernel = *mut c_void;
type ClEvent = *mut c_void;

const CL_SUCCESS: ClInt = 0;
const CL_DEVICE_NOT_FOUND: ClInt = -1;
const CL_PLATFORM_NOT_FOUND_KHR: ClInt = -1001;
const CL_TRUE: ClUint = 1;
const CL_DEVICE_TYPE_GPU: ClBitfield = 1 << 2;
const CL_DEVICE_NAME: ClUint = 0x102B;
const CL_QUEUE_CONTEXT: ClUint = 0x1090;
const CL_QUEUE_DEVICE: ClUint = 0x1091;
const CL_MEM_SIZE: ClUint = 0x1102;
const CL_PROGRAM_BUILD_LOG: ClUint = 0x1183;
const CL_MEM_READ_WRITE: ClBitfield = 1;
const CL_MEM_READ_ONLY: ClBitfield = 1 << 2;
const CL_MEM_COPY_HOST_PTR: ClBitfield = 1 << 5;

/// Pixels are RGBA float, 16 bytes: the only format the host uses in OpenCL mode. Any other goes
/// back to the CPU path.
pub const PIXEL_BYTES: usize = 16;

/// One work-item per output pixel: `dst[y][x] = src[y][map[x]]`, 16 bytes moved as one `uint4`
/// (a copy of bits, as the PTX does: no float is rewritten). A buffer cannot be offset from the
/// host, so the first row of the band and the pitches, in bytes, are arguments.
const WARP_SOURCE: &CStr = c"
__kernel void superview_warp(
    __global const uchar *src,
    __global uchar *dst,
    __global const uint *map,
    uint width,
    uint first_row,
    uint rows,
    ulong src_pitch,
    ulong dst_pitch)
{
    uint x = get_global_id(0);
    uint y = get_global_id(1);
    if (x >= width || y >= rows) {
        return;
    }
    ulong row = (ulong)first_row + y;
    __global const uint4 *from = (__global const uint4 *)(src + row * src_pitch);
    __global uint4 *to = (__global uint4 *)(dst + row * dst_pitch);
    to[x] = from[map[x]];
}
";

const KERNEL_NAME: &CStr = c"superview_warp";

#[cfg(windows)]
const LOADER_LIBRARY: &str = "OpenCL.dll";
#[cfg(not(windows))]
const LOADER_LIBRARY: &str = "libOpenCL.so.1";

/// The entry points of the OpenCL loader this module uses. The library stays loaded as long as
/// the pointers live.
pub struct Library {
    _library: libloading::Library,
    get_platform_ids: unsafe extern "system" fn(ClUint, *mut ClPlatform, *mut ClUint) -> ClInt,
    get_device_ids: unsafe extern "system" fn(
        ClPlatform,
        ClBitfield,
        ClUint,
        *mut ClDevice,
        *mut ClUint,
    ) -> ClInt,
    get_device_info:
        unsafe extern "system" fn(ClDevice, ClUint, usize, *mut c_void, *mut usize) -> ClInt,
    create_context: unsafe extern "system" fn(
        *const isize,
        ClUint,
        *const ClDevice,
        *const c_void,
        *mut c_void,
        *mut ClInt,
    ) -> ClContext,
    create_command_queue:
        unsafe extern "system" fn(ClContext, ClDevice, ClBitfield, *mut ClInt) -> ClQueue,
    get_command_queue_info:
        unsafe extern "system" fn(ClQueue, ClUint, usize, *mut c_void, *mut usize) -> ClInt,
    create_program_with_source: unsafe extern "system" fn(
        ClContext,
        ClUint,
        *const *const c_char,
        *const usize,
        *mut ClInt,
    ) -> ClProgram,
    build_program: unsafe extern "system" fn(
        ClProgram,
        ClUint,
        *const ClDevice,
        *const c_char,
        *const c_void,
        *mut c_void,
    ) -> ClInt,
    get_program_build_info: unsafe extern "system" fn(
        ClProgram,
        ClDevice,
        ClUint,
        usize,
        *mut c_void,
        *mut usize,
    ) -> ClInt,
    create_kernel: unsafe extern "system" fn(ClProgram, *const c_char, *mut ClInt) -> ClKernel,
    set_kernel_arg: unsafe extern "system" fn(ClKernel, ClUint, usize, *const c_void) -> ClInt,
    enqueue_nd_range_kernel: unsafe extern "system" fn(
        ClQueue,
        ClKernel,
        ClUint,
        *const usize,
        *const usize,
        *const usize,
        ClUint,
        *const ClEvent,
        *mut ClEvent,
    ) -> ClInt,
    create_buffer:
        unsafe extern "system" fn(ClContext, ClBitfield, usize, *mut c_void, *mut ClInt) -> ClMem,
    get_mem_object_info:
        unsafe extern "system" fn(ClMem, ClUint, usize, *mut c_void, *mut usize) -> ClInt,
    enqueue_write_buffer: unsafe extern "system" fn(
        ClQueue,
        ClMem,
        ClUint,
        usize,
        usize,
        *const c_void,
        ClUint,
        *const ClEvent,
        *mut ClEvent,
    ) -> ClInt,
    enqueue_read_buffer: unsafe extern "system" fn(
        ClQueue,
        ClMem,
        ClUint,
        usize,
        usize,
        *mut c_void,
        ClUint,
        *const ClEvent,
        *mut ClEvent,
    ) -> ClInt,
    finish: unsafe extern "system" fn(ClQueue) -> ClInt,
    release_mem_object: unsafe extern "system" fn(ClMem) -> ClInt,
    release_kernel: unsafe extern "system" fn(ClKernel) -> ClInt,
    release_program: unsafe extern "system" fn(ClProgram) -> ClInt,
    release_command_queue: unsafe extern "system" fn(ClQueue) -> ClInt,
    release_context: unsafe extern "system" fn(ClContext) -> ClInt,
}

/// `call: CL_NAME` for an OpenCL error code, the name when it is a common one.
fn error(call: &str, code: ClInt) -> String {
    let name = match code {
        -1 => "CL_DEVICE_NOT_FOUND",
        -2 => "CL_DEVICE_NOT_AVAILABLE",
        -3 => "CL_COMPILER_NOT_AVAILABLE",
        -4 => "CL_MEM_OBJECT_ALLOCATION_FAILURE",
        -5 => "CL_OUT_OF_RESOURCES",
        -6 => "CL_OUT_OF_HOST_MEMORY",
        -11 => "CL_BUILD_PROGRAM_FAILURE",
        -30 => "CL_INVALID_VALUE",
        -33 => "CL_INVALID_DEVICE",
        -34 => "CL_INVALID_CONTEXT",
        -36 => "CL_INVALID_COMMAND_QUEUE",
        -38 => "CL_INVALID_MEM_OBJECT",
        -45 => "CL_INVALID_PROGRAM_EXECUTABLE",
        -46 => "CL_INVALID_KERNEL_NAME",
        -48 => "CL_INVALID_KERNEL",
        -49 => "CL_INVALID_ARG_INDEX",
        -50 => "CL_INVALID_ARG_VALUE",
        -51 => "CL_INVALID_ARG_SIZE",
        -54 => "CL_INVALID_WORK_GROUP_SIZE",
        -63 => "CL_INVALID_GLOBAL_WORK_SIZE",
        -1001 => "CL_PLATFORM_NOT_FOUND_KHR",
        _ => return format!("{call}: error {code}"),
    };
    format!("{call}: {name}")
}

fn check(code: ClInt, call: &str) -> Result<(), String> {
    if code == CL_SUCCESS {
        Ok(())
    } else {
        Err(error(call, code))
    }
}

/// Checks that a buffer of `size` bytes holds `first_row + rows` rows of `pitch` bytes, each with
/// room for `width` pixels. Only "at least": the host's buffers come from a pool and are larger
/// than the frame.
pub fn check_buffer(
    width: usize,
    first_row: usize,
    rows: usize,
    pitch: usize,
    size: usize,
) -> Result<(), String> {
    if pitch < width.saturating_mul(PIXEL_BYTES) || !pitch.is_multiple_of(PIXEL_BYTES) {
        return Err(format!(
            "unusable row length of {pitch} bytes for {width} pixels"
        ));
    }
    let needed = first_row
        .checked_add(rows)
        .and_then(|rows| rows.checked_mul(pitch))
        .ok_or("a frame beyond the address space")?;
    if size < needed {
        return Err(format!(
            "a buffer of {size} bytes for {} rows of {pitch} bytes",
            first_row + rows
        ));
    }
    Ok(())
}

/// A GPU the loader lists, for the tests.
#[derive(Debug, Clone)]
pub struct Device {
    pub id: ClDevice,
    pub name: String,
}

impl Library {
    /// The loader of this process, loaded once; `None` where there is none.
    pub fn get() -> Option<&'static Library> {
        Library::loaded().as_ref().ok()
    }

    /// Why [`Library::get`] is `None`, for the plugin's log; `None` when the loader loaded.
    pub fn load_error() -> Option<&'static str> {
        Library::loaded().as_ref().err().map(String::as_str)
    }

    fn loaded() -> &'static Result<Library, String> {
        static LIBRARY: OnceLock<Result<Library, String>> = OnceLock::new();
        LIBRARY.get_or_init(|| Library::load().map_err(|e| e.to_string()))
    }

    fn load() -> Result<Library, libloading::Error> {
        // SAFETY: loading the OpenCL loader runs its initialisers, which have no precondition;
        // each symbol is cast to its signature in the OpenCL 1.2 headers (`CL_API_CALL` is
        // `__stdcall` on Windows, hence `extern "system"`).
        unsafe {
            let library = libloading::Library::new(LOADER_LIBRARY)?;
            macro_rules! symbol {
                ($name:literal) => {
                    *library.get($name)?
                };
            }
            Ok(Library {
                get_platform_ids: symbol!(b"clGetPlatformIDs\0"),
                get_device_ids: symbol!(b"clGetDeviceIDs\0"),
                get_device_info: symbol!(b"clGetDeviceInfo\0"),
                create_context: symbol!(b"clCreateContext\0"),
                create_command_queue: symbol!(b"clCreateCommandQueue\0"),
                get_command_queue_info: symbol!(b"clGetCommandQueueInfo\0"),
                create_program_with_source: symbol!(b"clCreateProgramWithSource\0"),
                build_program: symbol!(b"clBuildProgram\0"),
                get_program_build_info: symbol!(b"clGetProgramBuildInfo\0"),
                create_kernel: symbol!(b"clCreateKernel\0"),
                set_kernel_arg: symbol!(b"clSetKernelArg\0"),
                enqueue_nd_range_kernel: symbol!(b"clEnqueueNDRangeKernel\0"),
                create_buffer: symbol!(b"clCreateBuffer\0"),
                get_mem_object_info: symbol!(b"clGetMemObjectInfo\0"),
                enqueue_write_buffer: symbol!(b"clEnqueueWriteBuffer\0"),
                enqueue_read_buffer: symbol!(b"clEnqueueReadBuffer\0"),
                finish: symbol!(b"clFinish\0"),
                release_mem_object: symbol!(b"clReleaseMemObject\0"),
                release_kernel: symbol!(b"clReleaseKernel\0"),
                release_program: symbol!(b"clReleaseProgram\0"),
                release_command_queue: symbol!(b"clReleaseCommandQueue\0"),
                release_context: symbol!(b"clReleaseContext\0"),
                _library: library,
            })
        }
    }

    /// The context of `queue`, the key under which a kernel is kept.
    ///
    /// # Safety
    /// `queue` is a live command queue.
    pub unsafe fn queue_context(&self, queue: ClQueue) -> Result<ClContext, String> {
        let mut context: ClContext = std::ptr::null_mut();
        // SAFETY: the caller's contract; the value is a pointer, written in place.
        let code = unsafe {
            (self.get_command_queue_info)(
                queue,
                CL_QUEUE_CONTEXT,
                size_of::<ClContext>(),
                (&raw mut context).cast(),
                std::ptr::null_mut(),
            )
        };
        check(code, "clGetCommandQueueInfo")?;
        Ok(context)
    }

    /// # Safety
    /// `queue` is a live command queue.
    unsafe fn queue_device(&self, queue: ClQueue) -> Result<ClDevice, String> {
        let mut device: ClDevice = std::ptr::null_mut();
        // SAFETY: the caller's contract; the value is a pointer, written in place.
        let code = unsafe {
            (self.get_command_queue_info)(
                queue,
                CL_QUEUE_DEVICE,
                size_of::<ClDevice>(),
                (&raw mut device).cast(),
                std::ptr::null_mut(),
            )
        };
        check(code, "clGetCommandQueueInfo")?;
        Ok(device)
    }

    /// The size in bytes of `buffer`.
    ///
    /// # Safety
    /// `buffer` is a live memory object.
    unsafe fn size_of_buffer(&self, buffer: ClMem) -> Result<usize, String> {
        let mut size = 0usize;
        // SAFETY: the caller's contract; `CL_MEM_SIZE` is a `size_t`.
        let code = unsafe {
            (self.get_mem_object_info)(
                buffer,
                CL_MEM_SIZE,
                size_of::<usize>(),
                (&raw mut size).cast(),
                std::ptr::null_mut(),
            )
        };
        check(code, "clGetMemObjectInfo")?;
        Ok(size)
    }

    /// Every GPU of every platform, for the tests. None is an empty list, not an error.
    pub fn gpus(&self) -> Result<Vec<Device>, String> {
        let mut count: ClUint = 0;
        // SAFETY: asks the count only.
        let code = unsafe { (self.get_platform_ids)(0, std::ptr::null_mut(), &mut count) };
        if code == CL_PLATFORM_NOT_FOUND_KHR {
            return Ok(Vec::new());
        }
        check(code, "clGetPlatformIDs")?;
        let mut platforms = vec![std::ptr::null_mut(); count as usize];
        // SAFETY: `platforms` holds `count` entries.
        check(
            unsafe { (self.get_platform_ids)(count, platforms.as_mut_ptr(), std::ptr::null_mut()) },
            "clGetPlatformIDs",
        )?;
        let mut gpus = Vec::new();
        for platform in platforms {
            let mut count: ClUint = 0;
            // SAFETY: asks the count only.
            let code = unsafe {
                (self.get_device_ids)(
                    platform,
                    CL_DEVICE_TYPE_GPU,
                    0,
                    std::ptr::null_mut(),
                    &mut count,
                )
            };
            if code == CL_DEVICE_NOT_FOUND {
                continue;
            }
            check(code, "clGetDeviceIDs")?;
            let mut devices = vec![std::ptr::null_mut(); count as usize];
            // SAFETY: `devices` holds `count` entries.
            check(
                unsafe {
                    (self.get_device_ids)(
                        platform,
                        CL_DEVICE_TYPE_GPU,
                        count,
                        devices.as_mut_ptr(),
                        std::ptr::null_mut(),
                    )
                },
                "clGetDeviceIDs",
            )?;
            for id in devices {
                gpus.push(Device {
                    id,
                    name: self.device_name(id)?,
                });
            }
        }
        Ok(gpus)
    }

    fn device_name(&self, device: ClDevice) -> Result<String, String> {
        let mut size = 0usize;
        // SAFETY: asks the size only; `device` came from clGetDeviceIDs.
        check(
            unsafe {
                (self.get_device_info)(device, CL_DEVICE_NAME, 0, std::ptr::null_mut(), &mut size)
            },
            "clGetDeviceInfo",
        )?;
        let mut name = vec![0u8; size];
        // SAFETY: `name` holds `size` bytes.
        check(
            unsafe {
                (self.get_device_info)(
                    device,
                    CL_DEVICE_NAME,
                    name.len(),
                    name.as_mut_ptr().cast(),
                    std::ptr::null_mut(),
                )
            },
            "clGetDeviceInfo",
        )?;
        Ok(CStr::from_bytes_until_nul(&name)
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default())
    }

    /// A buffer of `bytes` in `context`, filled from `contents` when given.
    ///
    /// # Safety
    /// `context` is a live context.
    unsafe fn buffer(
        &'static self,
        context: ClContext,
        flags: ClBitfield,
        bytes: usize,
        contents: Option<*const c_void>,
    ) -> Result<Buffer, String> {
        let flags = flags | contents.map_or(0, |_| CL_MEM_COPY_HOST_PTR);
        let mut code = CL_SUCCESS;
        // SAFETY: the caller's contract; a host pointer is only read, `bytes` of it.
        let mem = unsafe {
            (self.create_buffer)(
                context,
                flags,
                bytes.max(1),
                contents.unwrap_or(std::ptr::null()).cast_mut(),
                &mut code,
            )
        };
        check(code, "clCreateBuffer")?;
        Ok(Buffer { library: self, mem })
    }
}

/// A memory object this module created, released on drop.
pub struct Buffer {
    library: &'static Library,
    mem: ClMem,
}

impl Buffer {
    pub fn mem(&self) -> ClMem {
        self.mem
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        // SAFETY: the object was created here and is released once.
        unsafe { (self.library.release_mem_object)(self.mem) };
    }
}

/// A context and a queue on one device, for the tests, which have no host to hand one over.
pub struct TestQueue {
    library: &'static Library,
    context: ClContext,
    queue: ClQueue,
}

impl TestQueue {
    pub fn new(library: &'static Library, device: &Device) -> Result<TestQueue, String> {
        let mut code = CL_SUCCESS;
        // SAFETY: one device from clGetDeviceIDs, no properties and no callback.
        let context = unsafe {
            (library.create_context)(
                std::ptr::null(),
                1,
                &device.id,
                std::ptr::null(),
                std::ptr::null_mut(),
                &mut code,
            )
        };
        check(code, "clCreateContext")?;
        // SAFETY: the context was just created for this device.
        let queue = unsafe { (library.create_command_queue)(context, device.id, 0, &mut code) };
        if let Err(err) = check(code, "clCreateCommandQueue") {
            // SAFETY: the context is used by nothing else.
            unsafe { (library.release_context)(context) };
            return Err(err);
        }
        Ok(TestQueue {
            library,
            context,
            queue,
        })
    }

    pub fn queue(&self) -> ClQueue {
        self.queue
    }

    /// A read-write buffer of `bytes`, as the host's are.
    pub fn alloc(&self, bytes: usize) -> Result<Buffer, String> {
        // SAFETY: the context lives as long as `self`.
        unsafe {
            self.library
                .buffer(self.context, CL_MEM_READ_WRITE, bytes, None)
        }
    }

    /// Writes `from` at the start of `to`, waiting for the copy.
    pub fn upload<T: Copy>(&self, to: &Buffer, from: &[T]) -> Result<(), String> {
        let bytes = size_of_val(from);
        // SAFETY: `to` is a live buffer of this context.
        if unsafe { self.library.size_of_buffer(to.mem) }? < bytes {
            return Err("the buffer is smaller than the data".to_string());
        }
        // SAFETY: a blocking write of `bytes`, which `to` holds and `from` is readable for.
        check(
            unsafe {
                (self.library.enqueue_write_buffer)(
                    self.queue,
                    to.mem,
                    CL_TRUE,
                    0,
                    bytes,
                    from.as_ptr().cast(),
                    0,
                    std::ptr::null(),
                    std::ptr::null_mut(),
                )
            },
            "clEnqueueWriteBuffer",
        )
    }

    /// Reads the start of `from` into `to`, after everything queued before.
    pub fn download<T: Copy>(&self, to: &mut [T], from: &Buffer) -> Result<(), String> {
        let bytes = size_of_val(to);
        // SAFETY: `from` is a live buffer of this context.
        if unsafe { self.library.size_of_buffer(from.mem) }? < bytes {
            return Err("the buffer is smaller than the data".to_string());
        }
        // SAFETY: a blocking read of `bytes`, which `from` holds and `to` is writable for; the
        // queue is in order, so the read comes after the kernels enqueued before.
        check(
            unsafe {
                (self.library.enqueue_read_buffer)(
                    self.queue,
                    from.mem,
                    CL_TRUE,
                    0,
                    bytes,
                    to.as_mut_ptr().cast(),
                    0,
                    std::ptr::null(),
                    std::ptr::null_mut(),
                )
            },
            "clEnqueueReadBuffer",
        )
    }

    /// Waits for everything queued.
    pub fn finish(&self) -> Result<(), String> {
        // SAFETY: the queue lives as long as `self`.
        check(unsafe { (self.library.finish)(self.queue) }, "clFinish")
    }
}

impl Drop for TestQueue {
    fn drop(&mut self) {
        // SAFETY: both were created here and are released once, the queue first.
        unsafe {
            (self.library.finish)(self.queue);
            (self.library.release_command_queue)(self.queue);
            (self.library.release_context)(self.context);
        }
    }
}

/// The warp kernel built in one context, and the column map uploaded to it.
pub struct Warp {
    library: &'static Library,
    context: ClContext,
    program: ClProgram,
    kernel: ClKernel,
    map: Option<Buffer>,
    map_len: usize,
}

impl Warp {
    /// Builds the kernel in the context of `queue`, for its device.
    ///
    /// # Safety
    /// `queue` is a live command queue.
    pub unsafe fn new(library: &'static Library, queue: ClQueue) -> Result<Warp, String> {
        // SAFETY: the caller's contract.
        let (context, device) =
            unsafe { (library.queue_context(queue)?, library.queue_device(queue)?) };
        let mut code = CL_SUCCESS;
        let source = WARP_SOURCE.as_ptr();
        // SAFETY: one NUL-terminated string (a null length array means NUL-terminated).
        let program = unsafe {
            (library.create_program_with_source)(context, 1, &source, std::ptr::null(), &mut code)
        };
        check(code, "clCreateProgramWithSource")?;
        // SAFETY: the program was just created, for this device of its context.
        let built = unsafe {
            (library.build_program)(
                program,
                1,
                &device,
                c"".as_ptr(),
                std::ptr::null(),
                std::ptr::null_mut(),
            )
        };
        if let Err(err) = check(built, "clBuildProgram") {
            // SAFETY: the program is live, of this device's context, and used by nothing else.
            let log = unsafe {
                let log = build_log(library, program, device);
                (library.release_program)(program);
                log
            };
            return Err(format!("{err} {log}").trim_end().to_string());
        }
        // SAFETY: the program is built; the name is NUL-terminated.
        let kernel = unsafe { (library.create_kernel)(program, KERNEL_NAME.as_ptr(), &mut code) };
        if let Err(err) = check(code, "clCreateKernel") {
            // SAFETY: the program is live and used by nothing else.
            unsafe { (library.release_program)(program) };
            return Err(err);
        }
        Ok(Warp {
            library,
            context,
            program,
            kernel,
            map: None,
            map_len: 0,
        })
    }

    /// Uploads the column map, every entry below its length.
    pub fn set_map(&mut self, map: &[usize]) -> Result<(), String> {
        let columns: Vec<u32> = map
            .iter()
            .map(|&from| u32::try_from(from).map_err(|_| "a column beyond u32".to_string()))
            .collect::<Result<_, _>>()?;
        if map.iter().any(|&from| from >= map.len()) {
            return Err("the map points outside the row".to_string());
        }
        // The old map goes before the new one is made; a map is small, so it is made anew rather
        // than written through a queue.
        self.map = None;
        self.map_len = 0;
        // SAFETY: the context lives as long as the program built in it; `columns` is read for its
        // whole length.
        self.map = Some(unsafe {
            self.library.buffer(
                self.context,
                CL_MEM_READ_ONLY,
                size_of_val(columns.as_slice()),
                Some(columns.as_ptr().cast()),
            )
        }?);
        self.map_len = columns.len();
        Ok(())
    }

    /// Enqueues the warp of rows `first_row..first_row + rows` of `map`'s width on `queue`,
    /// without waiting: the host waits for its queue.
    ///
    /// # Safety
    /// `queue` is a live queue of the context the warp was built in; `src` and `dst` are live
    /// buffers of that context; nothing else writes `dst` until the queue has run the kernel.
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn launch(
        &self,
        src: ClMem,
        dst: ClMem,
        first_row: usize,
        rows: usize,
        src_pitch: usize,
        dst_pitch: usize,
        queue: ClQueue,
    ) -> Result<(), String> {
        let Some(map) = &self.map else {
            return Ok(());
        };
        if rows == 0 {
            return Ok(());
        }
        let width = self.map_len;
        // SAFETY: the caller's contract.
        let (src_size, dst_size) = unsafe {
            (
                self.library.size_of_buffer(src)?,
                self.library.size_of_buffer(dst)?,
            )
        };
        check_buffer(width, first_row, rows, src_pitch, src_size)?;
        check_buffer(width, first_row, rows, dst_pitch, dst_size)?;

        let width_arg = ClUint::try_from(width).map_err(|_| "frame too wide")?;
        let first_arg = ClUint::try_from(first_row).map_err(|_| "frame too tall")?;
        let rows_arg = ClUint::try_from(rows).map_err(|_| "frame too tall")?;
        let (src_pitch, dst_pitch) = (src_pitch as u64, dst_pitch as u64);
        let map = map.mem;
        let args: [(usize, *const c_void); 8] = [
            (size_of::<ClMem>(), (&raw const src).cast()),
            (size_of::<ClMem>(), (&raw const dst).cast()),
            (size_of::<ClMem>(), (&raw const map).cast()),
            (size_of::<ClUint>(), (&raw const width_arg).cast()),
            (size_of::<ClUint>(), (&raw const first_arg).cast()),
            (size_of::<ClUint>(), (&raw const rows_arg).cast()),
            (size_of::<u64>(), (&raw const src_pitch).cast()),
            (size_of::<u64>(), (&raw const dst_pitch).cast()),
        ];
        for (index, (size, value)) in (0..).zip(args) {
            // SAFETY: the arguments match the kernel's, in order and in size; each value lives
            // until the call returns, which copies it.
            check(
                unsafe { (self.library.set_kernel_arg)(self.kernel, index, size, value) },
                "clSetKernelArg",
            )?;
        }
        // The local size is left to the driver, so the global size need not be a multiple of it.
        let global = [width, rows];
        // SAFETY: two dimensions, as given; the caller's contract covers the queue and buffers,
        // and the sizes were checked above against the rows the kernel touches.
        check(
            unsafe {
                (self.library.enqueue_nd_range_kernel)(
                    queue,
                    self.kernel,
                    2,
                    std::ptr::null(),
                    global.as_ptr(),
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    std::ptr::null_mut(),
                )
            },
            "clEnqueueNDRangeKernel",
        )
    }
}

/// The compiler's log of `program` for `device`, empty when there is none.
///
/// # Safety
/// `program` is a live program of `device`'s context.
unsafe fn build_log(library: &Library, program: ClProgram, device: ClDevice) -> String {
    let mut size = 0usize;
    // SAFETY: the caller's contract; asks the size only.
    let asked = unsafe {
        (library.get_program_build_info)(
            program,
            device,
            CL_PROGRAM_BUILD_LOG,
            0,
            std::ptr::null_mut(),
            &mut size,
        )
    };
    if asked != CL_SUCCESS || size == 0 {
        return String::new();
    }
    let mut log = vec![0u8; size];
    // SAFETY: `log` holds `size` bytes.
    let read = unsafe {
        (library.get_program_build_info)(
            program,
            device,
            CL_PROGRAM_BUILD_LOG,
            log.len(),
            log.as_mut_ptr().cast(),
            std::ptr::null_mut(),
        )
    };
    if read != CL_SUCCESS {
        return String::new();
    }
    CStr::from_bytes_until_nul(&log)
        .map(|log| log.to_string_lossy().trim().to_string())
        .unwrap_or_default()
}

impl Drop for Warp {
    fn drop(&mut self) {
        self.map = None;
        // SAFETY: both were created here and are released once; OpenCL objects are reference
        // counted, so this is safe on any thread and after the host has released its context.
        unsafe {
            (self.library.release_kernel)(self.kernel);
            (self.library.release_program)(self.program);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{PIXEL_BYTES, check_buffer, error};

    const ROW: usize = 1920 * PIXEL_BYTES;

    #[test]
    fn a_buffer_larger_than_the_frame_is_accepted() {
        // Resolve's 4K buffers are 128 MiB, a little more than the frame.
        assert!(check_buffer(3840, 0, 2160, 3840 * 16, 0x800_0000).is_ok());
        assert!(check_buffer(1920, 0, 1080, ROW, 1080 * ROW).is_ok());
    }

    #[test]
    fn a_band_needs_the_rows_below_it_too() {
        assert!(check_buffer(1920, 100, 500, ROW, 600 * ROW).is_ok());
        let err = check_buffer(1920, 100, 500, ROW, 600 * ROW - 1).unwrap_err();
        assert!(err.contains("600 rows"), "{err}");
    }

    #[test]
    fn a_row_too_short_or_misaligned_is_refused() {
        assert!(check_buffer(1920, 0, 1, ROW - 16, ROW).is_err());
        assert!(check_buffer(1920, 0, 1, ROW + 4, 2 * ROW).is_err());
        assert!(check_buffer(1920, 0, 1, ROW + 16, 2 * ROW).is_ok());
    }

    #[test]
    fn a_size_beyond_the_address_space_is_refused() {
        assert!(check_buffer(1, usize::MAX, 1, 16, usize::MAX).is_err());
    }

    #[test]
    fn errors_name_the_call_and_the_code() {
        assert_eq!(
            error("clBuildProgram", -11),
            "clBuildProgram: CL_BUILD_PROGRAM_FAILURE"
        );
        assert_eq!(error("clFinish", -9999), "clFinish: error -9999");
    }
}
