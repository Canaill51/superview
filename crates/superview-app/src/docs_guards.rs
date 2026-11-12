//! Guards on the documentation, on the model of OpenTuneBox's `docs.rs`: what a reader follows
//! (a relative link, a path cited between backticks) leads somewhere, `docs/README.md` indexes
//! every document, and the numbered registers — `DEC-xx` in `decisions.md`, `L-xxx` in
//! `lecons.md`, the findings (`U-03`, `E-24`…) of `analyse.md` and its archive — give each
//! number once and know every number cited anywhere in the repository.
//!
//! Whether a text is still *true* is beyond a test: that is the `docs-sync` skill
//! (`.claude/skills/docs-sync/SKILL.md`). Each check is a function of text with its own
//! counter-proof on a fabricated case (the `*_catches_*` tests); a failure means a document to
//! fix, not a check to weaken.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::repo_guards::heading_slugs;

/// Not canonicalised: on Windows that gives a `\\?\C:\…` path, in which the `/` of the
/// relative paths joined to it are no longer separators.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A file as text with `\n` line ends (a Windows checkout may have `\r\n`), or `None` when it
/// is not UTF-8 (an image, a font, a recorded video).
fn read_text(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
    String::from_utf8(bytes)
        .ok()
        .map(|t| t.replace("\r\n", "\n"))
}

fn read(path: &Path) -> String {
    read_text(path).unwrap_or_else(|| panic!("{path:?} is not UTF-8"))
}

/// Every file git tracks, relative, with `/`. Panics rather than check fewer files without
/// saying so.
fn tracked(root: &Path) -> Vec<String> {
    let out = Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(root)
        .output()
        .expect("git ls-files");
    assert!(out.status.success(), "git ls-files failed: {out:?}");
    let files: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .split('\0')
        .filter(|f| !f.is_empty() && root.join(f).is_file())
        .map(String::from)
        .collect();
    assert!(files.len() > 100, "only {} tracked files", files.len());
    files
}

fn markdown_files(root: &Path) -> Vec<String> {
    tracked(root)
        .into_iter()
        .filter(|f| f.ends_with(".md"))
        .collect()
}

/// Runs `check` on each file and prefixes each problem with the file's name.
fn each(files: &[String], mut check: impl FnMut(&str) -> Vec<String>) -> Vec<String> {
    files
        .iter()
        .flat_map(|f| {
            check(f)
                .into_iter()
                .map(move |p| format!("{f}: {p}"))
                .collect::<Vec<_>>()
        })
        .collect()
}

// ---------------------------------------------------------------------------------- Markdown

