//! Guards on files of the repository that no compiler reads: the README anchor the
//! application links to, the workflow files and the window's fixed size. Ports of two Go tests that lived beside
//! the Go window (removed in phase 8; last release with it: v0.2.9).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::app::INSTALL_URL;

fn repo_file(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join(relative)
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path:?}: {e}"))
}

/// How GitHub turns a heading into a fragment: lower-case, punctuation dropped, spaces to
/// hyphens. Letters keep their accents, so this cannot simply strip everything non-ASCII.
fn github_slug(title: &str) -> String {
    title
        .to_lowercase()
        .chars()
        .filter_map(|c| match c {
            ' ' => Some('-'),
            '-' | '_' => Some(c),
            c if c.is_ascii_digit() || c.is_ascii_lowercase() => Some(c),
            c if !c.is_ascii() => Some(c),
            _ => None,
        })
        .collect()
}

/// The slug of every Markdown heading, skipping fenced code blocks so that a `#` opening a
/// comment inside one is not mistaken for a heading.
fn heading_slugs(markdown: &str) -> HashSet<String> {
    let mut in_fence = false;
    let mut slugs = HashSet::new();
    for line in markdown.lines() {
        if line.trim().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence || !line.starts_with('#') {
            continue;
        }
        let title = line.trim_start_matches('#').trim();
        if !title.is_empty() {
            slugs.insert(github_slug(title));
        }
    }
    slugs
}

fn fragment(url: &str) -> &str {
    url.split_once('#').map_or("", |(_, f)| f)
}

#[test]
fn the_slug_follows_github() {
    assert_eq!(github_slug("Download and install"), "download-and-install");
    assert_eq!(
        github_slug("Informations complémentaires"),
        "informations-complémentaires"
    );
    assert_eq!(github_slug("What's new? (v2)"), "whats-new-v2");
    let found = heading_slugs("# Top\n```\n# not a heading\n```\n## Second part\n");
    assert_eq!(
        found,
        HashSet::from(["top".to_string(), "second-part".to_string()])
    );
}

// Go: TestInstallURLFragmentPointsAtARealHeading. A fragment that matches no heading does
// not fail: it silently drops the reader, who has no ffmpeg, at the top of the README.
#[test]
fn the_install_url_fragment_points_at_a_real_heading() {
    let wanted = fragment(INSTALL_URL);
    assert!(!wanted.is_empty(), "INSTALL_URL has no fragment");
    let slugs = heading_slugs(&read(&repo_file("README.md")));
    assert!(
        slugs.contains(wanted),
        "INSTALL_URL points at #{wanted}, which is not a heading of README.md; headings: {slugs:?}"
    );
}

// The error text of the core carries its own copy of the URL: it must be the same one.
// Unix only: on Windows the system search finds the machine's own ffmpeg whatever the
// injected environment says (see `format.rs`, `is_missing_ffmpeg_error_matches_…`).
#[cfg(unix)]
#[test]
fn the_missing_ffmpeg_error_gives_the_same_install_url() {
    use superview_core::config::Config;
    use superview_core::tools::{ToolResolver, check_ffmpeg};

    let nothing = ToolResolver::new(|_| None, None);
    let err = check_ffmpeg(&Config::default(), &nothing).expect_err("no tool can be found");
    assert!(
        err.to_string().contains(INSTALL_URL),
        "the error text does not carry INSTALL_URL: {err}"
    );
}

/// A tool's output being cut short by the next stage of a pipe: `ffmpeg -version | head -n 1`
/// in bash, `... | Select-Object -First 1` in PowerShell. The tool writes to a closed pipe and
/// exits non-zero (141 under bash), so a step that checks its status fails although the tool
/// works (L-115).
fn cuts_a_tools_output(line: &str) -> bool {
    let word_end = |rest: &str| !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_');
    let mut from = 0;
    while let Some(at) = line[from..].find("-version") {
        let after = line[from + at + "-version".len()..].trim_start();
        if let Some(next) = after.strip_prefix('|') {
            let next = next.trim_start();
            if let Some(rest) = next.strip_prefix("head")
                && word_end(rest)
            {
                return true;
            }
            if let Some(rest) = next.strip_prefix("Select-Object")
                && rest.starts_with(char::is_whitespace)
                && let Some(flag) = rest.trim_start().strip_prefix("-First")
                && word_end(flag)
            {
                return true;
            }
        }
        from += at + "-version".len();
    }
    false
}

#[test]
fn the_pipe_matcher_knows_the_cut_forms() {
    for cut in [
        "run: ffmpeg -version | head -n 1",
        "ffmpeg -version|head -n 1",
        "  ffprobe -version  |  Select-Object -First 1",
    ] {
        assert!(cuts_a_tools_output(cut), "{cut}");
    }
    for fine in [
        "ffmpeg -version | headroom",
        "out=$(ffmpeg -version); echo \"$out\" | head -n 1",
        "ffmpeg -version | Select-Object -Firstly",
        "ffmpeg -version > out.txt",
    ] {
        assert!(!cuts_a_tools_output(fine), "{fine}");
    }
}

// Go: TestWorkflows_DoNotCutAToolsOutputShort (L-115): a step must capture a tool's whole
// output before keeping its first line. Comment lines are skipped.
#[test]
fn workflows_do_not_cut_a_tools_output_short() {
    let dir = repo_file(".github/workflows");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{dir:?}: {e}"))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "yml"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no workflow files found in {dir:?}");
    let mut offenders = Vec::new();
    for file in &files {
        for (i, line) in read(file).lines().enumerate() {
            if !line.trim_start().starts_with('#') && cuts_a_tools_output(line) {
                offenders.push(format!("{}:{}: {}", file.display(), i + 1, line.trim()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "these cut a tool's output short; capture it whole first (L-115):\n{}",
        offenders.join("\n")
    );
}

/// The value of a `<name>: <N>px;` line written at the window's own indentation (four
/// spaces) in `main.slint`; nested items use deeper indentation.
fn window_constraint(slint: &str, name: &str) -> Option<f32> {
    let prefix = format!("    {name}: ");
    slint.lines().find_map(|line| {
        line.strip_prefix(&prefix)?
            .strip_suffix("px;")?
            .parse()
            .ok()
    })
}

/// The window has one size (DEC-54): its minimum and maximum are equal, so a desktop cannot
/// resize it. The testing backend applies no such constraint, so only the source can say.
#[test]
fn the_window_has_one_size_so_it_cannot_be_resized() {
    let slint = read(&repo_file("rust/crates/superview-app/ui/main.slint"));
    for axis in ["width", "height"] {
        let min = window_constraint(&slint, &format!("min-{axis}"))
            .unwrap_or_else(|| panic!("main.slint sets no window min-{axis}"));
        let max = window_constraint(&slint, &format!("max-{axis}"))
            .unwrap_or_else(|| panic!("main.slint sets no window max-{axis}"));
        assert_eq!(min, max, "the window's min-{axis} and max-{axis} differ");
    }
}
