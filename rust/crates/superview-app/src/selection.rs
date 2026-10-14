//! Logic of the input-file selection, with no dependency on Slint or on any
//! window: what the picker may return, and what the window shows afterwards.

use std::path::{Path, PathBuf};

/// True when the file name ends with `.mp4`, in any case. The hidden name
/// `.mp4` is accepted, as Go's `strings.EqualFold(filepath.Ext(path), ".mp4")`
/// does (`filepath.Ext(".mp4")` is `.mp4`).
/// Go's input side does not filter by extension (only its picker does, and its
/// output side forces `.mp4`): checking the input here is a choice of this trial.
pub fn is_mp4(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name.to_string_lossy().to_lowercase().ends_with(".mp4"))
}

/// What happened to a picker answer.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The path was accepted and is now the input.
    Chosen,
    /// The user closed the picker without choosing.
    Cancelled,
    /// The path is not an MP4; the previous input, if any, is kept.
    Rejected,
}

/// The state behind the window: the chosen input and whether a picker is open.
#[derive(Debug, Default)]
pub struct Selection {
    input: Option<PathBuf>,
    picking: bool,
}

impl Selection {
    /// Marks a picker as open. Returns false, changing nothing, when one is
    /// already open.
    pub fn begin_pick(&mut self) -> bool {
        if self.picking {
            return false;
        }
        self.picking = true;
        true
    }

    /// Records the picker's answer (`None` = cancelled) and closes the pick.
    pub fn finish_pick(&mut self, answer: Option<PathBuf>) -> Outcome {
        self.picking = false;
        match answer {
            None => Outcome::Cancelled,
            Some(path) if is_mp4(&path) => {
                self.input = Some(path);
                Outcome::Chosen
            }
            Some(_) => Outcome::Rejected,
        }
    }

    #[cfg(test)]
    pub fn is_picking(&self) -> bool {
        self.picking
    }

    /// The full path of the chosen input, kept for the conversion to come.
    #[cfg(test)]
    pub fn input(&self) -> Option<&Path> {
        self.input.as_deref()
    }

    /// The text of the field: the file name only (Go's `filepath.Base`), or
    /// empty before any choice. The full path stays in the state.
    pub fn input_text(&self) -> String {
        self.input
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// The new status line for an outcome, or `None` to leave it as it is
    /// (Go does not touch the status when the picker is cancelled).
    /// Go also shows an error dialog on a refusal; this trial has no dialog.
    pub fn status_for(outcome: &Outcome) -> Option<&'static str> {
        match outcome {
            Outcome::Chosen => Some("Status: Input loaded"),
            Outcome::Cancelled => None,
            Outcome::Rejected => Some("Status: Invalid input"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mp4_extension_is_case_insensitive() {
        for ok in [
            "a.mp4",
            "A.MP4",
            ".mp4",
            "/x/y/.MP4",
            "/x/y/clip.Mp4",
            "C:/Users/u/a.mp4",
        ] {
            assert!(is_mp4(Path::new(ok)), "{ok} should be accepted");
        }
    }

    #[test]
    fn other_extensions_are_refused() {
        for bad in [
            "a.mkv",
            "a.mp4.txt",
            "mp4",
            ".mp4x",
            "amp4",
            "a.",
            "dir/",
            "",
        ] {
            assert!(!is_mp4(Path::new(bad)), "{bad:?} should be refused");
        }
    }

    #[test]
    fn a_chosen_mp4_becomes_the_input() {
        let mut s = Selection::default();
        assert!(s.begin_pick());
        assert_eq!(s.finish_pick(Some("/v/a.mp4".into())), Outcome::Chosen);
        assert_eq!(s.input_text(), "a.mp4", "the field shows the name only");
        assert_eq!(
            s.input(),
            Some(Path::new("/v/a.mp4")),
            "the state keeps the path"
        );
        assert!(!s.is_picking());
    }

    #[test]
    fn a_non_mp4_keeps_the_previous_input() {
        let mut s = Selection::default();
        s.finish_pick(Some("/v/a.mp4".into()));
        assert_eq!(s.finish_pick(Some("/v/b.mkv".into())), Outcome::Rejected);
        assert_eq!(s.input_text(), "a.mp4");
    }

    #[test]
    fn cancelling_keeps_the_previous_input_and_releases_the_picker() {
        let mut s = Selection::default();
        s.finish_pick(Some("/v/a.mp4".into()));
        assert!(s.begin_pick());
        assert_eq!(s.finish_pick(None), Outcome::Cancelled);
        assert_eq!(s.input_text(), "a.mp4");
        assert!(s.begin_pick(), "the picker must be reusable after a cancel");
    }

    #[test]
    fn a_second_picker_cannot_open_while_one_is_open() {
        let mut s = Selection::default();
        assert!(s.begin_pick());
        assert!(!s.begin_pick());
        assert!(s.is_picking());
    }

    #[test]
    fn the_field_is_empty_before_any_choice() {
        assert_eq!(Selection::default().input_text(), "");
    }

    #[test]
    fn statuses_follow_go_and_cancel_leaves_the_status_alone() {
        assert_eq!(
            Selection::status_for(&Outcome::Chosen),
            Some("Status: Input loaded")
        );
        assert_eq!(
            Selection::status_for(&Outcome::Rejected),
            Some("Status: Invalid input")
        );
        assert_eq!(Selection::status_for(&Outcome::Cancelled), None);
    }
}
