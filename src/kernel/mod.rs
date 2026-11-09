//! The shared kernel: the vocabulary every domain agrees on.
//!
//! An issue and its labels, and the board of columns that holds them. The
//! kernel names no domain; it sits above `platform` and below every domain.

pub mod board;
pub mod issue;
pub mod labels;
