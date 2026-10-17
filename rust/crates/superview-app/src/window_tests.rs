//! Scenarios on the real window under the testing backend: the labels it shows, where its
//! toolbar sits, what it starts with, that every button reports its click, and the popups.
//! Plain functions called from the one `#[test]` in `app_tests.rs`: the backend can be initialised
//! only once per process. Port of Go's `TestToolbarFitsWindow` and `TestToolbarIsCentred`.
//!
//! The testing backend does not measure the real font, so nothing here asserts that a label
//! fits its button, nor that a long path wraps (the text sizes are not real, `word-wrap` passed
//! the same test as `char-wrap`); a button pushed out of the frame is what the toolbar test sees.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use slint::platform::PointerEventButton;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use super::{MainWindow, Popup};

/// The window's size, as the Go window's `windowWidth` and `windowHeight`.
const WINDOW_WIDTH: f32 = 980.0;
const WINDOW_HEIGHT: f32 = 470.0;
/// The padding around the window's content (`padding` in `main.slint`).
const PADDING: f32 = 12.0;

const TOOLBAR: [&str; 6] = [
    "Choose input file",
    "Choose output file",
    "Start transformation",
    "Cancel",
    "Diagnostic",
    "Quit",
];

const SQUEEZE: &str = "Source already stretched to 16:9 (un-squeeze)";
const INSTALL_URL: &str = "https://example.com/ffmpeg";

/// Every text that only a popup shows (titles and bodies; "OK" and "Error" are shared and
/// checked where they matter). None may be on screen while no popup is open.
const POPUP_TEXTS: [&str; 12] = [
    "Done",
    "Overwrite output file",
    "The selected output file already exists. Overwrite it?",
    "Cancel and quit",
    "An encoding is in progress. Do you want to cancel and quit?",
    "No input",
    "Please open an input video first.",
    "No output",
    "Please choose an output file first.",
    "System diagnostic",
    "cannot find ffmpeg/ffprobe on your system",
    "make sure to install it first:",
];

/// The elements whose accessible label is exactly `label`.
fn find(window: &MainWindow, label: &str) -> Vec<ElementHandle> {
    ElementHandle::find_by_accessible_label(window, label).collect()
}

/// The one element labelled `label`: a missing label or a duplicated one fails.
pub(crate) fn only(window: &MainWindow, label: &str) -> ElementHandle {
    let mut found = find(window, label);
    assert_eq!(found.len(), 1, "expected exactly one {label:?}");
    found.remove(0)
}

pub(crate) fn is_shown(window: &MainWindow, label: &str) -> bool {
    !find(window, label).is_empty()
}

/// The two selectors, quality then codec.
fn selectors(window: &MainWindow) -> Vec<ElementHandle> {
    ElementHandle::find_by_element_type_name(window, "ComboBox").collect()
}

/// A shown window has its layout computed.
fn shown() -> MainWindow {
    let window = MainWindow::new().expect("window");
    window.show().expect("show");
    window
}

/// Runs `future` on the event loop and returns when it is done. Gives up after 5 s, like
/// `wait_until` in `app_tests.rs`: a hang must not freeze the suite.
pub(crate) fn drive(future: impl std::future::Future<Output = ()> + 'static) {
    slint::spawn_local(async move {
        future.await;
        slint::quit_event_loop().expect("quit");
    })
    .expect("spawn");
    let give_up = slint::Timer::default();
    give_up.start(slint::TimerMode::SingleShot, Duration::from_secs(5), || {
        slint::quit_event_loop().expect("quit");
    });
    slint::run_event_loop_until_quit().expect("event loop");
}

/// Makes a fresh closure that bumps the counter.
type Bump = Box<dyn Fn() -> Box<dyn Fn()>>;

/// A counter and a maker of closures that bump it.
fn counter() -> (Rc<Cell<u32>>, Bump) {
    let count = Rc::new(Cell::new(0_u32));
    let shared = Rc::clone(&count);
    let bump: Bump = Box::new(move || {
        let count = Rc::clone(&shared);
        Box::new(move || count.set(count.get() + 1))
    });
    (count, bump)
}

pub fn run() {
    every_label_is_present();
    the_window_has_the_designed_size();
    the_toolbar_stays_in_the_frame();
    the_toolbar_is_centred();
    the_window_starts_in_its_initial_state();
    every_button_reports_its_click();
    the_form_follows_the_host_after_the_user_chose();
    the_popups_open_close_and_say_what_they_should();
    the_prerequisite_popup_without_retry();
    a_popup_blocks_the_window_behind_it();
}

