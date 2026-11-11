//! Shared library behind the `iboard` board and the `idesign` toolchain.
//!
//! Both binaries sit on this crate so they can never disagree about an on-disk
//! format: the board discovery, the agent runner and its prompts, and the
//! design system. The egui pieces both windows draw also live here, behind the
//! default `gui` feature: the shell, its theme, widgets and menus, and the
//! Design and Settings panels. The rest builds without egui (`make check` holds
//! that line).
//!
//! `iboard` is the board, reader, editor and Settings views; `idesign` creates,
//! browses, validates and renders a project's design system, in a terminal or
//! in its own window.

#[cfg(feature = "gui")]
pub mod design_ui;
pub mod domains;
pub mod general;
pub mod kernel;
pub mod new_issue;
pub mod notifications;
#[cfg(all(feature = "gui", target_os = "macos"))]
pub mod pasteboard;
pub mod platform;
pub mod preferences;
#[cfg(test)]
mod scratch_git;
pub mod settings;
#[cfg(feature = "gui")]
pub mod settings_ui;
#[cfg(feature = "gui")]
pub mod shell;

pub use domains::agent::{
    artifacts::{build_log, reviews},
    runtime as agent,
    workflows::{plan, prompts, refinement, titles},
};
pub use domains::authoring::{diff, prose, sheet};
pub use domains::board::{commits, config, discovery, filter, history, labels, priority, trash};
pub use domains::delivery as release;
pub use domains::design;
pub use domains::insights::{agenda, stats};
pub use domains::workspace;
pub use kernel::issue;
pub use platform::{files, format, git, job, markdown, process};
