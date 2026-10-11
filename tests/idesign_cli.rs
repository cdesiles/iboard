//! End-to-end checks on the `idesign` CLI, run through the built binary.
//!
//! These guard the argument handling that is easy to get wrong — a bare
//! `idesign` must behave like `idesign show`, not panic.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::{Command, Output};

fn repo() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn idesign(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_idesign"))
        .args(args)
        .current_dir(repo())
        .output()
        .expect("idesign runs")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn no_arguments_shows_the_system() {
    let output = idesign(&[]);
    assert!(
        output.status.success(),
        "bare idesign failed: {}",
        stderr(&output)
    );
    assert!(
        stdout(&output).contains("components"),
        "no summary: {}",
        stdout(&output)
    );
}

#[test]
fn read_commands_exit_zero() {
    for command in ["show", "list", "tokens", "validate"] {
        let output = idesign(&[command]);
        assert_eq!(
            output.status.code(),
            Some(0),
            "`idesign {command}` failed: {}",
            stderr(&output)
        );
    }
}

#[test]
fn help_and_version_exit_zero() {
    for flag in ["--help", "--version"] {
        let output = idesign(&[flag]);
        assert_eq!(output.status.code(), Some(0), "`idesign {flag}` failed");
    }
}

#[test]
fn an_unknown_option_is_a_usage_error() {
    let output = idesign(&["--nope"]);
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn catalog_writes_a_live_page() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("catalog.html");

    let output = idesign(&["catalog", "--out", out.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let html = std::fs::read_to_string(&out).expect("catalog written");
    assert!(html.contains("--bg:"), "tokens inlined");
    assert!(html.contains("ds-search"), "search box present");
    assert!(html.contains("Button"), "a component is shown");
}

#[test]
fn add_component_scaffolds_a_component() {
    // A throwaway copy of the design system, so the repo is untouched.
    let scratch = tempfile::tempdir().unwrap();
    let dir = scratch.path();
    let design = dir.join("design");
    copy_dir(&repo().join("design"), &design);

    let output = Command::new(env!("CARGO_BIN_EXE_idesign"))
        .args([
            "add-component",
            "Toast",
            "--group",
            "Controls",
            "--intent",
            "Transient confirmation.",
        ])
        .current_dir(dir)
        .output()
        .expect("idesign runs");
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(design.join("components/Toast/README.md").is_file());
    assert!(design.join("components/Toast/preview.html").is_file());
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}
