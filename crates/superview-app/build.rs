#[path = "build_support.rs"]
mod build_support;

use std::path::PathBuf;
use std::process::Command;

fn main() {
    compile_window();
    stamp_build();
    embed_windows_resources();
    link_for_appimage();
}

/// The Linux binary looks for its bundled libraries in `../lib` (the AppImage's `usr/lib`)
/// before the system's, DEC-47. Only the binary (`rustc-link-arg-bins`: not the tests, not
/// the build scripts) and only on Linux. `--disable-new-dtags` makes it a DT_RPATH, not a
/// RUNPATH: a RUNPATH is not searched for the dependencies of a library the program loads
/// with `dlopen` (winit does, for libxkbcommon-x11), a DT_RPATH is. No `LD_LIBRARY_PATH`:
/// it would leak to ffmpeg, zenity and kdialog.
fn link_for_appimage() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo:rustc-link-arg-bins=-Wl,--disable-new-dtags");
        println!("cargo:rustc-link-arg-bins=-Wl,-rpath,$ORIGIN/../lib");
    }
}

fn compile_window() {
    // The window tests find elements through `ElementHandle`, which the testing backend can
    // only walk when the generated code carries debug info (measured: without it, every
    // search comes back empty). The release binary does not need it and is built without.
    let debug_info = std::env::var("PROFILE").is_ok_and(|profile| profile != "release");
    // The window icon is `ui/Icon.png`. It is embedded in the binary: by default Slint would
    // look for it at its build-machine path when the program runs. (`EmbedForSoftwareRenderer`,
    // which decodes at build time, also embeds the glyphs and does not compile.)
    let config = slint_build::CompilerConfiguration::new()
        .with_debug_info(debug_info)
        .embed_resources(slint_build::EmbedResourcesKind::EmbedFiles);
    slint_build::compile_with_config("ui/main.slint", config)
        .expect("failed to compile ui/main.slint");
}

/// The version comes from the release (`SUPERVIEW_VERSION`, DEC-22), the revision from git.
/// No git, or a git that fails, means no revision: the same as a build without vcs information.
fn stamp_build() {
    println!("cargo:rerun-if-env-changed=SUPERVIEW_VERSION");
    let raw = std::env::var("SUPERVIEW_VERSION").ok();
    let version = match build_support::stamped_version(raw.as_deref()) {
        Ok(version) => version,
        Err(message) => panic!("{message}"),
    };
    println!(
        "cargo:rustc-env=SUPERVIEW_BUILD_VERSION={}",
        version.map_or_else(|| "dev".to_string(), |(text, _)| text)
    );
    println!(
        "cargo:rustc-env=SUPERVIEW_BUILD_STAMPED={}",
        raw.as_deref().is_some_and(|raw| !raw.is_empty())
    );
    let revision = git(&["rev-parse", "HEAD"]).map(|out| out.trim().to_string());
    let modified = git(&["--no-optional-locks", "status", "--porcelain"])
        .is_some_and(|out| !out.trim().is_empty());
    // Re-stamp when the commit moves. `modified` does not follow every edit locally: an edit
    // that touches neither the index nor HEAD does not rerun this script; a release builds
    // from a fresh checkout, where it is exact.
    if revision.is_some() {
        let mut watched = vec![
            git(&["rev-parse", "--git-path", "HEAD"]),
            git(&["rev-parse", "--git-path", "index"]),
        ];
        if let Some(head_ref) = git(&["symbolic-ref", "-q", "HEAD"]) {
            watched.push(git(&["rev-parse", "--git-path", head_ref.trim()]));
        }
        // If git fails on any of them, emit nothing rather than a partial list.
        if watched.iter().all(Option::is_some) {
            for path in watched.into_iter().flatten() {
                println!("cargo:rerun-if-changed={}", resolve_git_path(path.trim()));
            }
        }
    }
    // An empty revision is "no vcs information"; `modified` is meaningless then.
    println!(
        "cargo:rustc-env=SUPERVIEW_BUILD_REVISION={}",
        revision.as_deref().unwrap_or("")
    );
    println!(
        "cargo:rustc-env=SUPERVIEW_BUILD_MODIFIED={}",
        revision.is_some() && modified
    );
}

/// `--git-path` answers relative to the directory git ran in (the manifest directory).
fn resolve_git_path(path: &str) -> String {
    let path = std::path::Path::new(path);
    match std::env::var_os("CARGO_MANIFEST_DIR") {
        Some(dir) if path.is_relative() => PathBuf::from(dir).join(path).display().to_string(),
        _ => path.display().to_string(),
    }
}

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(std::env::var_os("CARGO_MANIFEST_DIR")?)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

/// The icon of `superview.exe` (the same file the installer uses) and its version resource.
/// Only for the MSVC target: the GNU one needs `windres`, which a cross-check machine lacks.
fn embed_windows_resources() {
    let target = |key| std::env::var(key).unwrap_or_default();
    if target("CARGO_CFG_TARGET_OS") != "windows" || target("CARGO_CFG_TARGET_ENV") != "msvc" {
        return;
    }
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let icon = manifest.join("../../packaging/windows/superview.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    let raw = std::env::var("SUPERVIEW_VERSION").ok();
    let (text, [major, minor, patch]) = build_support::stamped_version(raw.as_deref())
        .unwrap_or_else(|message| panic!("{message}"))
        .unwrap_or_else(|| ("0.0.0".to_string(), [0, 0, 0]));
    let packed = (u64::from(major) << 48) | (u64::from(minor) << 32) | (u64::from(patch) << 16);
    let mut resource = winresource::WindowsResource::new();
    resource
        .set_icon(icon.to_str().expect("icon path is UTF-8"))
        .set_version_info(winresource::VersionInfo::FILEVERSION, packed)
        .set_version_info(winresource::VersionInfo::PRODUCTVERSION, packed)
        .set("FileVersion", &text)
        .set("ProductVersion", &text)
        .set("FileDescription", "Superview")
        .set("ProductName", "Superview")
        .set("OriginalFilename", "superview.exe");
    resource
        .compile()
        .expect("failed to embed the Windows resources");
}
