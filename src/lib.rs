//! Shared library behind the `iboard` board and the `idesign` toolchain.
//!
//! Both binaries sit on this crate so they can never disagree about an on-disk
//! format: the board discovery, the agent runner and its prompts, and the
//! design system. The egui pieces both windows draw also live here, behind the
//! default `gui` feature: the theme, the widgets, the Design panel and the
//! macOS menu. The rest builds without egui (`make check` holds that line).
//!
//! `iboard` keeps its board, reader, editor and Settings views in its own
//! binary; `idesign` creates, browses, validates and renders a project's
//! design system, in a terminal or in its own window.

pub mod agenda;
pub mod assets;
pub mod design;
#[cfg(feature = "gui")]
pub mod design_ui;
pub mod domains;
pub mod general;
pub mod kernel;
#[cfg(all(feature = "gui", target_os = "macos"))]
pub mod menu;
pub mod new_issue;
pub mod notifications;
#[cfg(all(feature = "gui", target_os = "macos"))]
pub mod pasteboard;
pub mod platform;
pub mod preferences;
pub mod release;
#[cfg(test)]
mod scratch_git;
pub mod settings;
pub mod stats;
#[cfg(feature = "gui")]
pub mod theme;
#[cfg(feature = "gui")]
pub mod widgets;

pub use domains::agent::{
    artifacts::{build_log, reviews},
    runtime as agent,
    workflows::{plan, prompts, refinement, titles},
};
pub use domains::authoring::{diff, prose, sheet};
pub use domains::board::{commits, config, discovery, filter, history, labels, priority, trash};
pub use domains::workspace;
pub use kernel::issue;
pub use platform::{files, format, git, job, markdown, process};