fn every_label_is_present() {
    let window = shown();
    for label in [
        "Superview",
        "Convert 4:3 to 16:9 in one workflow",
        "Choose input file",
        "Choose output file",
        "Start transformation",
        "Cancel",
        "Diagnostic",
        "Quit",
        "Input file",
        "Output file",
        "Quality profile",
        "Video codec",
        "Input format",
        SQUEEZE,
        "Status: Ready",
        "Results: no completed run yet",
        "Hardware: waiting for input video",
        "Progress",
    ] {
        assert!(is_shown(&window, label), "label {label:?} is missing");
    }
    // Two lines read "No file selected": the input and the output.
    assert_eq!(find(&window, "No file selected").len(), 2);
}

fn the_window_has_the_designed_size() {
    let window = shown();
    let size = window
        .window()
        .size()
        .to_logical(window.window().scale_factor());
    assert!(
        (size.width - WINDOW_WIDTH).abs() < 0.5 && (size.height - WINDOW_HEIGHT).abs() < 0.5,
        "the window is {}x{}, not {WINDOW_WIDTH}x{WINDOW_HEIGHT}",
        size.width,
        size.height
    );
}

/// No button leaves the window.
fn the_toolbar_stays_in_the_frame() {
    let window = shown();
    for label in TOOLBAR {
        let button = only(&window, label);
        let (pos, size) = (button.absolute_position(), button.size());
        assert!(
            pos.x >= 0.0,
            "{label:?} starts at x={}, outside the window",
            pos.x
        );
        assert!(
            pos.x + size.width <= WINDOW_WIDTH,
            "{label:?} ends at x={}, past the {WINDOW_WIDTH}px window",
            pos.x + size.width
        );
    }
}

/// The row sits in the middle: equal margins within a pixel, and not flush left.
fn the_toolbar_is_centred() {
    let window = shown();
    let first = only(&window, TOOLBAR[0]);
    let last = only(&window, TOOLBAR[5]);
    let left = first.absolute_position().x;
    let row_end = last.absolute_position().x + last.size().width;
    let right = WINDOW_WIDTH - row_end;
    assert!(
        left > PADDING,
        "the row starts at x={left} in a {WINDOW_WIDTH}px window: it is flush left"
    );
    assert!(
        (left - right).abs() <= 1.0,
        "margins are {left}px left and {right}px right: the toolbar is off-centre"
    );
}

fn the_window_starts_in_its_initial_state() {
    let window = shown();
    assert_eq!(window.get_status(), "Status: Ready");
    assert_eq!(window.get_results(), "Results: no completed run yet");
    assert_eq!(window.get_hardware(), "Hardware: waiting for input video");
    assert_eq!(window.get_input_name(), "No file selected");
    assert_eq!(window.get_output_name(), "No file selected");
    assert_eq!(window.get_quality(), "Balanced");
    assert!(!window.get_squeeze());
    assert!(window.get_progress().abs() < f32::EPSILON);
    for (label, enabled) in [
        ("Choose input file", true),
        ("Choose output file", true),
        ("Start transformation", false),
        ("Cancel", false),
        ("Diagnostic", true),
        ("Quit", true),
        (SQUEEZE, true),
    ] {
        assert_eq!(
            only(&window, label).accessible_enabled(),
            Some(enabled),
            "{label:?} has the wrong initial state"
        );
    }
    // The two selectors (quality, then codec) are usable.
    let selectors = selectors(&window);
    assert_eq!(selectors.len(), 2);
    for selector in &selectors {
        assert_eq!(
            selector.accessible_enabled(),
            Some(true),
            "a selector starts disabled"
        );
    }
    assert_eq!(selectors[0].accessible_value().as_deref(), Some("Balanced"));
    // No popup is open: none of their texts is on screen, and "Error" appears nowhere.
    for text in POPUP_TEXTS
        .iter()
        .chain(&["Error", "OK", "Close", "Retry", "Yes", "No"])
    {
        assert!(
            !is_shown(&window, text),
            "{text:?} is on screen at the start"
        );
    }
}

fn every_button_reports_its_click() {
    let window = shown();
    window.set_start_enabled(true);
    window.set_cancel_enabled(true);
    let clicks = Rc::new(Cell::new([0_u32; 6]));
    let count = |index: usize| {
        let clicks = Rc::clone(&clicks);
        move || {
            let mut now = clicks.get();
            now[index] += 1;
            clicks.set(now);
        }
    };
    window.on_choose_input(count(0));
    window.on_choose_output(count(1));
    window.on_start(count(2));
    window.on_cancel(count(3));
    window.on_diagnostic(count(4));
    window.on_quit(count(5));
    for (index, label) in TOOLBAR.iter().enumerate() {
        only(&window, label).invoke_accessible_default_action();
        let mut expected = [0_u32; 6];
        expected[index] = 1;
        assert_eq!(
            clicks.get(),
            expected,
            "{label:?} did not report exactly its own click"
        );
        clicks.set([0; 6]);
    }
}

