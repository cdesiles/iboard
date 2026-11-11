//! The standard sections a ticket is written under.

/// The sections the reader asks for when `[reader] sections` is not set.
pub const DEFAULT_SECTIONS: [&str; 4] = [
    "Context",
    "Acceptance criteria",
    "Out of scope",
    "Open questions",
];
