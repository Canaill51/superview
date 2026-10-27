//! The glue between Resolve and [`crate::warp`]: describe the effect, then warp each row.
//!
//! In Resolve the effect receives the whole timeline frame with the clip already scaled to fit
//! (set *Mismatched resolution* to *Scale entire image to fit*), so source and output have the
//! same bounds and the effect writes every column.
//!
//! Three paths render the same pixels: CUDA when the host hands over device memory
//! ([`crate::cuda`], DEC-66), OpenCL when it hands over OpenCL buffers ([`crate::opencl`], DEC-68),
//! the CPU otherwise. The host picks, from the mode the user set in Resolve. A GPU frame that
//! fails returns `FAILED`, and the host then asks for the same frame on the CPU.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex, PoisonError};

use ofx::*;
use ofx_sys::OfxPropertySuiteV1;

use superview_core::logfile::user_cache_dir;

use crate::cuda::{self, CuContext, CuDevicePtr, CuStream, Driver};
use crate::gpu::PerContext;
use crate::journal::{self, PathLog, RenderPath};
use crate::opencl::{self, ClContext, ClMem, ClQueue, Library};
use crate::warp::{Mode, column_map, warp_row};

plugin_module!(
    "io.github.canaill51.superview",
    ApiVersion(1),
    PluginVersion(1, 0),
    SuperviewPlugin::default
);

const PARAM_MODE: &str = "mode";

/// Properties the fork does not name (OpenFX 1.4, `ofxGPURender.h`).
const CUDA_STREAM_SUPPORTED: &str = "OfxImageEffectPropCudaStreamSupported";
const CUDA_STREAM: &str = "OfxImageEffectPropCudaStream";

/// Threads of the CPU path. Measured in Resolve on a 4K float frame (i7-11800H): 18 ms on one
/// thread, 14 ms on two, no better beyond -- the warp is bound by memory -- and more threads
/// starve the host's own decoding.
const CPU_THREADS: usize = 2;

#[derive(Default)]
struct SuperviewPlugin;

struct InstanceData {
    source_clip: ClipInstance,
    output_clip: ClipInstance,
    mode: ParamHandle<Int>,
    /// `FullySafe` lets the host render several frames of one instance at once: what they share
    /// and change sits behind a lock, held to swap the map or to enqueue the kernel, no longer.
    state: Mutex<RenderState>,
}

#[derive(Default)]
struct RenderState {
    /// The map of the last frame: the size and mode rarely change during playback.
    map: Option<CachedMap>,
    /// The kernel and the map on the GPU, once a CUDA frame has come, or why the kernel did not load.
    cuda: PerContext<CuContext, GpuWarp<cuda::Warp>>,
    /// The same for OpenCL frames. The lock also covers the kernel's arguments, which OpenCL does
    /// not let two threads set at once on one kernel.
    opencl: PerContext<ClContext, GpuWarp<opencl::Warp>>,
    /// What this instance has already said in the log.
    paths: PathLog,
}

struct CachedMap {
    key: MapKey,
    columns: Arc<[usize]>,
}

/// What a column map depends on.
type MapKey = (usize, usize, Mode);

struct GpuWarp<W> {
    warp: W,
    /// The map on the GPU, `None` until uploaded.
    key: Option<MapKey>,
}

/// Raw access to a property set, for the properties the fork does not name.
#[derive(Clone)]
struct RawProps {
    handle: OfxPropertySetHandle,
    suite: *const OfxPropertySuiteV1,
}

impl RawProps {
    fn of(set: &impl AsProperties) -> RawProps {
        RawProps {
            handle: set.handle(),
            suite: set.suite(),
        }
    }
}

impl AsProperties for RawProps {
    fn handle(&self) -> OfxPropertySetHandle {
        self.handle
    }
    fn suite(&self) -> *const OfxPropertySuiteV1 {
        self.suite
    }
}
impl RawReadable for RawProps {}
impl RawWritable for RawProps {}

