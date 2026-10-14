//! Port of `pgm_golden_test.go`: golden fingerprints and map properties.

mod common;

use common::{parse_p5, sha256_hex, video};
use superview_core::pgm::{generate_pgm, generate_pgm_bytes, remap_map_bytes};

// Go: TestGeneratePGM_Golden
#[test]
fn golden_fingerprints() {
    // (width, height, squeeze, size, x sha256, y sha256)
    let cases = [
        (
            1440,
            1080,
            false,
            4147219,
            "2227c17119b499db7ce11bc27376e423ecccbfe24bf4a9f6c49363312261202f",
            "87348bfb3fbb4a513dfc5f02293445c09e940ab663f1befb033c8dc383b9e6fa",
        ),
        (
            1440,
            1080,
            true,
            3110419,
            "f9c97781445026e2d306e05b521df416e77bca843e66b04ea3c60a44e31c6421",
            "38cb195cc83f764cab65cd5c3db87de8a4a66b16bdaf2e68b55b994ef807a33e",
        ),
        (
            641,
            481,
            false,
            821565,
            "e500fe0ca537005f4d9a9c78b2a25492cc7e5bb0592dd0d7f4df856166897a8e",
            "aeddcf2ebad943c66d267d387f77eb63ab49491a7945d473a6e0e1f881656cfa",
        ),
        (
            641,
            481,
            true,
            616659,
            "a10aa7c6ead73b685b651cb4e04d338ae3ebda4e2e6144b533705667de5f1e46",
            "aace303c7eb5336eba6bdec932e263f3610a377731e01cdbdffde248380abb72",
        ),
    ];
    for (w, h, squeeze, size, x_hash, y_hash) in cases {
        let name = format!("{w}x{h} squeeze={squeeze}");
        let (x, y) = generate_pgm_bytes(&video(w, h), squeeze).unwrap();
        assert_eq!((x.len(), y.len()), (size, size), "{name}: sizes");
        assert_eq!(sha256_hex(&x), x_hash, "{name}: x.pgm content changed");
        assert_eq!(sha256_hex(&y), y_hash, "{name}: y.pgm content changed");
    }
}

// Go: TestGeneratePGM_XMapIsRowInvariant
#[test]
fn x_map_is_row_invariant() {
    let (x, _) = generate_pgm_bytes(&video(160, 120), false).unwrap();
    let (_, body, width, height) = parse_p5(&x);
    assert!(width > 0 && height >= 2);
    let row = width * 2;
    for y in 1..height {
        assert_eq!(
            &body[y * row..(y + 1) * row],
            &body[..row],
            "row {y} differs from row 0"
        );
    }
}

// Go: TestGeneratePGM_MapsAreBigEndianP5
#[test]
fn maps_are_big_endian_p5() {
    let (_, y) = generate_pgm_bytes(&video(640, 480), false).unwrap();
    let (header, body, width, height) = parse_p5(&y);
    assert!(header.starts_with("P5 "));
    assert!(height > 256);
    // Row 256 is the first whose value does not fit in a byte.
    assert_eq!(&body[256 * width * 2..256 * width * 2 + 2], &[0x01, 0x00]);
}

// Go: TestRemapMapBytes_MatchesWhatGeneratePGMWrites
#[test]
fn remap_map_bytes_matches_what_is_written() {
    let dir = std::env::temp_dir().join(format!("superview-core-pgm-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (w, h) in [(1440, 1080), (641, 481)] {
        for squeeze in [false, true] {
            let v = video(w, h);
            let (xp, yp) = generate_pgm(&v, squeeze, &dir).unwrap();
            let written =
                std::fs::metadata(&xp).unwrap().len() + std::fs::metadata(&yp).unwrap().len();
            assert_eq!(
                remap_map_bytes(&v, squeeze),
                written as i64,
                "{w}x{h} squeeze={squeeze}"
            );
            // The files on disk are the in-memory bytes.
            let (x, y) = generate_pgm_bytes(&v, squeeze).unwrap();
            assert_eq!(std::fs::read(&xp).unwrap(), x);
            assert_eq!(std::fs::read(&yp).unwrap(), y);
        }
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

// Go: TestRemapMapBytes_NoStreams
#[test]
fn remap_map_bytes_without_streams_is_zero() {
    let empty = superview_core::video::VideoSpecs {
        file: String::new(),
        streams: vec![],
    };
    assert_eq!(remap_map_bytes(&empty, false), 0);
}

fn sample(body: &[u8], x: usize) -> i64 {
    i64::from(body[x * 2]) << 8 | i64::from(body[x * 2 + 1])
}

// Go: TestGeneratePGM_SqueezeMapIsContinuous
#[test]
fn squeeze_map_is_continuous() {
    for (w, h) in [
        (640, 480),
        (1440, 1080),
        (1920, 1440),
        (2880, 2160),
        (4064, 3048),
    ] {
        assert_ne!(w % 112, 0, "on a multiple of 112 the defect cannot appear");
        let (x, _) = generate_pgm_bytes(&video(w, h), true).unwrap();
        let (_, body, width, _) = parse_p5(&x);
        let mid = width / 2;
        let seam = sample(body, mid) - sample(body, mid - 1);
        let before = sample(body, mid - 1) - sample(body, mid - 2);
        let after = sample(body, mid + 1) - sample(body, mid);
        assert!(
            seam <= before.max(after) + 1,
            "{w}x{h}: step {seam} across the centre, {before}/{after} beside it"
        );
    }
}

// Go: TestGeneratePGM_SqueezeMapStaysWellFormed
#[test]
fn squeeze_map_stays_well_formed() {
    for (w, h) in [(640, 480), (1440, 1080), (2880, 2160), (4064, 3048)] {
        let (x, _) = generate_pgm_bytes(&video(w, h), true).unwrap();
        let (_, body, width, _) = parse_p5(&x);
        for i in 1..width {
            assert!(
                sample(body, i) >= sample(body, i - 1),
                "{w}x{h}: goes backwards at x={i}"
            );
        }
        assert_eq!(sample(body, 0), 0);
        assert_eq!(sample(body, width - 1), w - 1);
        assert_eq!(sample(body, width / 2), w / 2);
    }
}

#[test]
fn refused_video_writes_no_file() {
    let dir = std::env::temp_dir().join(format!("superview-core-refused-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    assert!(generate_pgm(&video(1080, 1440), false, &dir).is_err());
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
    std::fs::remove_dir_all(&dir).unwrap();
}
