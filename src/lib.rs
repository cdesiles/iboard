//! Shared library behind the `iboard` board and the `idesign` toolchain.
//!
//! Both binaries sit on this crate so they can never disagree about an on-disk
//! format: the board discovery, the agent runner and its prompts, and the
//! design system. The egui pieces both windows draw also live here: the theme,
//! the widgets, the Design panel and the macOS menu.
//!
//! `iboard` keeps its board, reader, editor and Settings views in its own
//! binary; `idesign` creates, browses, validates and renders a project's
//! design system, in a terminal or in its own window.

pub mod agent;
pub mod commits;
pub mod config;
pub mod design;
pub mod design_ui;
pub mod discovery;
pub mod files;
pub mod filter;
pub mod general;
pub mod history;
pub mod issue;
pub mod labels;
#[cfg(target_os = "macos")]
pub mod menu;
pub mod prompts;
pub mod settings;
pub mod sheet;
pub mod theme;
pub mod titles;
pub mod widgets;