/// The text with the lines of fenced code blocks blanked, so that nothing in an example counts.
fn strip_fences(markdown: &str) -> String {
    let mut in_fence = false;
    markdown
        .lines()
        .map(|line| {
            if line.trim_start().starts_with("```") {
                in_fence = !in_fence;
                ""
            } else if in_fence {
                ""
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The inline code spans of a text already stripped of its fences.
fn code_spans(text: &str) -> Vec<&str> {
    text.lines()
        .flat_map(|line| line.split('`').skip(1).step_by(2))
        .collect()
}

/// The text outside inline code spans, which is where a link can be.
fn outside_code_spans(text: &str) -> String {
    text.lines()
        .map(|line| line.split('`').step_by(2).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The target of every inline link `[text](target)`, title and angle brackets removed.
fn link_targets(markdown: &str) -> Vec<String> {
    let text = outside_code_spans(&strip_fences(markdown));
    text.match_indices("](")
        .filter_map(|(at, _)| {
            let rest = &text[at + 2..];
            let end = rest.find(')')?;
            let inner = rest[..end].trim();
            let target = inner
                .split_whitespace()
                .next()?
                .trim_start_matches('<')
                .trim_end_matches('>');
            Some(target.to_string())
        })
        .collect()
}

/// The fragments a Markdown file offers: its headings, as GitHub slugs them, and any
/// `<a id="…">` or `<a name="…">`.
fn anchors(markdown: &str) -> HashSet<String> {
    let mut found = heading_slugs(markdown);
    for marker in ["<a id=\"", "<a name=\""] {
        for (at, _) in markdown.match_indices(marker) {
            let rest = &markdown[at + marker.len()..];
            if let Some(end) = rest.find('"') {
                found.insert(rest[..end].to_string());
            }
        }
    }
    found
}

/// Relative links of `file` (a path from the root) whose file, or heading, does not exist.
/// Web links and GitHub pages reached by climbing out of the repository are not checked.
fn broken_links(root: &Path, file: &str, markdown: &str) -> Vec<String> {
    let dir = Path::new(file).parent().unwrap_or(Path::new(""));
    let mut broken = Vec::new();
    for target in link_targets(markdown) {
        if ["http://", "https://", "mailto:"]
            .iter()
            .any(|p| target.starts_with(p))
        {
            continue;
        }
        let (path, fragment) = target.split_once('#').unwrap_or((&target, ""));
        let dest = if path.is_empty() {
            PathBuf::from(file)
        } else {
            dir.join(path)
        };
        if !normalise(&dest).is_some_and(|d| root.join(d).exists()) {
            broken.push(format!("{target} (no such file)"));
            continue;
        }
        let is_markdown = dest.extension().is_some_and(|e| e == "md");
        if !fragment.is_empty()
            && is_markdown
            && !anchors(&read(&root.join(&dest))).contains(fragment)
        {
            broken.push(format!("{target} (no such heading)"));
        }
    }
    broken
}

/// `a/b/../c` as `a/c`; `None` when the path climbs out of the repository (a GitHub page such
/// as `../../releases` is not a file of the checkout).
fn normalise(path: &Path) -> Option<PathBuf> {
    let mut parts: Vec<&std::ffi::OsStr> = Vec::new();
    for part in path.components() {
        match part {
            std::path::Component::ParentDir => {
                parts.pop()?;
            }
            std::path::Component::CurDir => {}
            other => parts.push(other.as_os_str()),
        }
    }
    Some(parts.iter().collect())
}

// ---------------------------------------------------------------------------------- cited paths

/// The first segment of a cited path that makes it a path of this repository. A crate's own
/// name (`superview-core/src/session.rs`) is read under `crates/`, as the documents write it.
const REPO_ROOTS: [&str; 7] = [
    "crates/",
    "docs/",
    "scripts/",
    "packaging/",
    ".github/",
    ".claude/",
    ".githooks/",
];
const CRATES: [&str; 4] = [
    "superview-core/",
    "superview-app/",
    "superview-harness/",
    "superview-ofx/",
];

/// Removed files that a current document names on purpose, to say they are gone.
const REMOVED_PATHS: [&str; 1] = [
    // `docs/contrats.md`: its content moved to AGENTS.md; do not recreate it.
    ".github/copilot-instructions.md",
];

/// Documents that tell the project's history: they cite files that were renamed or removed
/// (the Go code until DEC-49, plans since archived), and their texts are not rewritten
/// (DEC-55). Every other document describes the repository as it is.
const HISTORICAL_DOCS: [&str; 5] = [
    "docs/analyse.md",
    "docs/corrections.md",
    "docs/decisions.md",
    "docs/lecons.md",
    "docs/archives/",
];

fn is_historical(file: &str) -> bool {
    HISTORICAL_DOCS.iter().any(|h| file.starts_with(h))
}

/// The item is declared in Rust source: `fn`, `struct`, `enum`, `trait`, `const`, `static`,
/// `type` or `mod` followed by the name as a whole word.
fn rust_item_defined(source: &str, name: &str) -> bool {
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    [
        "fn", "struct", "enum", "trait", "const", "static", "type", "mod",
    ]
    .iter()
    .any(|kw| {
        let needle = format!("{kw} {name}");
        source.match_indices(&needle).any(|(at, _)| {
            let before = source[..at].chars().next_back();
            let after = source[at + needle.len()..].chars().next();
            !before.is_some_and(ident) && !after.is_some_and(ident)
        })
    })
}

/// Paths cited between backticks (`crates/superview-core/src/session.rs`, optionally with
/// `:item` or `:line`) that do not exist, or whose Rust item does not. Only paths under a
/// known root are read; anything with a placeholder (`<mod>`, `*`) or a space is prose.
fn missing_paths(root: &Path, markdown: &str) -> Vec<String> {
    let plain = |c: char| c.is_alphanumeric() || "._/-".contains(c);
    let mut missing = Vec::new();
    for span in code_spans(&strip_fences(markdown)) {
        let (path, item) = span.split_once(':').unwrap_or((span, ""));
        if !path.contains('/') || !path.chars().all(plain) {
            continue;
        }
        let file = if REPO_ROOTS.iter().any(|r| path.starts_with(r)) {
            root.join(path)
        } else if CRATES.iter().any(|c| path.starts_with(c)) {
            root.join("crates").join(path)
        } else {
            continue;
        };
        if !file.exists() && !REMOVED_PATHS.contains(&path) {
            missing.push(span.to_string());
        } else if !item.is_empty()
            && item.chars().all(|c| c.is_alphanumeric() || c == '_')
            && !item.chars().all(|c| c.is_ascii_digit())
            && file.extension().is_some_and(|e| e == "rs")
            && !rust_item_defined(&read(&file), item)
        {
            missing.push(format!("{span} (no such item)"));
        }
    }
    missing
}

// ---------------------------------------------------------------------------------- the index

/// Documents of `docs/` (not `README.md` itself) that `docs/README.md` does not link to.
fn index_missing(names: &[String], index: &str) -> Vec<String> {
    names
        .iter()
        .filter(|n| *n != "README.md" && !index.contains(&format!("]({n})")))
        .cloned()
        .collect()
}

// ---------------------------------------------------------------------------------- registers

/// A number at the start of `text`: an upper-case prefix of one to three letters, a hyphen,
/// two or three digits, and no letter or digit right after. `DEC-xx` (the template) is not one.
fn id_at_start(text: &str) -> Option<&str> {
    let letters = text.chars().take_while(char::is_ascii_uppercase).count();
    if !(1..=3).contains(&letters) || !text[letters..].starts_with('-') {
        return None;
    }
    let digits = text[letters + 1..]
        .chars()
        .take_while(char::is_ascii_digit)
        .count();
    let end = letters + 1 + digits;
    let next = text[end..].chars().next();
    ((2..=3).contains(&digits) && !next.is_some_and(char::is_alphanumeric)).then(|| &text[..end])
}

fn prefix(id: &str) -> &str {
    id.split('-').next().unwrap_or(id)
}

/// Every number cited in `text` whose prefix is a register's; a number glued to a word
/// (`SHA-256`, `x86-64`) is not one.
fn cited_ids(text: &str, prefixes: &BTreeSet<&str>) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut previous: Option<char> = None;
    for (at, c) in text.char_indices() {
        if c.is_ascii_uppercase()
            && !previous.is_some_and(char::is_alphanumeric)
            && let Some(id) = id_at_start(&text[at..])
            && prefixes.contains(prefix(id))
        {
            found.insert(id.to_string());
        }
        previous = Some(c);
    }
    found
}

/// The entries a register defines: a heading of level 3 or 4 whose title opens on the number,
/// after any status mark (`🔴`, `✅`, `~~`). A heading that groups several numbers
/// (`N-03 / N-04`) or answers one already opened (`Q-01 — Réponse…`) defines nothing.
fn register_entries(markdown: &str) -> Vec<String> {
    strip_fences(markdown)
        .lines()
        .filter(|l| l.starts_with("### ") || l.starts_with("#### "))
        .filter(|l| !l.contains("— Réponse"))
        .filter_map(|l| {
            let title = l
                .trim_start_matches('#')
                .trim_start_matches(|c: char| !c.is_ascii_alphanumeric());
            let id = id_at_start(title)?;
            (!title[id.len()..].starts_with(" /")).then(|| id.to_string())
        })
        .collect()
}

fn duplicates(entries: &[String]) -> Vec<String> {
    let mut count: BTreeMap<&str, usize> = BTreeMap::new();
    for e in entries {
        *count.entry(e).or_default() += 1;
    }
    count
        .into_iter()
        .filter(|(_, n)| *n > 1)
        .map(|(e, _)| e.to_string())
        .collect()
}

/// The numbers listed in a register's index table (`| [DEC-01](#dec-01) | …`).
fn index_rows(markdown: &str) -> BTreeSet<String> {
    markdown
        .lines()
        .filter_map(|l| id_at_start(l.strip_prefix("| [")?).map(String::from))
        .collect()
}

/// The registers, read from the repository: `(name, entries)`.
fn registers(root: &Path) -> Vec<(&'static str, Vec<String>)> {
    let text = |f: &str| read(&root.join(f));
    let findings = text("docs/analyse.md") + "\n" + &text("docs/archives/analyse-passes-3-a-13.md");
    let registers = vec![
        (
            "docs/decisions.md",
            register_entries(&text("docs/decisions.md")),
        ),
        ("docs/lecons.md", register_entries(&text("docs/lecons.md"))),
        (
            "findings of docs/analyse.md and its archive",
            register_entries(&findings),
        ),
    ];
    // A pattern that stops matching would turn every check below green on an empty register.
    for (name, entries) in &registers {
        assert!(
            entries.len() > 50,
            "{name}: only {} entries read",
            entries.len()
        );
    }
    registers
}

// ---------------------------------------------------------------------------------- the checks

#[test]
fn relative_links_lead_to_a_file_and_a_heading() {
    let root = root();
    let files = markdown_files(&root);
    // A parser that stops seeing links would pass on anything.
    let links: usize = files
        .iter()
        .map(|f| link_targets(&read(&root.join(f))).len())
        .sum();
    assert!(links > 200, "only {links} links read");
    let bad = each(&files, |f| broken_links(&root, f, &read(&root.join(f))));
    assert!(bad.is_empty(), "broken links: {bad:#?}");
}

#[test]
fn cited_paths_exist() {
    let root = root();
    let current: Vec<String> = markdown_files(&root)
        .into_iter()
        .filter(|f| !is_historical(f))
        .collect();
    assert!(current.len() > 10, "documents read: {current:?}");
    let spans: usize = current
        .iter()
        .map(|f| code_spans(&strip_fences(&read(&root.join(f)))).len())
        .sum();
    assert!(spans > 500, "only {spans} code spans read");
    let bad = each(&current, |f| missing_paths(&root, &read(&root.join(f))));
    assert!(bad.is_empty(), "cited paths that do not exist: {bad:#?}");
}

#[test]
fn the_docs_index_lists_every_document() {
    let root = root();
    let names: Vec<String> = std::fs::read_dir(root.join("docs"))
        .expect("docs/")
        .flatten()
        .filter(|e| e.path().is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".md"))
        .collect();
    assert!(names.len() > 5, "documents found: {names:?}");
    let missing = index_missing(&names, &read(&root.join("docs/README.md")));
    assert!(
        missing.is_empty(),
        "absent from docs/README.md: {missing:?}"
    );
}

#[test]
fn each_number_is_given_once() {
    for (name, entries) in registers(&root()) {
        let twice = duplicates(&entries);
        assert!(
            twice.is_empty(),
            "{name} gives these numbers twice: {twice:?}"
        );
    }
}

#[test]
fn the_decision_index_lists_every_decision() {
    let decisions = read(&root().join("docs/decisions.md"));
    let entries: BTreeSet<String> = register_entries(&decisions).into_iter().collect();
    let index = index_rows(&decisions);
    let unlisted: Vec<_> = entries.difference(&index).collect();
    let extra: Vec<_> = index.difference(&entries).collect();
    assert!(
        unlisted.is_empty() && extra.is_empty(),
        "decisions.md index: missing {unlisted:?}, without an entry {extra:?}"
    );
}

#[test]
fn every_lesson_has_its_short_rule_by_domain() {
    let root = root();
    let lessons = register_entries(&read(&root.join("docs/lecons.md")));
    let prefixes = BTreeSet::from(["L"]);
    let ruled = cited_ids(&read(&root.join("docs/pieges-par-domaine.md")), &prefixes);
    let missing: Vec<_> = lessons.iter().filter(|l| !ruled.contains(*l)).collect();
    assert!(
        missing.is_empty(),
        "absent from docs/pieges-par-domaine.md: {missing:?}"
    );
}

#[test]
fn every_cited_number_exists() {
    let root = root();
    let known: BTreeSet<String> = registers(&root).into_iter().flat_map(|r| r.1).collect();
    let prefixes: BTreeSet<&str> = known.iter().map(|id| prefix(id)).collect();
    let mut unknown = Vec::new();
    let mut cited = 0;
    for file in tracked(&root) {
        let Some(text) = read_text(&root.join(&file)) else {
            continue;
        };
        let ids = cited_ids(&text, &prefixes);
        cited += ids.len();
        unknown.extend(
            ids.into_iter()
                .filter(|id| !known.contains(id))
                .map(|id| format!("{file}: {id}")),
        );
    }
    assert!(cited > 1000, "only {cited} citations read");
    assert!(
        unknown.is_empty(),
        "numbers cited but defined nowhere: {unknown:#?}"
    );
}

// ---------------------------------------------------------------------------------- counter-proofs

#[test]
fn the_link_check_catches_a_missing_file_and_a_missing_heading() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("docs")).unwrap();
    std::fs::write(dir.path().join("docs/a.md"), "# Title\n## Second part\n").unwrap();
    let text = "[ok](a.md#second-part) [self](#title) [web](https://x.org/y) \
                [gone](b.md) [lost](a.md#third-part) `[not a link](c.md)`\n\
                ```\n[in a fence](d.md)\n```\n";
    std::fs::write(dir.path().join("docs/b2.md"), "# Title\n").unwrap();
    let found = broken_links(dir.path(), "docs/b2.md", text);
    assert_eq!(
        found,
        ["b.md (no such file)", "a.md#third-part (no such heading)"]
    );
    assert_eq!(
        broken_links(dir.path(), "docs/b2.md", "[up](../../releases)"),
        ["../../releases (no such file)"]
    );
}

#[test]
fn the_path_check_catches_a_missing_file_and_a_missing_item() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("crates/superview-core/src")).unwrap();
    std::fs::write(
        dir.path().join("crates/superview-core/src/session.rs"),
        "pub struct EncodingSession;\npub fn closed() {}\n",
    )
    .unwrap();
    let text = "`superview-core/src/session.rs:EncodingSession` \
                `crates/superview-core/src/session.rs:12` `superview-core/src/session.rs:close` \
                `docs/gone.md` `src/<mod>/tests.rs` `tests/pgm_golden.rs` `a/b c`";
    assert_eq!(
        missing_paths(dir.path(), text),
        [
            "superview-core/src/session.rs:close (no such item)",
            "docs/gone.md"
        ]
    );
}

