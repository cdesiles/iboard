//! Git for the tests' scratch repositories, shielded from the machine it runs
//! on: no global or system config, no hooks, and no `GIT_DIR`, `GIT_WORK_TREE`
//! or `GIT_INDEX_FILE` inherited from a git hook. Both the library's tests and
//! the binary's include this file, each using part of it.

#![allow(dead_code)]

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

/// What `git args` prints in `dir`, trimmed. Fails the test, with what git
/// said, when git does.
pub fn git(dir: &Path, args: &[&str]) -> String {
    git_env(dir, &[], args)
}

/// [`git`] with `env` set, such as a `GIT_COMMITTER_DATE`.
pub fn git_env(dir: &Path, env: &[(&str, &str)], args: &[&str]) -> String {
    let output = command(dir)
        .envs(env.iter().copied())
        .args(args)
        .output()
        .unwrap();
    checked(args, &output)
}

/// [`git`] reading `input`, such as a `fast-import` stream.
pub fn git_stdin(dir: &Path, input: &str, args: &[&str]) -> String {
    let mut child = command(dir)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    checked(args, &child.wait_with_output().unwrap())
}

fn command(dir: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(["-c", "commit.gpgsign=false"])
        .args(["-c", "core.hooksPath=/dev/null"])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE");
    command
}

fn checked(args: &[&str], output: &Output) -> String {
    let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).trim().to_string();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        text(&[output.stderr.as_slice(), output.stdout.as_slice()].concat())
    );
    text(&output.stdout)
}
