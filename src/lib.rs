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

pub mod agent;
pub mod assets;
pub mod build_log;
pub mod commits;
pub mod config;
pub mod design;
#[cfg(feature = "gui")]
pub mod design_ui;
pub mod diff;
pub mod discovery;
pub mod files;
pub mod filter;
pub mod format;
pub mod general;
pub mod history;
pub mod issue;
pub mod job;
pub mod labels;
pub mod markdown;
#[cfg(all(feature = "gui", target_os = "macos"))]
pub mod menu;
pub mod order;
#[cfg(all(feature = "gui", target_os = "macos"))]
pub mod pasteboard;
pub mod process;
pub mod prompts;
pub mod reviews;
pub mod settings;
pub mod sheet;
#[cfg(feature = "gui")]
pub mod theme;
pub mod titles;
pub mod trash;
#[cfg(feature = "gui")]
pub mod widgets;
