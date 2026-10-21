// Shared by `build.rs` (which `#[path]`-includes it) and by the crate itself under `cfg(test)`,
// so that the rule is the code the build script runs AND is covered by `cargo test`.

/// Reads the version the release stamps through `SUPERVIEW_VERSION`.
///
/// `None` or empty means "no version injected" (a plain `cargo build`): the binary is `dev`.
/// Anything else must be exactly three groups of ASCII digits, `x.y.z`: no `v` prefix, no
/// suffix such as `-rc1`, no fourth group. Leading zeros are accepted (`01.2.3`), as releases
/// up to v0.2.9 accepted them; each group must fit the 16 bits of a Windows version resource.
/// A bad value is an error so that a mistyped release fails at build time, not after publishing.
#[allow(dead_code)] // in the test build only the tests call it
pub fn stamped_version(raw: Option<&str>) -> Result<Option<(String, [u16; 3])>, String> {
    let Some(raw) = raw.filter(|raw| !raw.is_empty()) else {
        return Ok(None);
    };
    let bad = || format!("SUPERVIEW_VERSION must be x.y.z (digits only, e.g. 0.3.0), got {raw:?}");
    let groups: Vec<&str> = raw.split('.').collect();
    if groups.len() != 3 {
        return Err(bad());
    }
    let mut numbers = [0u16; 3];
    for (number, group) in numbers.iter_mut().zip(&groups) {
        if group.is_empty() || !group.bytes().all(|b| b.is_ascii_digit()) {
            return Err(bad());
        }
        *number = group.parse().map_err(|_| bad())?;
    }
    Ok(Some((raw.to_string(), numbers)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(raw: &str) -> Option<String> {
        stamped_version(Some(raw)).unwrap().map(|(text, _)| text)
    }

    #[test]
    fn a_plain_x_y_z_is_stamped() {
        assert_eq!(ok("0.3.0").as_deref(), Some("0.3.0"));
        assert_eq!(
            stamped_version(Some("12.0.345")).unwrap().unwrap().1,
            [12, 0, 345]
        );
    }

    #[test]
    fn absent_or_empty_means_dev() {
        assert_eq!(stamped_version(None).unwrap(), None);
        assert_eq!(stamped_version(Some("")).unwrap(), None);
    }

    #[test]
    fn leading_zeros_are_accepted_as_the_go_build_did() {
        assert_eq!(ok("01.2.3").as_deref(), Some("01.2.3"));
    }

    #[test]
    fn anything_else_is_refused() {
        for bad in [
            "1.2",
            "1.2.3.4",
            "v1.2.3",
            "1.2.3-rc1",
            "1.2.x",
            "1..3",
            ".1.2",
            "1.2.",
            " 1.2.3",
            "1.2.3 ",
            "+1.2.3",
            "-1.2.3",
            "１.2.3",
            "70000.0.0",
            "dev",
        ] {
            let err = stamped_version(Some(bad)).expect_err(bad);
            assert!(err.contains("x.y.z") && err.contains(bad), "{err}");
        }
    }
}
