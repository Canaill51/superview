//! The OpenCL warp against the CPU one, on every GPU of the machine.
//!
//! Skipped where there is no OpenCL loader or GPU, as on CI, unless `SUPERVIEW_REQUIRE_OPENCL=1`,
//! where that absence fails the test: on a machine with a GPU, set it, or a green run can mean the
//! kernel never ran. Every GPU the loader lists is tried, so one run covers an NVIDIA card and an
//! Intel iGPU side by side.

use superview_ofx::opencl::{Buffer, Device, Library, PIXEL_BYTES, TestQueue, Warp};
use superview_ofx::warp::{Mode, column_map, warp_row};

fn gpus() -> Option<(&'static Library, Vec<Device>)> {
    let require = std::env::var("SUPERVIEW_REQUIRE_OPENCL").as_deref() == Ok("1");
    let Some(library) = Library::get() else {
        assert!(
            !require,
            "no OpenCL loader although SUPERVIEW_REQUIRE_OPENCL=1"
        );
        eprintln!("skipped: no OpenCL loader on this machine");
        return None;
    };
    match library.gpus() {
        Ok(gpus) if !gpus.is_empty() => Some((library, gpus)),
        listed => {
            let why = listed.err().unwrap_or_else(|| "no OpenCL GPU".to_string());
            assert!(
                !require,
                "no usable GPU although SUPERVIEW_REQUIRE_OPENCL=1: {why}"
            );
            eprintln!("skipped: {why}");
            None
        }
    }
}

/// A frame of RGBA float pixels, every channel distinct, each row `pitch` floats long.
fn frame(height: usize, pitch: usize) -> Vec<f32> {
    (0..height * pitch)
        .map(|i| {
            let (y, rest) = (i / pitch, i % pitch);
            (y * 100_000 + rest) as f32
        })
        .collect()
}

/// A buffer holding `values`, with `spare` bytes more, as the host's pool hands out.
fn buffer(queue: &TestQueue, values: &[f32], spare: usize) -> Buffer {
    let buffer = queue.alloc(size_of_val(values) + spare).unwrap();
    queue.upload(&buffer, values).unwrap();
    buffer
}

/// Runs the OpenCL warp of rows `first..first + rows` and returns the whole output frame, whose
/// other rows keep the value -1.
fn on_gpu(
    library: &'static Library,
    queue: &TestQueue,
    (width, height, pitch, mode, first, rows): Case,
) -> Vec<f32> {
    // SAFETY: the queue lives for the whole function.
    let mut warp = unsafe { Warp::new(library, queue.queue()) }.expect("the kernel builds");
    warp.set_map(&column_map(width, height, mode)).unwrap();

    let source = frame(height, pitch);
    let mut output = vec![-1.0f32; height * pitch];
    let (src, dst) = (buffer(queue, &source, 4096), buffer(queue, &output, 4096));
    let row_bytes = pitch * 4;
    // SAFETY: both buffers are of the queue's context, and nothing else writes them.
    unsafe {
        warp.launch(
            src.mem(),
            dst.mem(),
            first,
            rows,
            row_bytes,
            row_bytes,
            queue.queue(),
        )
    }
    .unwrap();
    queue.finish().unwrap();
    queue.download(&mut output, &dst).unwrap();
    output
}

/// The same with the CPU warp, pixel by pixel.
fn on_cpu((width, height, pitch, mode, first, rows): Case) -> Vec<f32> {
    let map = column_map(width, height, mode);
    let source = frame(height, pitch);
    let mut output = vec![-1.0f32; height * pitch];
    for y in first..first + rows {
        let row = y * pitch;
        let (src, _) = source[row..row + width * 4].as_chunks::<4>();
        let mut dst = vec![[0f32; 4]; width];
        assert!(warp_row(src, &mut dst, &map));
        output[row..row + width * 4].copy_from_slice(dst.as_flattened());
    }
    output
}

/// (width, height, pitch in floats, mode, first row, rows)
type Case = (usize, usize, usize, Mode, usize, usize);

#[test]
fn the_opencl_warp_gives_the_pixels_of_the_cpu_warp_on_every_gpu() {
    let Some((library, gpus)) = gpus() else {
        return;
    };
    assert_eq!(PIXEL_BYTES, 16);
    let cases: [Case; 6] = [
        (1920, 1080, 1920 * 4, Mode::Widen, 0, 1080),
        (3840, 2160, 3840 * 4, Mode::Widen, 0, 2160),
        // Rows longer than the picture, as a host may pad them; a band of rows only.
        (1920, 1080, 2048 * 4, Mode::Widen, 100, 500),
        (1920, 1080, 1920 * 4, Mode::Unsqueeze, 0, 1080),
        // An odd width, and no warp (identity).
        (1921, 1080, 1921 * 4, Mode::Widen, 0, 1080),
        (1440, 1080, 1440 * 4, Mode::Widen, 0, 1080),
    ];
    // Every device is tried before failing, so a wrong kernel shows on each of them.
    let mut wrong = Vec::new();
    for device in &gpus {
        eprintln!("OpenCL device: {}", device.name);
        let queue = TestQueue::new(library, device).unwrap();
        for case in cases {
            let gpu = on_gpu(library, &queue, case);
            let cpu = on_cpu(case);
            let differ = gpu.iter().zip(&cpu).filter(|(g, c)| g != c).count();
            if differ != 0 {
                wrong.push(format!("{}: {case:?}: {differ} values differ", device.name));
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn a_map_pointing_outside_the_row_is_refused() {
    let Some((library, gpus)) = gpus() else {
        return;
    };
    let queue = TestQueue::new(library, &gpus[0]).unwrap();
    // SAFETY: the queue lives for the whole test.
    let mut warp = unsafe { Warp::new(library, queue.queue()) }.unwrap();
    assert!(warp.set_map(&[0, 1, 2]).is_ok());
    assert!(warp.set_map(&[0, 3, 2]).is_err());
}

#[test]
fn a_buffer_too_small_for_the_band_is_refused_before_the_kernel_runs() {
    let Some((library, gpus)) = gpus() else {
        return;
    };
    let queue = TestQueue::new(library, &gpus[0]).unwrap();
    // SAFETY: the queue lives for the whole test.
    let mut warp = unsafe { Warp::new(library, queue.queue()) }.unwrap();
    let (width, height) = (64, 32);
    warp.set_map(&column_map(width, height, Mode::Widen))
        .unwrap();
    let row_bytes = width * PIXEL_BYTES;
    let full = queue.alloc(height * row_bytes).unwrap();
    let short = queue.alloc(height * row_bytes - 1).unwrap();
    let launch = |src: &Buffer, dst: &Buffer, first, rows, pitch| {
        // SAFETY: the buffers are of the queue's context, and nothing else writes them.
        unsafe {
            warp.launch(
                src.mem(),
                dst.mem(),
                first,
                rows,
                pitch,
                pitch,
                queue.queue(),
            )
        }
    };
    assert!(launch(&full, &full, 0, height, row_bytes).is_ok());
    assert!(launch(&full, &short, 0, height, row_bytes).is_err());
    assert!(launch(&short, &full, 0, height, row_bytes).is_err());
    assert!(launch(&full, &full, 1, height, row_bytes).is_err());
    assert!(launch(&full, &full, 0, height, row_bytes - PIXEL_BYTES).is_err());
    queue.finish().unwrap();
}
