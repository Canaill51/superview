//! Scenarios on the real window under the testing backend: the labels it shows, where its
//! buttons sit, what it starts with, that every button reports its click, and the popups.
//! Plain functions called from the one `#[test]` in `app_tests.rs`: the backend can be initialised
//! only once per process.
//!
//! The testing backend does not measure the real font, so nothing here asserts that a label
//! fits its button, nor that a long path wraps (the text sizes are not real, `word-wrap` passed
//! the same test as `char-wrap`); a button pushed out of the frame is what the button-position test sees.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use slint::platform::PointerEventButton;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use super::{DiagnosticRow, MainWindow, Popup, ReportRowKind, Theme};

/// The window's size (fixed, DEC-61).
const WINDOW_WIDTH: f32 = 1000.0;
const WINDOW_HEIGHT: f32 = 600.0;

const BUTTONS: [&str; 6] = [
    "Choose input file",
    "Choose output file",
    "Start transformation",
    "Cancel",
    "Diagnostic",
    "Quit",
];

const SQUEEZE: &str = "Source already stretched to 16:9 (un-squeeze)";
const PATH_ENTRY_BODY: &str = "No file picker could be opened. Type the full path of the file:";
const INSTALL_URL: &str = "https://example.com/ffmpeg";

/// Every text that only a popup shows (titles and bodies; "OK" and "Error" are shared and
/// checked where they matter). None may be on screen while no popup is open.
const POPUP_TEXTS: [&str; 13] = [
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
    PATH_ENTRY_BODY,
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

/// The codec menu, the one `ComboBox` of the settings (the Theme menu is not one of them, and the
/// quality profile is a row of segments, not a menu).
fn selectors(window: &MainWindow) -> Vec<ElementHandle> {
    ElementHandle::find_by_element_type_name(window, "ComboBox")
        .filter(|combo| combo.accessible_label().as_deref() != Some("Theme"))
        .collect()
}

/// A shown window has its layout computed.
fn shown() -> MainWindow {
    let window = MainWindow::new().expect("window");
    // As `App::wire` does: the menu's entries first, then the choice (a new model selects its
    // first entry).
    crate::app::set_quality_profiles(&window);
    window.set_quality("Balanced".into());
    window.set_theme("Dark".into());
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
    the_buttons_and_the_settings_sit_where_the_mockup_puts_them();
    the_window_starts_in_its_initial_state();
    every_button_reports_its_click();
    the_form_follows_the_host_after_the_user_chose();
    the_popups_open_close_and_say_what_they_should();
    the_prerequisite_popup_without_retry();
    a_popup_blocks_the_window_behind_it();
    the_theme_menu_switches_the_colours();
    an_active_button_answers_the_first_mouse_click();
    the_bar_changes_colour_with_its_state();
    the_pill_names_the_state();
    a_long_error_scrolls_and_keeps_its_button_in_the_window();
    a_locked_control_ignores_the_accessible_action();
    the_right_column_fits_with_the_longest_texts();
}

fn every_label_is_present() {
    let window = shown();
    for label in [
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

/// The buttons are where the mockup puts them, none leaves the window: the input button in the
/// file card and the output button by the output field, on the left; Start and Cancel in the
/// conversion card under Status and Results, then Diagnostic and Quit at the foot of the right
/// column. On the right the settings come in order (quality, codec, input format, hardware),
/// then the conversion with the bar first and Status and Results under it.
fn the_buttons_and_the_settings_sit_where_the_mockup_puts_them() {
    let window = shown();
    let x = |e: &ElementHandle| e.absolute_position().x;
    let y = |e: &ElementHandle| e.absolute_position().y;
    let quality = only(&window, "Quality profile");
    let codec = only(&window, "Video codec");
    let input_format = only(&window, "Input format");
    let hardware = only(&window, "Hardware: waiting for input video");
    let progress = only(&window, "Progress");
    let status = only(&window, "Status: Ready");
    let results = only(&window, "Results: no completed run yet");

    let column = x(&quality);
    assert!(
        x(&only(&window, BUTTONS[0])) + only(&window, BUTTONS[0]).size().width <= column,
        "the file card must be in the left column, the settings in the right one"
    );
    for (name, element) in [
        ("Video codec", &codec),
        ("Input format", &input_format),
        ("Hardware", &hardware),
        ("Progress", &progress),
        ("Status", &status),
        ("Results", &results),
    ] {
        assert!(
            (x(element) - column).abs() <= 16.0,
            "{name} is not in the settings column (x={}, column x={column})",
            x(element)
        );
    }
    let order = [
        &quality,
        &codec,
        &input_format,
        &hardware,
        &progress,
        &status,
        &results,
    ];
    for pair in order.windows(2) {
        assert!(y(pair[0]) < y(pair[1]), "the right column is out of order");
    }
    for combo in selectors(&window) {
        let right = x(&combo) + combo.size().width;
        assert!(
            right <= WINDOW_WIDTH,
            "the codec menu ends at x={right}, outside the window"
        );
    }
    let fast = only(&window, "Fast");
    let balanced = only(&window, "Balanced");
    assert!(
        x(&fast) < x(&balanced),
        "the quality segments are out of order"
    );

    for label in BUTTONS {
        let button = only(&window, label);
        let (pos, size) = (button.absolute_position(), button.size());
        assert!(
            pos.x >= 0.0 && pos.x + size.width <= WINDOW_WIDTH,
            "{label:?} is outside the window (x={}..{})",
            pos.x,
            pos.x + size.width
        );
        assert!(
            pos.y >= 0.0 && pos.y + size.height <= WINDOW_HEIGHT,
            "{label:?} is outside the window (y={}..{})",
            pos.y,
            pos.y + size.height
        );
    }
    let button = |label: &str| only(&window, label);
    let (input, output) = (button("Choose input file"), button("Choose output file"));
    let (start, cancel) = (button("Start transformation"), button("Cancel"));
    let (diagnostic, quit) = (button("Diagnostic"), button("Quit"));
    assert!(
        x(&output) + output.size().width <= column,
        "Choose output file is not on the left"
    );
    assert!(
        y(&input) < y(&output),
        "the input button must be above the output one"
    );
    assert!(
        y(&start) > y(&results) && x(&start) >= column,
        "Start must be in the conversion card, under Results"
    );
    assert!(
        (y(&start) - y(&cancel)).abs() <= 1.0 && x(&cancel) > x(&start),
        "Cancel must sit beside Start, to its right"
    );
    assert!(
        y(&diagnostic) > y(&start) && x(&diagnostic) >= column && x(&quit) > x(&diagnostic),
        "Diagnostic and Quit must close the right column, in that order"
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
    assert!(
        !window.get_diagnostic_as_text(),
        "the Diagnostic opens on the list with icons"
    );
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
    // The codec menu is usable, and the quality row shows the saved profile.
    let selectors = selectors(&window);
    assert_eq!(selectors.len(), 1);
    for selector in &selectors {
        assert_eq!(
            selector.accessible_enabled(),
            Some(true),
            "a selector starts disabled"
        );
    }
    assert_eq!(only(&window, "Balanced").accessible_checked(), Some(true));
    assert_eq!(only(&window, "Fast").accessible_checked(), Some(false));
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
    for (index, label) in BUTTONS.iter().enumerate() {
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
        selectors[0].accessible_value().as_deref(),
        Some("h264_nvenc")
    );
    // The user opens the list and picks "Auto": the window reports it and keeps it.
    let chosen = Rc::new(std::cell::RefCell::new(String::new()));
    {
        let chosen = Rc::clone(&chosen);
        window.on_encoder_changed(move |value| *chosen.borrow_mut() = value.to_string());
    }
    let combo = selectors[0].clone();
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
    assert_eq!(selectors[0].accessible_value().as_deref(), Some("libx264"));
    window.set_quality("Fast".into());
    assert_eq!(only(&window, "Fast").accessible_checked(), Some(true));
    // The user picks a segment: the window reports it and keeps it, and the host can write again.
    let picked = Rc::new(std::cell::RefCell::new(String::new()));
    {
        let picked = Rc::clone(&picked);
        window.on_quality_changed(move |value| *picked.borrow_mut() = value.to_string());
    }
    let balanced = only(&window, "Balanced");
    drive(async move { balanced.single_click(PointerEventButton::Left).await });
    assert_eq!(
        *picked.borrow(),
        "Balanced",
        "picking a segment did not report it"
    );
    assert_eq!(window.get_quality(), "Balanced");
    window.set_quality("Fast".into());
    assert_eq!(only(&window, "Fast").accessible_checked(), Some(true));
}

/// The bar is red while a conversion goes and green once it is done, and the two differ from
/// each other and from the idle and stopped ones (a bar that kept one colour would pass a
/// check of one state alone).
fn the_bar_changes_colour_with_its_state() {
    use super::BarState;
    let window = shown();
    let mut seen = Vec::new();
    for state in [
        BarState::Idle,
        BarState::Running,
        BarState::Done,
        BarState::Stopped,
    ] {
        window.set_bar_state(state);
        seen.push(window.get_bar_fill());
    }
    for (i, a) in seen.iter().enumerate() {
        for b in &seen[i + 1..] {
            assert_ne!(a, b, "two bar states share a colour");
        }
    }
    // A screen reader reads the bar's figure, as it read Slint's own indicator.
    window.set_progress(0.25);
    let bars: Vec<_> = ElementHandle::find_by_element_type_name(&window, "ProgressBar").collect();
    assert_eq!(bars.len(), 1);
    assert_eq!(
        bars[0].accessible_role(),
        Some(AccessibleRole::ProgressIndicator)
    );
    assert_eq!(bars[0].accessible_value().as_deref(), Some("0.25"));
    let theme = window.global::<Theme>();
    window.set_bar_state(BarState::Running);
    assert_eq!(window.get_bar_fill(), theme.get_running());
    window.set_bar_state(BarState::Done);
    assert_eq!(window.get_bar_fill(), theme.get_done());
}

/// A segment, the switch and the file card answer an assistive technology only while they are
/// usable: during a conversion the profile and the input format must not change.
fn a_locked_control_ignores_the_accessible_action() {
    let window = shown();
    let (changes, bump) = counter();
    window.on_quality_changed({
        let bump = bump();
        move |_| bump()
    });
    window.on_squeeze_toggled({
        let bump = bump();
        move |_| bump()
    });
    window.set_quality_enabled(false);
    window.set_squeeze_enabled(false);
    only(&window, "Fast").invoke_accessible_default_action();
    only(&window, SQUEEZE).invoke_accessible_default_action();
    assert_eq!(changes.get(), 0, "a locked control reported a change");
    window.set_quality_enabled(true);
    window.set_squeeze_enabled(true);
    // The chosen segment is not a change.
    only(&window, "Balanced").invoke_accessible_default_action();
    assert_eq!(
        changes.get(),
        0,
        "choosing the chosen profile reported a change"
    );
    only(&window, "Fast").invoke_accessible_default_action();
    only(&window, SQUEEZE).invoke_accessible_default_action();
    assert_eq!(changes.get(), 2, "a usable control did not report");
}

/// The longest texts the right column can hold at once (a three-line Hardware line, the
/// results line of a finished run, the longest Status) stay inside the window. The backend does
/// not measure the real font, so this catches a column that grew, not a font that is wider.
fn the_right_column_fits_with_the_longest_texts() {
    let window = shown();
    // The longest refusal a probe line can hold: the prefix, then two reasons of the budget's
    // length (the VAAPI "constant quality only" case, DEC-48), with spaces so that it wraps.
    let reason = "word ".repeat(superview_core::probe::PROBE_REASON_BUDGET / 5);
    let hardware =
        format!("Hardware: planned h264_vaapi -- {reason}; in constant quality: {reason}");
    window.set_hardware(hardware.into());
    window.set_status("Status: Starting the encoder... (no progress until the first frame)".into());
    window.set_results(
        "Results: in=1122.9 MB | out=1291.5 MB (115.0%) | total=1m18s | encode=1m12s".into(),
    );
    // The last thing in the column: Diagnostic and Quit.
    let quit = only(&window, "Quit");
    let bottom = quit.absolute_position().y + quit.size().height;
    assert!(
        bottom <= WINDOW_HEIGHT - 8.0,
        "the right column ends at y={bottom}, past the {WINDOW_HEIGHT}px window"
    );
}

/// An error that quotes ffmpeg's stderr can be dozens of lines: the text scrolls in the card and
/// the OK button stays inside the window.
fn a_long_error_scrolls_and_keeps_its_button_in_the_window() {
    let window = shown();
    let message: String = (0..300).map(|i| format!("line {i}\n")).collect();
    window.set_error_message(message.into());
    window.set_popup(Popup::Error);
    let ok = only(&window, "OK");
    let bottom = ok.absolute_position().y + ok.size().height;
    assert!(
        bottom <= WINDOW_HEIGHT,
        "OK ends at y={bottom}, below the {WINDOW_HEIGHT}px window"
    );
    window.set_popup(Popup::None);
}

/// The capsule says a word for each state of the bar; a stopped conversion, cancelled or failed,
/// reads "Stopped" (the Status line says which).
fn the_pill_names_the_state() {
    use super::BarState;
    let window = shown();
    for (state, word) in [
        (BarState::Idle, "Ready"),
        (BarState::Running, "Converting"),
        (BarState::Done, "Completed"),
        (BarState::Stopped, "Stopped"),
    ] {
        window.set_bar_state(state);
        assert_eq!(window.get_state_label(), word);
        assert!(is_shown(&window, word), "{word:?} is not on screen");
    }
}

/// A press on an enabled button is a click the first time: a focus scope over the button took
/// the first press for the focus and the click was lost (it took two).
fn an_active_button_answers_the_first_mouse_click() {
    let window = shown();
    window.set_start_enabled(true);
    let (started, bump) = counter();
    window.on_start(bump());
    let start = only(&window, "Start transformation");
    drive(async move { start.single_click(PointerEventButton::Left).await });
    assert_eq!(
        started.get(),
        1,
        "the first click on an active button was lost"
    );
}

/// The Theme menu (DEC-57): dark to start with, the user's pick is reported and changes the
/// colours, and a theme the host writes (the saved one, at start-up) changes them too.
fn the_theme_menu_switches_the_colours() {
    let window = shown();
    assert!(
        window.global::<Theme>().get_dark(),
        "the window must start dark"
    );
    assert!(window.get_scheme_dark(), "Slint's widgets must start dark");
    let dark_background = window.global::<Theme>().get_background();

    let chosen = Rc::new(std::cell::RefCell::new(String::new()));
    {
        let chosen = Rc::clone(&chosen);
        window.on_theme_changed(move |value| *chosen.borrow_mut() = value.to_string());
    }
    let combo = ElementHandle::find_by_element_type_name(&window, "ComboBox")
        .find(|combo| combo.accessible_label().as_deref() == Some("Theme"))
        .expect("the Theme menu");
    assert_eq!(combo.accessible_value().as_deref(), Some("Dark"));
    drive(async move { combo.single_click(PointerEventButton::Left).await });
    let item = only(&window, "Light");
    drive(async move { item.single_click(PointerEventButton::Left).await });
    assert_eq!(*chosen.borrow(), "Light", "picking Light did not report it");
    assert!(
        !window.global::<Theme>().get_dark(),
        "Light left the colours dark"
    );
    assert_ne!(window.global::<Theme>().get_background(), dark_background);
    assert!(
        !window.get_scheme_dark(),
        "Slint's widgets stay dark in Light"
    );

    // The host writes the saved theme: the colours follow, the menu shows it.
    window.set_theme("Dark".into());
    // `changed` handlers run on the next turn of the event loop.
    drive(async {});
    assert!(window.global::<Theme>().get_dark());
    assert!(window.get_scheme_dark());
    assert_eq!(window.global::<Theme>().get_background(), dark_background);
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

/// A report far longer than the window: the card stays inside it, the Close button with it, and
/// the report sits in a read-only `TextEdit`, whose scrollbars, wheel and selection are what
/// reach the end of it and copy it. The testing backend does not measure the font, so the
/// content height cannot be compared with the visible one here: the box being read-only text
/// that is not clipped to a fixed size is what is checked (`docs/TESTS-MANUELS.md` reads it).
fn the_long_diagnostic_fits_and_can_be_read_to_its_end(window: &MainWindow) {
    let report: String = (0..200)
        .map(|i| format!("line {i}: {}\n", "x".repeat(300)))
        .collect();
    window.set_diagnostic_report(report.as_str().into());
    window.set_diagnostic_as_text(true);
    window.set_popup(Popup::Diagnostic);

    let inside = |what: &str, element: &ElementHandle| {
        let (position, size) = (element.absolute_position(), element.size());
        assert!(
            position.x >= 0.0
                && position.y >= 0.0
                && position.x + size.width <= WINDOW_WIDTH
                && position.y + size.height <= WINDOW_HEIGHT,
            "the diagnostic's {what} leaves the window: {position:?} {size:?}"
        );
    };
    let boxes: Vec<ElementHandle> =
        ElementHandle::find_by_element_type_name(window, "TextEdit").collect();
    assert_eq!(boxes.len(), 1, "the report is not in one TextEdit");
    let report_box = &boxes[0];
    assert_eq!(report_box.accessible_read_only(), Some(true));
    assert_eq!(
        report_box.accessible_value().as_deref(),
        Some(report.as_str())
    );
    assert!(
        report_box.size().height >= 120.0 && report_box.size().width >= 600.0,
        "the report box is too small to read: {:?}",
        report_box.size()
    );
    inside("report box", report_box);
    inside("Close button", &only(window, "Close"));
    window.set_popup(Popup::None);
    window.set_diagnostic_as_text(false);
}

fn report_row(kind: ReportRowKind, ok: bool, text: &str) -> DiagnosticRow {
    DiagnosticRow {
        kind,
        ok,
        text: text.into(),
    }
}

/// The report as a list: a drawn icon per check (green "OK", red "Failed", named for screen
/// readers), the toggle to the selectable text and back, and a 200-line report that still
/// leaves the list and Close inside the window.
fn the_diagnostic_list_draws_its_icons_and_switches_to_text(window: &MainWindow) {
    let rows = vec![
        report_row(ReportRowKind::Plain, false, "Superview build: dev"),
        report_row(ReportRowKind::Status, false, "UNHEALTHY"),
        report_row(ReportRowKind::Check, true, "FFMPEG: ok"),
        report_row(ReportRowKind::Value, false, "Value: /usr/bin/ffmpeg"),
        report_row(ReportRowKind::Check, false, "DISK: low"),
        report_row(ReportRowKind::Check, false, "h264_nvenc: no device"),
    ];
    window.set_diagnostic_rows(ModelRc::new(VecModel::from(rows)));
    window.set_diagnostic_report("[FAIL] DISK: low".into());
    window.set_popup(Popup::Diagnostic);

    assert_eq!(find(window, "OK").len(), 1, "one green icon expected");
    assert_eq!(find(window, "Failed").len(), 3, "three red icons expected");
    for text in [
        "FFMPEG: ok",
        "DISK: low",
        "UNHEALTHY",
        "Value: /usr/bin/ffmpeg",
    ] {
        assert!(is_shown(window, text), "{text:?} is not in the list");
    }
    assert_eq!(
        ElementHandle::find_by_element_type_name(window, "TextEdit").count(),
        0,
        "the list is the default view"
    );
    assert!(is_shown(window, "Show as text"));

    // To the text: no icon, the report in a read-only box, and the button offers the way back.
    only(window, "Show as text").invoke_accessible_default_action();
    assert!(window.get_diagnostic_as_text());
    assert!(find(window, "OK").is_empty() && find(window, "Failed").is_empty());
    let boxes: Vec<ElementHandle> =
        ElementHandle::find_by_element_type_name(window, "TextEdit").collect();
    assert_eq!(boxes.len(), 1);
    assert_eq!(
        boxes[0].accessible_value().as_deref(),
        Some("[FAIL] DISK: low")
    );
    only(window, "Show as list").invoke_accessible_default_action();
    assert!(!window.get_diagnostic_as_text());
    assert_eq!(
        find(window, "Failed").len(),
        3,
        "the icons did not come back"
    );

    long_lines_wrap_and_the_list_is_tall_enough_for_them(window);

    // A long report: the list scrolls inside the card, Close stays in the window.
    let many: Vec<DiagnosticRow> = (0..200)
        .map(|i| report_row(ReportRowKind::Check, i % 2 == 0, &format!("line {i}")))
        .collect();
    window.set_diagnostic_rows(ModelRc::new(VecModel::from(many)));
    for (what, element) in [
        (
            "list",
            ElementHandle::find_by_element_type_name(window, "ScrollView")
                .next()
                .expect("the list is a ScrollView"),
        ),
        ("Close button", only(window, "Close")),
        ("toggle button", only(window, "Show as text")),
    ] {
        let (position, size) = (element.absolute_position(), element.size());
        assert!(
            position.x >= 0.0
                && position.y >= 0.0
                && position.x + size.width <= WINDOW_WIDTH
                && position.y + size.height <= WINDOW_HEIGHT,
            "the diagnostic's {what} leaves the window: {position:?} {size:?}"
        );
        if what == "list" {
            assert!(
                size.height >= 100.0 && size.width >= 600.0,
                "the list is too small to read: {size:?}"
            );
        }
    }
    window.set_popup(Popup::None);
}

/// The lesson of #124 made automatic: a line that wraps is taller than a line that does not, and
/// the list's content is at least as tall as its lines put together, or the end of the report
/// cannot be scrolled to. The testing backend measures text (`monospace` included); where a
/// system gives it no font at all, the wrapped line is not taller and the test says so.
fn long_lines_wrap_and_the_list_is_tall_enough_for_them(window: &MainWindow) {
    let long = "ffmpeg exited with status 1: Cannot load libcuda.so.1 ".repeat(6);
    let rows = vec![
        report_row(ReportRowKind::Check, true, "short"),
        report_row(ReportRowKind::Check, false, &long),
        report_row(ReportRowKind::Plain, false, "Log file: last"),
    ];
    window.set_diagnostic_rows(ModelRc::new(VecModel::from(rows)));
    let lines: Vec<ElementHandle> =
        ElementHandle::find_by_element_type_name(window, "ReportLine").collect();
    assert_eq!(
        lines.len(),
        3,
        "the list does not hold one ReportLine per row"
    );
    let heights: Vec<f32> = lines.iter().map(|l| l.size().height).collect();
    assert!(
        heights[0] >= 17.0 && heights[2] >= 17.0,
        "a one-line row has no height: {heights:?}"
    );
    assert!(
        heights[1] > heights[0] * 1.8,
        "a 300-character line did not wrap to several lines ({heights:?}): the test backend has \
         no font to measure with, or the line height no longer follows its text"
    );
    let content = ElementHandle::find_by_element_id(window, "MainWindow::rows")
        .next()
        .expect("the list's content")
        .size()
        .height;
    assert!(
        content >= heights.iter().sum::<f32>(),
        "the list is {content} px tall for lines that add up to {heights:?}: its end cannot be reached"
    );
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
    window.on_path_entry_ok(bump());
    window.on_path_entry_cancel(bump());

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
    // The report is the value of a `TextEdit`, not a label: it is read from there.
    window.set_diagnostic_report("ffmpeg: ok\nffprobe: ok".into());
    window.set_diagnostic_as_text(true);
    window.set_popup(Popup::Diagnostic);
    let report_box = ElementHandle::find_by_element_type_name(&window, "TextEdit")
        .next()
        .expect("the report box");
    assert_eq!(
        report_box.accessible_value().as_deref(),
        Some("ffmpeg: ok\nffprobe: ok")
    );
    window.set_popup(Popup::None);
    window.set_diagnostic_as_text(false);
    check_popup(
        &window,
        Popup::Diagnostic,
        &["System diagnostic"],
        "Close",
        &fired,
    );
    the_long_diagnostic_fits_and_can_be_read_to_its_end(&window);
    the_diagnostic_list_draws_its_icons_and_switches_to_text(&window);

    // The text field: the title follows the file asked for, the reason shows only when there
    // is one, and both buttons report. "Cancel" also names the other one (greyed out).
    check_popup(
        &window,
        Popup::PathEntry,
        &["Select input video", PATH_ENTRY_BODY],
        "OK",
        &fired,
    );
    window.set_path_entry_output(true);
    check_popup(
        &window,
        Popup::PathEntry,
        &["Save output video", PATH_ENTRY_BODY],
        "OK",
        &fired,
    );
    window.set_path_entry_error("Please type the path of a file.".into());
    window.set_popup(Popup::PathEntry);
    assert!(is_shown(&window, "Please type the path of a file."));
    let cancel = find(&window, "Cancel")
        .into_iter()
        .find(|e| e.accessible_enabled() == Some(true))
        .expect("the text field's Cancel is enabled");
    cancel.invoke_accessible_default_action();
    assert_eq!(fired.get(), 1, "Cancel did not report its click");
    window.set_popup(Popup::None);
    window.set_path_entry_error("".into());
    window.set_path_entry_output(false);
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
        "Fast",
        "Balanced",
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