#[test]
fn the_index_check_catches_an_unlisted_document() {
    let names = ["README.md", "plan.md", "lecons.md"].map(String::from);
    assert_eq!(
        index_missing(
            &names,
            "| [plan.md](plan.md) | … |\nlecons.md without a link"
        ),
        ["lecons.md"]
    );
}

#[test]
fn the_register_checks_catch_a_number_given_twice_and_one_cited_but_undefined() {
    let register = "### DEC-01\n#### 🔴 B-01 — Bug\n### N-03 ✅ — ~~Fixed~~\n\
                    ### N-03 / N-04 — one fix\n### Q-01 — Réponse\n### DEC-xx — template\n\
                    ### DEC-01\n";
    let entries = register_entries(register);
    assert_eq!(entries, ["DEC-01", "B-01", "N-03", "DEC-01"]);
    assert_eq!(duplicates(&entries), ["DEC-01"]);
    assert_eq!(
        index_rows("| [DEC-01](#dec-01) | x |\n| ID | y |\n"),
        BTreeSet::from(["DEC-01".to_string()])
    );

    let prefixes = BTreeSet::from(["DEC", "L", "E"]);
    let cited = cited_ids(
        "DEC-01, (L-117) E-24. SHA-256 x86-64 L-1 DEC-xx EDEC-02 L-1234",
        &prefixes,
    );
    assert_eq!(
        cited,
        BTreeSet::from(["DEC-01", "E-24", "L-117"].map(String::from))
    );
}

#[test]
fn rust_items_are_found_as_whole_words() {
    let source = "pub fn check_video() {}\nstruct EncodingSessionX;\n";
    assert!(rust_item_defined(source, "check_video"));
    assert!(!rust_item_defined(source, "check"));
    assert!(!rust_item_defined(source, "EncodingSession"));
}
