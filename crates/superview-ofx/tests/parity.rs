//! The plugin must draw what Superview's PGM maps make ffmpeg's `remap` draw.

use superview_core::pgm::{generate_pgm_bytes, remap_output_size};
use superview_core::video::{VideoSpecs, VideoStream};
use superview_ofx::warp::{Mode, column_map, warp_row};

fn video(width: i64, height: i64) -> VideoSpecs {
    VideoSpecs {
        file: "in.mp4".into(),
        streams: vec![VideoStream {
            codec: "h264".into(),
            width,
            height,
            duration_float: 1.0,
            bitrate_int: 1,
            ..VideoStream::default()
        }],
    }
}

/// The first row of an X map: the big-endian 16-bit samples after the P5 header.
fn first_row_of_x_map(pgm: &[u8], width: usize) -> Vec<i64> {
    let newlines: Vec<usize> = pgm
        .iter()
        .enumerate()
        .filter(|(_, b)| **b == b'\n')
        .map(|(i, _)| i)
        .collect();
    let body = &pgm[newlines[0] + 1..];
    (0..width)
        .map(|x| i64::from(u16::from_be_bytes([body[x * 2], body[x * 2 + 1]])))
        .collect()
}

/// The columns the application's map draws from, shifted by the bar. A sample outside the
/// picture is black in ffmpeg's `remap`, so it is a failure here, not something to clamp.
fn expected(source_width: i64, height: i64, squeeze: bool, pad: usize) -> Vec<usize> {
    let specs = video(source_width, height);
    let (x_map, _) = generate_pgm_bytes(&specs, squeeze).unwrap();
    let (out_width, _) = remap_output_size(&specs, squeeze);
    first_row_of_x_map(&x_map, out_width as usize)
        .into_iter()
        .map(|c| {
            assert!(
                (0..source_width).contains(&c),
                "{source_width}x{height}: the application's map leaves the picture ({c})"
            );
            c as usize + pad
        })
        .collect()
}

#[test]
fn widening_matches_the_application_map() {
    // 1920x1080 timeline, 1440x1080 picture centred between two 240-pixel bars.
    assert_eq!(remap_output_size(&video(1440, 1080), false).0, 1920);
    assert_eq!(
        column_map(1920, 1080, Mode::Widen),
        expected(1440, 1080, false, 240)
    );
}

#[test]
fn widening_matches_at_4k() {
    assert_eq!(
        column_map(3840, 2160, Mode::Widen),
        expected(2880, 2160, false, 480)
    );
}

#[test]
fn unsqueeze_matches_the_application_map() {
    assert_eq!(
        column_map(1920, 1080, Mode::Unsqueeze),
        expected(1920, 1080, true, 0)
    );
}

#[test]
fn a_timeline_that_is_not_wider_than_the_picture_is_left_alone() {
    let identity: Vec<usize> = (0..1440).collect();
    assert_eq!(column_map(1440, 1080, Mode::Widen), identity);
    assert_eq!(
        column_map(1000, 1080, Mode::Widen),
        (0..1000).collect::<Vec<_>>()
    );
}

#[test]
fn a_frame_without_height_or_too_wide_for_the_curve_is_left_alone() {
    // No height: no picture. 32:9: wider than twice the picture, the curve would fold back.
    assert_eq!(
        column_map(1920, 0, Mode::Widen),
        (0..1920).collect::<Vec<_>>()
    );
    assert_eq!(
        column_map(3840, 1080, Mode::Widen),
        (0..3840).collect::<Vec<_>>()
    );
    assert!(column_map(0, 1080, Mode::Widen).is_empty());
    assert!(column_map(0, 0, Mode::Unsqueeze).is_empty());
    assert_eq!(column_map(5, 0, Mode::Unsqueeze).len(), 5);
}

#[test]
fn the_picture_is_centred_with_the_odd_column_on_the_right() {
    // 1921 - 1440 = 481: the bar is 240 on the left, 241 on the right.
    let map = column_map(1921, 1080, Mode::Widen);
    assert_eq!(map.len(), 1921);
    assert_eq!(map[0], 240);
    assert_eq!(map[1920], 240 + 1439);
}

#[test]
fn the_curve_stays_inside_the_picture_so_the_guard_never_acts() {
    // The guard of `column_map` is only that. If the curve ever left the picture, the maps of the
    // application would paint black there while the plugin repeated an edge column.
    for height in (240..=2160).step_by(2) {
        let picture = (height * 4 + 1) / 3;
        for width in [picture + 2, picture * 4 / 3, picture * 3 / 2, picture * 2] {
            for (w, squeeze) in [(width, false), (picture, true)] {
                for x in 0..w {
                    let c = superview_core::pgm::source_x_for_output_x(
                        x as i64,
                        w as i64,
                        if squeeze { w } else { picture } as i64,
                        squeeze,
                    );
                    assert!(
                        (0..(if squeeze { w } else { picture }) as i64).contains(&c),
                        "{w}x{height} squeeze={squeeze} x={x}: {c}"
                    );
                }
            }
        }
    }
}

#[test]
fn no_column_leaves_the_buffer() {
    for (w, h) in [
        (1920, 1080),
        (1280, 720),
        (2560, 1080),
        (641, 361),
        (3840, 2160),
    ] {
        for mode in [Mode::Widen, Mode::Unsqueeze] {
            let map = column_map(w, h, mode);
            assert_eq!(map.len(), w);
            assert!(map.iter().all(|&c| c < w), "{w}x{h} {mode:?}");
        }
    }
}

#[test]
fn a_row_is_copied_through_the_map() {
    let src = [10u8, 11, 12, 13];
    let mut dst = [0u8; 4];
    assert!(warp_row(&src, &mut dst, &[3, 3, 0, 1]));
    assert_eq!(dst, [13, 13, 10, 11]);
}

#[test]
fn a_row_of_the_wrong_length_is_refused_untouched() {
    let src = [10u8, 11, 12, 13];
    let mut dst = [7u8; 4];
    assert!(!warp_row(&src, &mut dst, &[0, 1, 2]));
    assert!(!warp_row(&src, &mut dst, &[0, 1, 2, 4]));
    assert!(!warp_row(&src[..3], &mut dst, &[0, 1, 2, 3]));
    assert_eq!(dst, [7; 4]);
}
