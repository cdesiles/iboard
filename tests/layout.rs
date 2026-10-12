//! Keeps the source tree easy to find one's way in.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

/// The longest a source file may get. A ratchet: lower it whenever the longest
/// file shrinks, never raise it.
const MAX_LINES: usize = 2_275;

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// Every Rust and Swift file under `dir`, recursively.
fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            sources(&path, out);
        } else if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("rs" | "swift")
        ) {
            out.push(path);
        }
    }
}

fn source_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    for dir in ["src", "macos", "tests"] {
        sources(&root().join(dir), &mut files);
    }
    files.push(root().join("build.rs"));
    files.sort();
    files
}

fn relative(file: &Path) -> String {
    file.strip_prefix(root()).unwrap().display().to_string()
}

/// The paths in the first cell of AGENTS.md's file table: `` | `src/x.rs` | ``.
fn documented_files(agents: &str) -> Vec<String> {
    agents
        .lines()
        .filter_map(|line| line.strip_prefix("| `"))
        .filter_map(|rest| rest.split_once('`'))
        .map(|(path, _)| path.to_string())
        .collect()
}

#[test]
fn no_source_file_grows_past_the_budget() {
    let too_long: Vec<String> = source_files()
        .iter()
        .filter_map(|file| {
            let lines = std::fs::read_to_string(file).unwrap().lines().count();
            (lines > MAX_LINES).then(|| format!("{} ({lines} lines)", relative(file)))
        })
        .collect();
    assert!(
        too_long.is_empty(),
        "split these files, they are over {MAX_LINES} lines: {too_long:?}"
    );
}

#[test]
fn agents_md_lists_every_source_file() {
    // AGENTS.md may be git-ignored, so a fresh clone has none to check.
    let Ok(agents) = std::fs::read_to_string(root().join("AGENTS.md")) else {
        return;
    };
    let documented = documented_files(&agents);
    let missing: Vec<String> = source_files()
        .iter()
        .map(|file| relative(file))
        .filter(|file| !documented.contains(file))
        .collect();
    assert!(
        missing.is_empty(),
        "add these files to the table in AGENTS.md: {missing:?}"
    );
    let gone: Vec<&String> = documented
        .iter()
        .filter(|file| !root().join(file).exists())
        .collect();
    assert!(gone.is_empty(), "AGENTS.md lists missing files: {gone:?}");
}