impl Execute for SuperviewPlugin {
    /// A panic must not cross into the host: it would take Resolve down with it.
    fn execute(&mut self, plugin_context: &PluginContext, action: &mut Action) -> Result<Int> {
        match catch_unwind(AssertUnwindSafe(|| run(plugin_context, action))) {
            Ok(result) => result,
            Err(_) => {
                log::error!("Superview: the effect panicked; the frame is left as it was");
                FAILED
            }
        }
    }
}

fn run(plugin_context: &PluginContext, action: &mut Action) -> Result<Int> {
    use Action::*;
    match *action {
        Load => {
            let env = |key: &str| std::env::var(key).ok();
            if journal::install(user_cache_dir(&env).as_deref()).is_some() {
                log::info!("Superview: plugin loaded");
            }
            OK
        }

        Describe(ref mut effect) => {
            let mut props: EffectDescriptor = effect.properties()?;
            props.set_grouping("Superview")?;
            props.set_label("Superview")?;
            props.set_short_label("Superview")?;
            props.set_long_label("Superview: widen 4:3 to 16:9")?;
            props.set_supported_pixel_depths(&[
                BitDepth::Byte,
                BitDepth::Short,
                BitDepth::Float,
            ])?;
            props.set_supported_contexts(&[ImageEffectContext::Filter])?;
            props.set_supports_tiles(false)?;
            // The host then hands over source and output of the same size, which `Render` relies on.
            props.set_supports_multi_resolution(false)?;
            // As Gyroflow's plugin on the same fork. Measured in Resolve 21 (4K, CUDA): 8 fps with
            // `InstanceSafe`, 29.97 with this, although Resolve never ran two renders at once --
            // the declaration alone takes it off a slow path. `RenderState`'s lock covers the case.
            props.set_render_thread_safety(ImageEffectRender::FullySafe)?;
            // CUDA only where the driver loads: a host would otherwise ask for every frame in
            // CUDA first, then again on the CPU.
            let host_has_cuda = plugin_context
                .get_host()
                .get_cuda_render_supported()
                .is_ok_and(|supported| supported == "true");
            if host_has_cuda && Driver::get().is_some() {
                props.set_cuda_render_supported("true")?;
                RawProps::of(&props).set_raw(CUDA_STREAM_SUPPORTED, "true")?;
                log::info!("Superview: CUDA rendering offered to the host");
            } else if !host_has_cuda {
                log::info!(
                    "Superview: CUDA rendering not offered, the host does not propose it; \
                     frames render on the CPU"
                );
            } else {
                log::info!(
                    "Superview: CUDA rendering not offered, no CUDA driver ({}); frames render \
                     on the CPU",
                    Driver::load_error().unwrap_or("unknown reason")
                );
            }
            // OpenCL beside CUDA, on the same terms: the host uses one or the other, from the mode
            // set in its preferences (DEC-68).
            let host_has_opencl = plugin_context
                .get_host()
                .get_opencl_render_supported()
                .is_ok_and(|supported| supported == "true");
            if host_has_opencl && Library::get().is_some() {
                props.set_opencl_render_supported("true")?;
                log::info!("Superview: OpenCL rendering offered to the host");
            } else if !host_has_opencl {
                log::info!("Superview: OpenCL rendering not offered, the host does not propose it");
            } else {
                log::info!(
                    "Superview: OpenCL rendering not offered, no OpenCL loader ({}); frames render \
                     on the CPU",
                    Library::load_error().unwrap_or("unknown reason")
                );
            }
            OK
        }

        DescribeInContext(ref mut effect, _) => {
            effect
                .new_output_clip()?
                .set_supported_components(&[ImageComponent::RGBA])?;
            effect
                .new_simple_input_clip()?
                .set_supported_components(&[ImageComponent::RGBA])?;

            let mut params = effect.parameter_set()?;
            let mut mode = params.param_define_choice(PARAM_MODE)?;
            mode.set_label("Mode")?;
            mode.set_hint(
                "Widen: a 4:3 picture centred in a wider frame is stretched to fill it. \
                 Un-squeeze: the picture already fills the frame, stretched by the camera.",
            )?;
            mode.set_choices(&["Widen 4:3 to 16:9", "Un-squeeze"])?;
            mode.set_default(0)?;
            params
                .param_define_page("Main")?
                .set_children(&[PARAM_MODE])?;
            OK
        }

        CreateInstance(ref mut effect) => {
            let params = effect.parameter_set()?;
            let data = InstanceData {
                source_clip: effect.get_simple_input_clip()?,
                output_clip: effect.get_output_clip()?,
                mode: params.parameter(PARAM_MODE)?,
                state: Mutex::default(),
            };
            effect.set_instance_data(data)?;
            OK
        }

        DestroyInstance(_) => OK,

        Render(ref mut effect, ref in_args) => {
            let time = in_args.get_time()?;
            let window = in_args.get_render_window()?;
            let gpu_path = if in_args.get_cuda_enabled().unwrap_or(false) {
                Some(RenderPath::Cuda)
            } else if in_args.get_opencl_enabled().unwrap_or(false) {
                Some(RenderPath::OpenCl)
            } else {
                None
            };
            let data: &mut InstanceData = effect.get_instance_data()?;
            let mode = match data.mode.get_value_at_time(time)? {
                0 => Mode::Widen,
                _ => Mode::Unsqueeze,
            };

            let source = data.source_clip.get_image(time)?;
            let output = data.output_clip.get_image_mut(time)?;
            let mut output = output.borrow_mut();

            let bounds = output.get_bounds()?;
            let from = source.get_bounds()?;
            if (from.x1, from.y1, from.x2, from.y2) != (bounds.x1, bounds.y1, bounds.x2, bounds.y2)
            {
                log::error!("Superview: the source and output frames differ in size");
                return FAILED;
            }
            let width = (bounds.x2 - bounds.x1).max(0) as usize;
            let height = (bounds.y2 - bounds.y1).max(0) as usize;
            if width == 0 || height == 0 {
                return OK;
            }
            let key = (width, height, mode);
            let map: Arc<[usize]> = {
                let mut state = data.state.lock().unwrap_or_else(PoisonError::into_inner);
                match state.map.as_ref() {
                    Some(cached) if cached.key == key => cached.columns.clone(),
                    _ => {
                        let columns: Arc<[usize]> = column_map(width, height, mode).into();
                        state.map = Some(CachedMap {
                            key,
                            columns: columns.clone(),
                        });
                        columns
                    }
                }
            };
            let map: &[usize] = &map;
            let rows = window.y1.max(bounds.y1)..window.y2.min(bounds.y2);
            // The user scrolled on: the host no longer wants this frame, which is no error. Asked
            // once: the host's handle cannot leave this thread, and a frame takes milliseconds.
            if effect.abort()? {
                return OK;
            }

            if let Some(path) = gpu_path {
                let frame = DeviceFrame {
                    src: source.get_data()? as usize,
                    dst: output.get_data()? as usize,
                    src_pitch: source.get_row_bytes()?,
                    dst_pitch: output.get_row_bytes()?,
                    first_row: (rows.start - bounds.y1).max(0) as usize,
                    row_count: (rows.end - rows.start).max(0) as usize,
                    float_rgba: (output.get_pixel_depth()?, output.get_components()?)
                        == (BitDepth::Float, ImageComponent::RGBA),
                };
                let mut state = data.state.lock().unwrap_or_else(PoisonError::into_inner);
                let state = &mut *state;
                let (api, rendered) = if path == RenderPath::Cuda {
                    let stream: CuStream = RawProps::of(in_args)
                        .get_raw::<VoidPtrMut, _>(CUDA_STREAM)
                        .unwrap_or(std::ptr::null_mut());
                    let rendered = render_on_cuda(&mut state.cuda, key, map, &frame, stream);
                    ("CUDA", rendered)
                } else {
                    let queue: ClQueue = in_args
                        .get_opencl_command_queue()
                        .unwrap_or(std::ptr::null_mut());
                    let rendered = render_on_opencl(&mut state.opencl, key, map, &frame, queue);
                    ("OpenCL", rendered)
                };
                return match rendered {
                    Ok(()) => {
                        if state.paths.first_on(path) {
                            log::info!(
                                "Superview: rendering on the GPU with {api} ({width}x{height})"
                            );
                        }
                        OK
                    }
                    Err(err) => {
                        // Once at warn: a card whose kernel never loads fails every frame.
                        if state.paths.first_failure(path) {
                            log::warn!(
                                "Superview: {api} render failed, the host falls back to the CPU: \
                                 {err}"
                            );
                        } else {
                            log::debug!("Superview: {api} render failed again: {err}");
                        }
                        FAILED
                    }
                };
            }

            let threads = CPU_THREADS.min(height);
            // Each band of rows goes to its own thread; a tile owns its rows, so no two threads
            // write the same pixel. A band that panics fails the frame, as a wrong row does.
            macro_rules! warp {
                ($pixel:ty) => {{
                    let src = source.get_descriptor::<$pixel>()?;
                    let tiles = output.get_tiles_mut::<$pixel>(threads)?;
                    let src = &src;
                    let rows = &rows;
                    let all_rows_fit = std::thread::scope(|scope| {
                        let bands: Vec<_> = tiles
                            .into_iter()
                            .map(|mut tile| {
                                // A tile counts its rows from the top of the frame, not from y1.
                                let band = (bounds.y1 + tile.y1).max(rows.start)
                                    ..(bounds.y1 + tile.y2).min(rows.end);
                                scope.spawn(move || {
                                    band.into_iter()
                                        .all(|y| warp_row(src.row(y), tile.row(y), map))
                                })
                            })
                            .collect();
                        bands.into_iter().all(|band| band.join().unwrap_or(false))
                    });
                    if !all_rows_fit {
                        log::error!("Superview: a row does not have the width of the frame");
                        return FAILED;
                    }
                }};
            }
            match (output.get_pixel_depth()?, output.get_components()?) {
                (BitDepth::Float, ImageComponent::RGBA) => warp!(RGBAColourF),
                (BitDepth::Short, ImageComponent::RGBA) => warp!(RGBAColourS),
                (BitDepth::Byte, ImageComponent::RGBA) => warp!(RGBAColourB),
                _ => return FAILED,
            }
            let first = data
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .paths
                .first_on(RenderPath::Cpu);
            if first {
                log::info!(
                    "Superview: rendering on the CPU, {threads} threads ({width}x{height}), \
                     as the host asked"
                );
            }
            OK
        }

        _ => REPLY_DEFAULT,
    }
}

