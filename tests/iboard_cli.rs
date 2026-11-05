//! End-to-end checks on the `iboard` binary, in the modes that never open a
//! window: `--list`, `init … --list`, help and usage errors, and handing the
//! board to an iboard already running.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::{Command, Output};

/// iboard on `args`, with no user configuration: the user's own workspace
/// would change what a folder without issues opens.
fn iboard(args: &[&str]) -> Output {
    let config = tempfile::tempdir().unwrap();
    Command::new(env!("CARGO_BIN_EXE_iboard"))
        .args(args)
        .env("XDG_CONFIG_HOME", config.path())
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

/// How long a later iboard gets to leave, and what it asked to come through.
#[cfg(unix)]
const LIMIT: std::time::Duration = std::time::Duration::from_secs(20);

/// An iboard already running, as a later one finds it through `cache`: it
/// answers `count` requests, and hands back what each asked once answered.
#[cfg(unix)]
fn running_iboard(cache: &Path, count: usize) -> std::sync::mpsc::Receiver<serde_json::Value> {
    use std::io::{BufRead, Write};
    std::fs::create_dir_all(cache.join("iboard")).unwrap();
    let socket = cache.join("iboard/iboard.sock");
    let listener = std::os::unix::net::UnixListener::bind(socket).unwrap();
    let (sender, asked) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for _ in 0..count {
            let (stream, _) = listener.accept().unwrap();
            let mut line = String::new();
            std::io::BufReader::new(&stream)
                .read_line(&mut line)
                .unwrap();
            (&stream).write_all(b"ok\n").unwrap();
            let _ = sender.send(serde_json::from_str(&line).unwrap());
        }
    });
    asked
}

/// iboard on `args`, stopped if it opens a window rather than leaving.
#[cfg(unix)]
fn later_iboard(args: &[&str], cache: &Path, config: &Path) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_iboard"))
        .args(args)
        .env("XDG_CACHE_HOME", cache)
        .env("XDG_CONFIG_HOME", config)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let start = std::time::Instant::now();
    while matches!(child.try_wait(), Ok(None)) && start.elapsed() < LIMIT {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let _ = child.kill();
    child.wait_with_output().unwrap()
}

/// With an iboard running, a later one hands it the project (its root, from
/// inside it), the first group with `--all`, or with no path only asks it to
/// come forward, and opens no window of its own.
#[cfg(unix)]
#[test]
fn a_later_iboard_hands_its_board_to_the_running_one() {
    let (dir, cache, config) = (
        project(),
        tempfile::tempdir().unwrap(),
        tempfile::tempdir().unwrap(),
    );
    std::fs::create_dir_all(config.path().join("iboard")).unwrap();
    std::fs::write(
        config.path().join("iboard/iboard.toml"),
        format!(
            "[[workspace.group]]\nname = \"Dev\"\nrepos = [{:?}]\n",
            dir.path().display().to_string()
        ),
    )
    .unwrap();
    let running = running_iboard(cache.path(), 3);
    let inside = dir.path().join("issues/todo");
    for args in [&[inside.to_str().unwrap()][..], &["--all"], &[]] {
        let output = later_iboard(args, cache.path(), config.path());
        assert_eq!(
            output.status.code(),
            Some(0),
            "{args:?}: {}",
            text(&output.stderr)
        );
    }
    let project = std::fs::canonicalize(dir.path()).unwrap();
    let asked: Vec<serde_json::Value> = (0..3)
        .map_while(|_| running.recv_timeout(LIMIT).ok())
        .collect();
    assert_eq!(
        asked,
        [
            serde_json::json!({ "project": project }),
            serde_json::json!({ "group": "Dev" }),
            serde_json::json!("focus"),
        ]
    );
}

#[test]
fn help_and_version_exit_zero_and_unknown_options_two() {
    assert_eq!(iboard(&["--help"]).status.code(), Some(0));
    let version = iboard(&["--version"]);
    assert_eq!(version.status.code(), Some(0));
    assert!(text(&version.stdout).starts_with("iboard "));
    assert_eq!(iboard(&["--print"]).status.code(), Some(2));
}

/// `--all --list` lists each group of the user configuration as a board of
/// its own projects merged, names each issue's project, and skips a folder
/// without `issues/`.
#[test]
fn all_lists_each_group_with_its_projects_merged() {
    let config = tempfile::tempdir().unwrap();
    let (app, site, bare) = (project(), project(), tempfile::tempdir().unwrap());
    std::fs::write(site.path().join("issues/ready/001-ship.md"), "# Ship\n").unwrap();
    std::fs::create_dir_all(config.path().join("iboard")).unwrap();
    let repos = |dirs: &[&Path]| {
        let quoted: Vec<String> = dirs
            .iter()
            .map(|d| format!("{:?}", d.display().to_string()))
            .collect();
        quoted.join(", ")
    };
    std::fs::write(
        config.path().join("iboard/iboard.toml"),
        format!(
            "[[workspace.group]]\nname = \"Dev\"\nrepos = [{}]\n\n\
             [[workspace.group]]\nname = \"Client\"\nrepos = [{}]\n",
            repos(&[app.path()]),
            repos(&[site.path(), bare.path()]),
        ),
    )
    .unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_iboard"))
            .args(args)
            .env("XDG_CONFIG_HOME", config.path())
            .output()
            .unwrap()
    };
    let output = run(&["--all", "--list"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let stdout = text(&output.stdout);
    let outside = run(&["--list", bare.path().to_str().unwrap()]);
    assert_eq!(
        text(&outside.stdout),
        stdout,
        "outside any project, the workspace opens"
    );
    let (app_name, site_name) = (
        app.path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned(),
        site.path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned(),
    );
    assert!(stdout.starts_with("Dev  (group, 1 repo)"), "{stdout}");
    let client = stdout.find("\nClient  (group, 1 repo)").expect(&stdout);
    let (dev, client) = stdout.split_at(client);
    assert!(dev.contains("\nReady (1)\n"), "{dev}");
    assert!(client.contains("\nReady (2)\n"), "{client}");
    assert!(!dev.contains("Ship"), "site is not in Dev: {dev}");
    assert!(
        stdout.contains(&format!("  • [{site_name}] Ship")),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("  • [{app_name}] Fix login")),
        "{stdout}"
    );
    assert!(text(&output.stderr).contains("skipped"));

    std::fs::remove_file(config.path().join("iboard/iboard.toml")).unwrap();
    let output = run(&["--all", "--list"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(text(&output.stderr).contains("no [[workspace.group]]"));
}
