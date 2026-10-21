use super::*;

#[test]
fn the_arguments_read_the_input_as_mp4_and_ask_for_one_raw_frame() {
    let args = thumbnail_args("/videos/clip.mp4", 2.5, Path::new("/tmp/out/frame.rgba"));
    let at = |needle: &str| args.iter().position(|a| a == needle).expect(needle);
    // DEC-52: `-f mov` comes before the input.
    assert_eq!(&args[at("-f")..at("-f") + 2], ["-f", "mov"]);
    assert!(at("-f") < at("-i"), "-f mov must come before the input");
    assert_eq!(args[at("-i") + 1], "/videos/clip.mp4");
    assert!(
        at("-ss") < at("-i"),
        "the seek is on the input, so it is fast"
    );
    assert_eq!(args[at("-ss") + 1], "2.500");
    assert_eq!(args[at("-frames:v") + 1], "1");
    assert_eq!(args[at("-pix_fmt") + 1], "rgba");
    assert_eq!(
        args.last().map(String::as_str),
        Some("file:/tmp/out/frame.rgba")
    );
}

#[test]
fn a_negative_time_reads_as_the_first_frame() {
    let args = thumbnail_args("a.mp4", -3.0, Path::new("o"));
    let ss = args.iter().position(|a| a == "-ss").expect("-ss");
    assert_eq!(args[ss + 1], "0.000");
}

#[test]
fn the_file_name_is_one_argument_whatever_it_holds() {
    let nasty = "clip; echo hi $(x) `y`.mp4";
    let args = thumbnail_args(nasty, 0.0, Path::new("o"));
    assert_eq!(args.iter().filter(|a| a.as_str() == nasty).count(), 1);
}
