use super::*;

#[test]
fn the_emoji_marks_become_plain_text() {
    assert_eq!(
        plain_marks("Status: ✅ HEALTHY\n❌ DISK: low\n✅ FFMPEG: ok"),
        "Status: [OK] HEALTHY\n[FAIL] DISK: low\n[OK] FFMPEG: ok"
    );
    assert_eq!(plain_marks("no marks here"), "no marks here");
}

#[test]
fn a_dialog_is_not_queued_twice_and_the_next_one_waits_its_turn() {
    let mut popups = Popups::default();
    popups.push(PopupKind::Quit);
    popups.push(PopupKind::Quit);
    popups.push(PopupKind::Done("x".into()));
    assert_eq!(popups.current(), Some(&PopupKind::Quit));
    popups.close(true);
    assert_eq!(popups.current(), Some(&PopupKind::Done("x".into())));
    popups.close(true);
    assert_eq!(popups.current(), None);
    popups.close(true); // answering nothing is harmless
    assert_eq!(popups.current(), None);
}

#[test]
fn a_panic_payload_is_read_whatever_its_type() {
    let text = catch_unwind(|| panic!("static text")).expect_err("panics");
    assert_eq!(panic_message(text.as_ref()), "static text");
    let owned = catch_unwind(|| panic!("{}", String::from("owned text"))).expect_err("panics");
    assert_eq!(panic_message(owned.as_ref()), "owned text");
    let other = catch_unwind(|| std::panic::panic_any(7_u8)).expect_err("panics");
    assert_eq!(panic_message(other.as_ref()), "unknown cause");
}
