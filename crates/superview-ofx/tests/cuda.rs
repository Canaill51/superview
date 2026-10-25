//! The CUDA warp against the CPU one, on a real GPU (DEC-66).
//!
//! Skipped where there is no CUDA driver or GPU, as on CI, unless `SUPERVIEW_REQUIRE_CUDA=1`,
//! where that absence fails the test: on a machine with an NVIDIA GPU, set it, or a green run
//! can mean the kernel never ran.

use superview_ofx::cuda::{Driver, PIXEL_BYTES, Warp};
use superview_ofx::warp::{Mode, column_map, warp_row};

fn gpu() -> Option<&'static Driver> {
    let require = std::env::var("SUPERVIEW_REQUIRE_CUDA").as_deref() == Ok("1");
    let Some(driver) = Driver::get() else {
        assert!(!require, "no CUDA driver although SUPERVIEW_REQUIRE_CUDA=1");
        eprintln!("skipped: no CUDA driver on this machine");
        return None;
    };
    match driver.use_first_device() {
        Ok(_) => Some(driver),
        Err(err) => {
            assert!(
                !require,
                "no usable GPU although SUPERVIEW_REQUIRE_CUDA=1: {err}"
            );
            eprintln!("skipped: {err}");
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

/// Runs the CUDA warp of rows `first..first + rows` and returns the whole output frame, whose
/// other rows keep the value -1.
fn on_gpu(
    driver: &'static Driver,
    width: usize,
    height: usize,
    pitch: usize,
    mode: Mode,
    first: usize,
    rows: usize,
) -> Vec<f32> {
    let context = driver
        .current_context()
        .unwrap()
        .expect("a current context");
    let mut warp = Warp::new(driver, context).expect("the PTX loads");
    warp.set_map(&column_map(width, height, mode)).unwrap();

    let source = frame(height, pitch);
    let mut output = vec![-1.0f32; height * pitch];
    let bytes = size_of_val(source.as_slice());
    let row_bytes = pitch * 4;
    let (src, dst) = (driver.alloc(bytes).unwrap(), driver.alloc(bytes).unwrap());
    // SAFETY: both buffers hold `bytes`; rows `first..first + rows` are inside them.
    unsafe {
        driver.upload(src, &source).unwrap();
        driver.upload(dst, &output).unwrap();
        let offset = (first * row_bytes) as u64;
        warp.launch(
            src + offset,
            dst + offset,
            rows,
            row_bytes,
            row_bytes,
            std::ptr::null_mut(),
        )
        .unwrap();
        driver.synchronize(std::ptr::null_mut()).unwrap();
        driver.download(&mut output, dst).unwrap();
        driver.free(src).unwrap();
        driver.free(dst).unwrap();
    }
    output
}

/// The same with the CPU warp, pixel by pixel.
fn on_cpu(
    width: usize,
    height: usize,
    pitch: usize,
    mode: Mode,
    first: usize,
    rows: usize,
) -> Vec<f32> {
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

#[test]
fn the_gpu_warp_gives_the_pixels_of_the_cpu_warp() {
    let Some(driver) = gpu() else { return };
    assert_eq!(PIXEL_BYTES, 16);
    // (width, height, pitch in floats, mode, first row, rows)
    let cases = [
        (1920, 1080, 1920 * 4, Mode::Widen, 0, 1080),
        (3840, 2160, 3840 * 4, Mode::Widen, 0, 2160),
        // Rows longer than the picture, as a host may pad them; a band of rows only.
        (1920, 1080, 2048 * 4, Mode::Widen, 100, 500),
        (1920, 1080, 1920 * 4, Mode::Unsqueeze, 0, 1080),
        // A width that is not a multiple of the block, and no warp (identity).
        (1921, 1080, 1921 * 4, Mode::Widen, 0, 1080),
        (1440, 1080, 1440 * 4, Mode::Widen, 0, 1080),
    ];
    for (width, height, pitch, mode, first, rows) in cases {
        let gpu = on_gpu(driver, width, height, pitch, mode, first, rows);
        let cpu = on_cpu(width, height, pitch, mode, first, rows);
        let differ = gpu.iter().zip(&cpu).filter(|(g, c)| g != c).count();
        assert_eq!(
            differ, 0,
            "{width}x{height} pitch {pitch} {mode:?} rows {first}+{rows}: {differ} values differ"
        );
    }
}

#[test]
fn a_map_pointing_outside_the_row_is_refused() {
    let Some(driver) = gpu() else { return };
    let context = driver.current_context().unwrap().unwrap();
    let mut warp = Warp::new(driver, context).unwrap();
    assert!(warp.set_map(&[0, 1, 2]).is_ok());
    assert!(warp.set_map(&[0, 3, 2]).is_err());
}
