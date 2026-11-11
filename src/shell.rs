//! The egui application: windows, screens, widgets, theme, menus and dialogs.
//!
//! The shell names every domain; no domain names it back. `tests/architecture.rs`
//! holds that line.

pub mod app;
pub mod assets;
pub mod card_facts;
pub mod dialog;
pub mod groups_ui;
pub mod instance;
pub mod label_picker;
#[cfg(target_os = "macos")]
pub mod menu;
pub mod project_menu;
pub mod recents;
pub mod theme;
pub mod widgets;
