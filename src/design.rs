//! A project's design system, read from the Claude Design shape in `design/`.
//!
//! The on-disk layout is the source of truth (see `design/README.md`):
//!
//! ```text
//! design/
//! ├── design-system.json          metadata (title, namespace, last change)
//! ├── tokens.json                 color, type, spacing, radius, size, shadow
//! └── components/<Name>/
//!     ├── README.md               what the component is for
//!     └── preview.html            a standalone rendering
//! ```
//!
//! `idesign` and iboard read it through here so they never disagree. This is
//! the canonical store: an Open Design package is an import source that maps
//! onto these types, never a second format on disk.

mod card;
pub mod catalog;
pub mod css;
mod files;
pub mod fonts;
pub mod helpers;
mod load;
mod model;
pub mod open_design;
pub mod render;
mod save;
mod scaffold;
mod validate;

pub use model::{
    Asset, CardMeta, Component, DesignFile, DesignSystem, LastChange, Meta, Theme, Token,
    TokenValue, Tokens, TypeGroup, TypeStyle,
};

use load::read_json;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn repo() -> DesignSystem {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("design");
        DesignSystem::load(&dir).expect("the repo's design/ loads")
    }

    #[test]
    fn discover_reports_a_broken_system_instead_of_skipping_it() {
        let scratch = tempfile::tempdir().unwrap();
        let dir = scratch.path();
        let design = dir.join("design");
        std::fs::create_dir_all(&design).unwrap();
        std::fs::write(
            design.join("design-system.json"),
            r#"{ "v": 1, "title": "T", "namespace": "t" }"#,
        )
        .unwrap();
        std::fs::write(design.join("tokens.json"), "{ not json").unwrap();

        let err = DesignSystem::discover(&dir.join("issues")).unwrap_err();

        assert!(err.contains("tokens.json"), "{err}");
    }

    #[test]
    fn loads_the_repo_design_system() {
        let system = repo();
        assert_eq!(system.meta.title, "iboard");
        assert_eq!(system.meta.namespace, "iboard");
        assert!(system.token("bg").is_some());
        assert!(system.token("accent").is_some());
        assert!(system.components.iter().any(|c| c.name == "Button"));
        assert_eq!(system.tokens.themes.len(), 2);
    }

    #[test]
    fn color_tokens_are_themed_both_ways() {
        let system = repo();
        let bg = system.token("bg").expect("bg exists");
        assert_eq!(bg.value.theme("light"), Some("#f4f5f7"));
        assert_eq!(bg.value.theme("dark"), Some("#15171a"));
    }

    #[test]
    fn references_and_themes_resolve() {
        let system = repo();
        assert_eq!(system.errors(), Vec::<String>::new());
    }

    #[test]
    fn discover_walks_up_from_the_manifest_dir() {
        let system = DesignSystem::discover(Path::new(env!("CARGO_MANIFEST_DIR")))
            .expect("discover finds design/");
        assert_eq!(system.meta.title, "iboard");
    }

    #[test]
    fn the_browser_can_read_files_assets_the_brief_and_groups() {
        let system = repo();
        let files = system.files();
        assert!(files.iter().any(|file| file.rel == "tokens.json"));
        assert!(files.iter().any(|file| file.rel == "design-system.json"));
        assert!(
            files
                .iter()
                .any(|file| file.rel == "components/Button/preview.html")
        );
        assert!(files.iter().all(|file| !file.rel.contains('\\')));
        assert!(system.brief().is_some(), "design/README.md is the brief");

        // The project's bundled fonts are exposed as assets.
        assert!(
            system
                .assets()
                .iter()
                .any(|asset| asset.name.starts_with("IBMPlexSans"))
        );
    }

    #[test]
    fn components_group_by_their_readme_group() {
        let system = repo();
        // Button ships no `Group:` line, so it lands in the default group.
        let groups = system.component_groups();
        assert!(
            groups
                .iter()
                .any(|(group, list)| list.iter().any(|c| c.name == "Button") && !group.is_empty())
        );
    }

    #[test]
    fn missing_preview_and_readme_are_warnings_not_errors() {
        let system = repo();
        // BoardScreen and Cover ship a preview without a README today.
        assert!(system.warnings.iter().any(|w| w.contains("BoardScreen")));
        assert!(system.errors().is_empty());
    }

    fn temp_dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn save_round_trips_the_repo_system() {
        let source = repo();
        let scratch = temp_dir();
        let base = scratch.path();
        let target = base.join("design");

        source.save(&target).expect("save");
        let reloaded = DesignSystem::load(&target).expect("reload");

        assert_eq!(reloaded.meta, source.meta);
        assert_eq!(reloaded.tokens, source.tokens);
        assert_eq!(reloaded.components.len(), source.components.len());
        let before: Vec<&str> = source.components.iter().map(|c| c.name.as_str()).collect();
        let after: Vec<&str> = reloaded
            .components
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        assert_eq!(after, before);
        assert_eq!(reloaded.errors(), Vec::<String>::new());
    }

    #[test]
    fn add_component_creates_and_reloads() {
        let scratch = temp_dir();
        let base = scratch.path();
        let target = base.join("design");
        repo().save(&target).expect("seed");
        let mut system = DesignSystem::load(&target).expect("load copy");

        let dir = system
            .add_component("Toast", "Controls", Some("Transient confirmation."))
            .expect("add");
        assert!(dir.join("README.md").is_file());
        assert!(dir.join("preview.html").is_file());

        let reloaded = DesignSystem::load(&target).expect("reload");
        assert!(reloaded.components.iter().any(|c| c.name == "Toast"));
        assert_eq!(reloaded.components.len(), system.components.len());
        assert!(reloaded.errors().is_empty());
        assert_eq!(
            reloaded
                .meta
                .last_change
                .as_ref()
                .and_then(|c| c.via.as_deref()),
            Some("idesign")
        );
    }

    #[test]
    fn add_component_refuses_duplicates_and_bad_names() {
        let scratch = temp_dir();
        let base = scratch.path();
        let target = base.join("design");
        repo().save(&target).expect("seed");
        let mut system = DesignSystem::load(&target).expect("load copy");

        assert!(system.add_component("Button", "Controls", None).is_err());
        assert!(system.add_component("", "Controls", None).is_err());
        assert!(system.add_component("../Evil", "Controls", None).is_err());
        assert!(system.add_component("Lead/ing", "Controls", None).is_err());
    }
}