/// The images of a GPU frame, as the host hands them over: device pointers in CUDA, `cl_mem`
/// handles in OpenCL. Rows `first_row..first_row + row_count` are rendered, counted from the
/// bottom of the frame.
struct DeviceFrame {
    src: usize,
    dst: usize,
    src_pitch: Int,
    dst_pitch: Int,
    first_row: usize,
    row_count: usize,
    float_rgba: bool,
}

/// Enqueues the warp of the frame's rows on the host's stream, or runs it to completion when the
/// host gave no stream.
fn render_on_cuda(
    gpu: &mut PerContext<CuContext, GpuWarp<cuda::Warp>>,
    key: MapKey,
    map: &[usize],
    frame: &DeviceFrame,
    stream: CuStream,
) -> std::result::Result<(), String> {
    let (first_row, row_count) = (frame.first_row, frame.row_count);
    let driver = Driver::get().ok_or("no CUDA driver")?;
    if !frame.float_rgba {
        return Err("CUDA frames other than RGBA float are not handled".to_string());
    }
    let pitch = |bytes: Int| {
        usize::try_from(bytes)
            .ok()
            .filter(|&bytes| bytes >= map.len() * cuda::PIXEL_BYTES && bytes.is_multiple_of(16))
            .ok_or_else(|| format!("unusable row length of {bytes} bytes"))
    };
    let (src_pitch, dst_pitch) = (pitch(frame.src_pitch)?, pitch(frame.dst_pitch)?);
    if frame.src == 0 || frame.dst == 0 || !(frame.src | frame.dst).is_multiple_of(16) {
        return Err("an image pointer is null or misaligned".to_string());
    }
    let (src, dst) = (frame.src as CuDevicePtr, frame.dst as CuDevicePtr);

    let context: CuContext = driver
        .current_context()?
        .ok_or("the host made no CUDA context current")?;
    // A new context (another GPU, a restarted engine): the old kernel and map go with it. A kernel
    // that did not load in this context is not compiled again on every frame.
    let gpu = gpu.get_or_load(context, || {
        Ok(GpuWarp {
            warp: cuda::Warp::new(driver, context)?,
            key: None,
        })
    })?;
    if gpu.key != Some(key) {
        gpu.key = None;
        gpu.warp.set_map(map)?;
        gpu.key = Some(key);
    }

    let src = src + (first_row * src_pitch) as CuDevicePtr;
    let dst = dst + (first_row * dst_pitch) as CuDevicePtr;
    // SAFETY: the host made `context` current and handed over these device images, each
    // `pitch` bytes a row (checked above against the map's width) and holding the frame's rows;
    // the map was set for this width just above.
    unsafe {
        gpu.warp
            .launch(src, dst, row_count, src_pitch, dst_pitch, stream)
    }?;
    if stream.is_null() {
        // Without a stream the host expects the work done on return (`ofxGPURender.h`).
        // SAFETY: the null stream, in the context the host made current.
        unsafe { driver.synchronize(stream) }?;
    }
    Ok(())
}