/// The selectors and the box are two-way: once the user has chosen, a value the host writes
/// still shows. A one-way binding is broken by the first user choice and the host's later
/// writes then change nothing.
fn the_form_follows_the_host_after_the_user_chose() {
    let window = shown();
    let (toggled, bump) = counter();
    let last = Rc::new(Cell::new(false));
    {
        let (bump, last) = (bump(), Rc::clone(&last));
        window.on_squeeze_toggled(move |on| {
            last.set(on);
            bump();
        });
    }
    let checkbox = only(&window, SQUEEZE);
    {
        let checkbox = checkbox.clone();
        drive(async move { checkbox.single_click(PointerEventButton::Left).await });
    }
    assert_eq!(toggled.get(), 1, "the click did not report the toggle");
    assert!(last.get(), "the toggle reported the wrong value");
    assert_eq!(checkbox.accessible_checked(), Some(true));
    window.set_squeeze(false);
    assert_eq!(
        checkbox.accessible_checked(),
        Some(false),
        "the host's value no longer reaches the box after a user click"
    );

    // The codec list and its selection come from the host.
    let options: Vec<SharedString> = ["Auto", "libx264", "h264_nvenc"].map(Into::into).into();
    window.set_encoder_options(ModelRc::new(VecModel::from(options)));
    window.set_encoder_selected("h264_nvenc".into());
    let selectors = selectors(&window);
    assert_eq!(
        selectors[1].accessible_value().as_deref(),
        Some("h264_nvenc")
    );
    // The user opens the list and picks "Auto": the window reports it and keeps it.
    let chosen = Rc::new(std::cell::RefCell::new(String::new()));
    {
        let chosen = Rc::clone(&chosen);
        window.on_encoder_changed(move |value| *chosen.borrow_mut() = value.to_string());
    }
    let combo = selectors[1].clone();
    drive(async move { combo.single_click(PointerEventButton::Left).await });
    let item = only(&window, "Auto");
    drive(async move { item.single_click(PointerEventButton::Left).await });
    assert_eq!(
        *chosen.borrow(),
        "Auto",
        "picking an item did not report it"
    );
    assert_eq!(window.get_encoder_selected(), "Auto");
    window.set_encoder_selected("libx264".into());
    assert_eq!(selectors[1].accessible_value().as_deref(), Some("libx264"));
    window.set_quality("Fast".into());
    assert_eq!(selectors[0].accessible_value().as_deref(), Some("Fast"));
}

/// Opens a popup through `open`, checks the texts, clicks `button` and checks `fired`, then
/// closes the popup and checks that it is gone.
fn check_popup(window: &MainWindow, popup: Popup, texts: &[&str], button: &str, fired: &Cell<u32>) {
    window.set_popup(popup);
    for text in texts {
        assert!(is_shown(window, text), "popup text {text:?} is missing");
    }
    only(window, button).invoke_accessible_default_action();
    assert_eq!(fired.get(), 1, "{button:?} did not report its click");
    fired.set(0);
    window.set_popup(Popup::None);
    for text in texts {
        assert!(
            !is_shown(window, text),
            "popup text {text:?} stays after closing"
        );
    }
}

