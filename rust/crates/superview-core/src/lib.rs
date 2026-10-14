//! Logique de Superview, sans interface graphique (DEC-43).
//!
//! Le squelette ne contient encore que la version ; les modules arrivent un par PR,
//! chacun avec ses tests (voir le plan de portage).

/// Version du cœur, celle du workspace.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_follows_cargo_manifest() {
        assert_eq!(version(), env!("CARGO_PKG_VERSION"));
        assert!(!version().is_empty());
    }
}
