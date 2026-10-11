//! End-to-end checks on the `iboard` binary, in the modes that never open a
//! window: `--list`, `init … --list`, help and usage errors.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::{Command, Output};

fn iboard(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_iboard"))
        .args(args)
        .output()
        .expect("iboard runs")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// A project with one issue in Todo and one in Ready.
fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let issues = dir.path().join("issues");
    std::fs::create_dir_all(issues.join("todo")).unwrap();
    std::fs::create_dir_all(issues.join("ready")).unwrap();
    std::fs::write(
        issues.join("todo/fix-login.md"),
        "# [bug] Fix login\n\nThe redirect loops.\n",
    )
    .unwrap();
    std::fs::write(issues.join("ready/001-export.md"), "# Export board\n").unwrap();
    dir
}

/// `--list` without its first line, which names the folder.
fn listing(path: &Path) -> String {
    let output = iboard(&["--list", path.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let stdout = text(&output.stdout);
    stdout.lines().skip(1).collect::<Vec<_>>().join("\n")
}

#[test]
fn list_prints_the_board_without_a_window() {
    let dir = project();
    assert_eq!(
        listing(dir.path()),
        "\nTodo (1)\n  • Fix login\n    The redirect loops.\n\nReady (1)\n  • Export board"
    );
}

#[test]
fn list_finds_the_issues_from_inside_them() {
    let dir = project();
    assert_eq!(
        listing(&dir.path().join("issues/todo")),
        listing(dir.path())
    );
}

#[test]
fn init_creates_the_four_columns() {
    let dir = tempfile::tempdir().unwrap();
    let fresh = dir.path().join("fresh");
    let output = iboard(&["init", fresh.to_str().unwrap(), "--list"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    for column in ["todo", "ready", "doing", "done"] {
        assert!(fresh.join("issues").join(column).is_dir(), "{column}");
    }
}

#[test]
fn a_folder_without_issues_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let output = iboard(&["--list", dir.path().to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(text(&output.stderr).starts_with("iboard: "));
}

#[test]
fn help_and_version_exit_zero_and_unknown_options_two() {
    assert_eq!(iboard(&["--help"]).status.code(), Some(0));
    let version = iboard(&["--version"]);
    assert_eq!(version.status.code(), Some(0));
    assert!(text(&version.stdout).starts_with("iboard "));
    assert_eq!(iboard(&["--print"]).status.code(), Some(2));
}
