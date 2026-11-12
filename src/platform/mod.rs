//! The OS and the tools: files, git, processes, jobs, markdown, formatting.
//!
//! The lowest layer. It names nothing above it and draws nothing, so a domain,
//! the kernel and this layer all build without egui.

pub mod download;
pub mod files;
pub mod format;
pub mod git;
pub mod job;
pub mod markdown;
pub mod process;
