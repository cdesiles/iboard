//! Shared library behind the `iboard` board and the `idesign` toolchain.
//!
//! Both binaries sit on this crate so they can never disagree about an on-disk
//! format. Everything that is not egui drawing lives here: the board
//! discovery, the agent runner and its prompt, and the design system.
//!
//! `iboard` (the window) keeps the egui views in its own binary; `idesign`
//! (the CLI) creates, browses and validates a project's design system.

pub mod agent;
pub mod config;
pub mod design;
pub mod design_ui;
pub mod discovery;
pub mod filter;
pub mod general;
pub mod history;
pub mod issue;
pub mod labels;
#[cfg(target_os = "macos")]
pub mod menu;
pub mod prompts;
pub mod sheet;
pub mod theme;
pub mod titles;
pub mod widgets;