fn the_popups_open_close_and_say_what_they_should() {
    let window = shown();
    let (fired, bump) = counter();
    window.on_done_ok(bump());
    window.on_error_ok(bump());
    window.on_prerequisite_retry_clicked(bump());
    window.on_prerequisite_close(bump());
    window.on_overwrite_yes(bump());
    window.on_overwrite_no(bump());
    window.on_quit_yes(bump());
    window.on_quit_no(bump());
    window.on_no_input_ok(bump());
    window.on_no_output_ok(bump());
    window.on_diagnostic_close(bump());

    window.set_done_message("Transform complete. Output file:\n/tmp/out.mp4".into());
    check_popup(
        &window,
        Popup::Done,
        &["Done", "Transform complete. Output file:\n/tmp/out.mp4"],
        "OK",
        &fired,
    );
    window.set_error_message("something broke".into());
    check_popup(
        &window,
        Popup::Error,
        &["Error", "something broke"],
        "OK",
        &fired,
    );

    // The prerequisite popup: three lines, the link, the hint, Retry and Close.
    window.set_install_url(INSTALL_URL.into());
    let texts = [
        "Error",
        "cannot find ffmpeg/ffprobe on your system",
        "make sure to install it first:",
        INSTALL_URL,
        "Already installed it? Use Retry.",
    ];
    check_popup(&window, Popup::Prerequisite, &texts, "Retry", &fired);
    check_popup(&window, Popup::Prerequisite, &texts, "Close", &fired);

    // The link reports its address on a real pointer click.
    let opened = Rc::new(Cell::new(0_u32));
    {
        let opened = Rc::clone(&opened);
        window.on_open_install_link(move |url| {
            assert_eq!(url, INSTALL_URL);
            opened.set(opened.get() + 1);
        });
    }
    window.set_popup(Popup::Prerequisite);
    let link = only(&window, &format!("Open {INSTALL_URL}"));
    assert_eq!(link.accessible_role(), Some(AccessibleRole::Button));
    drive(async move { link.single_click(PointerEventButton::Left).await });
    assert_eq!(opened.get(), 1, "a click on the link did not report it");
    window.set_popup(Popup::None);

    let overwrite = [
        "Overwrite output file",
        "The selected output file already exists. Overwrite it?",
    ];
    check_popup(&window, Popup::Overwrite, &overwrite, "Yes", &fired);
    check_popup(&window, Popup::Overwrite, &overwrite, "No", &fired);
    let quit = [
        "Cancel and quit",
        "An encoding is in progress. Do you want to cancel and quit?",
    ];
    check_popup(&window, Popup::Quit, &quit, "Yes", &fired);
    check_popup(&window, Popup::Quit, &quit, "No", &fired);
    check_popup(
        &window,
        Popup::NoInput,
        &["No input", "Please open an input video first."],
        "OK",
        &fired,
    );
    check_popup(
        &window,
        Popup::NoOutput,
        &["No output", "Please choose an output file first."],
        "OK",
        &fired,
    );
    window.set_diagnostic_report("ffmpeg: ok\nffprobe: ok".into());
    check_popup(
        &window,
        Popup::Diagnostic,
        &["System diagnostic", "ffmpeg: ok\nffprobe: ok"],
        "Close",
        &fired,
    );
}

/// With nothing to retry the popup offers "OK" alone and no hint about Retry.
fn the_prerequisite_popup_without_retry() {
    let window = shown();
    let (closed, bump) = counter();
    window.on_prerequisite_close(bump());
    window.set_install_url(INSTALL_URL.into());
    window.set_prerequisite_retry(false);
    window.set_popup(Popup::Prerequisite);
    for text in [
        "Error",
        "cannot find ffmpeg/ffprobe on your system",
        INSTALL_URL,
    ] {
        assert!(is_shown(&window, text), "{text:?} is missing");
    }
    for text in ["Retry", "Close", "Already installed it? Use Retry."] {
        assert!(
            !is_shown(&window, text),
            "{text:?} is offered with nothing to retry"
        );
    }
    only(&window, "OK").invoke_accessible_default_action();
    assert_eq!(closed.get(), 1, "OK did not close the popup");
}

/// While a popup is open the controls behind it are disabled (keys), and a click on one
/// reaches nothing (mouse); both come back when it closes.
fn a_popup_blocks_the_window_behind_it() {
    let window = shown();
    window.set_start_enabled(true);
    let (started, bump) = counter();
    window.on_start(bump());
    let behind = [
        "Choose input file",
        "Choose output file",
        "Start transformation",
        "Diagnostic",
        "Quit",
        SQUEEZE,
    ];
    window.set_cancel_enabled(true);
    window.set_popup(Popup::Done);
    for label in behind.iter().chain(&["Cancel"]) {
        assert_eq!(
            only(&window, label).accessible_enabled(),
            Some(false),
            "{label:?} stays usable behind a popup"
        );
    }
    for selector in selectors(&window) {
        assert_eq!(
            selector.accessible_enabled(),
            Some(false),
            "a selector stays usable"
        );
    }
    let start = only(&window, "Start transformation");
    drive(async move { start.single_click(PointerEventButton::Left).await });
    assert_eq!(started.get(), 0, "a click went through the popup");

    window.set_popup(Popup::None);
    for label in behind.iter().chain(&["Cancel"]) {
        assert_eq!(
            only(&window, label).accessible_enabled(),
            Some(true),
            "{label:?} stays locked after the popup closed"
        );
    }
}