/// Enqueues the warp of the frame's rows on the host's OpenCL queue, without waiting: the host
/// waits for its queue. The buffers are checked against the rows in [`opencl::Warp::launch`].
fn render_on_opencl(
    gpu: &mut PerContext<ClContext, GpuWarp<opencl::Warp>>,
    key: MapKey,
    map: &[usize],
    frame: &DeviceFrame,
    queue: ClQueue,
) -> std::result::Result<(), String> {
    let library = Library::get().ok_or("no OpenCL loader")?;
    if !frame.float_rgba {
        return Err("OpenCL frames other than RGBA float are not handled".to_string());
    }
    // Unlike CUDA's null stream, OpenCL has no queue to fall back on.
    if queue.is_null() {
        return Err("the host gave no OpenCL command queue".to_string());
    }
    let pitch = |bytes: Int| {
        usize::try_from(bytes).map_err(|_| format!("unusable row length of {bytes} bytes"))
    };
    let (src_pitch, dst_pitch) = (pitch(frame.src_pitch)?, pitch(frame.dst_pitch)?);
    if frame.src == 0 || frame.dst == 0 {
        return Err("an image buffer is null".to_string());
    }
    let (src, dst) = (frame.src as ClMem, frame.dst as ClMem);

    // SAFETY: the host handed over this queue for this render.
    let context = unsafe { library.queue_context(queue) }?;
    // A new context: the old kernel and map go with it. A kernel that did not build in this
    // context is not compiled again on every frame, which the host would ask for (Lot 4a).
    let gpu = gpu.get_or_load(context, || {
        Ok(GpuWarp {
            // SAFETY: as above.
            warp: unsafe { opencl::Warp::new(library, queue) }?,
            key: None,
        })
    })?;
    if gpu.key != Some(key) {
        gpu.key = None;
        gpu.warp.set_map(map)?;
        gpu.key = Some(key);
    }
    // SAFETY: the host handed over the queue and these buffers of its context for this render,
    // and writes `dst` only after its queue has run; the warp was built in the queue's context and
    // its map set for this width just above.
    unsafe {
        gpu.warp.launch(
            src,
            dst,
            frame.first_row,
            frame.row_count,
            src_pitch,
            dst_pitch,
            queue,
        )
    }
}
