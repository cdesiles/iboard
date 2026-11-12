//! A project's own step in its release: the command its `iboard.toml` names,
//! run before the release commit, and the files that command changes.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use serde::Deserialize;

use super::{Bump, check_file};
use crate::platform::git;
use crate::platform::job::Cancel;
use crate::platform::process;

/// The file at the project root that holds the project's release settings.
pub const PROJECT_FILE: &str = "iboard.toml";

/// How long the command may work.
const TIMEOUT: Duration = Duration::from_secs(600);

/// `[release] prepare` and `files`: what to run, and what it changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hook {
    pub command: String,
    pub files: Vec<String>,
}

impl Hook {
    /// What trusting a hook is tied to: change either part and it must be
    /// trusted again.
    pub fn fingerprint(&self) -> String {
        format!("{}\n{}", self.command, self.files.join("\n"))
    }

    /// Whether `trusted`, the fingerprints kept per project folder, holds
    /// this hook for `project`.
    pub fn trusted(&self, trusted: &BTreeMap<String, String>, project: &Path) -> bool {
        trusted.get(project.to_string_lossy().as_ref()) == Some(&self.fingerprint())
    }

    /// Records this hook as trusted for `project`, in place of any before it.
    pub fn trust(&self, trusted: &mut BTreeMap<String, String>, project: &Path) {
        trusted.insert(project.to_string_lossy().into_owned(), self.fingerprint());
    }
}

/// What the command is told about the release, as `IBOARD_*` variables.
pub struct Env<'a> {
    pub tag: &'a str,
    pub previous: Option<&'a str>,
    pub bump: Bump,
    pub note_file: Option<&'a str>,
}

#[derive(Default, Deserialize)]
struct Document {
    release: Option<Section>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Section {
    prepare: Option<String>,
    files: Vec<String>,
}

/// The hook the project's `iboard.toml` names, or `None` when it names none.
/// A file that cannot be read is an error, so a release never skips a hook
/// silently.
pub fn read(project: &Path) -> Result<Option<Hook>, String> {
    let text = match std::fs::read_to_string(project.join(PROJECT_FILE)) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(format!("Couldn't read {PROJECT_FILE}: {err}")),
    };
    let document: Document =
        toml::from_str(&text).map_err(|err| format!("{PROJECT_FILE}: {}", err.message()))?;
    let Some(section) = document.release else {
        return Ok(None);
    };
    let Some(command) = section.prepare else {
        return Ok(None);
    };
    let command = command.trim().to_string();
    if command.is_empty() {
        return Err(format!("{PROJECT_FILE}: [release] prepare is empty"));
    }
    let files: Vec<String> = section
        .files
        .iter()
        .map(|file| file.trim().to_string())
        .collect();
    for file in &files {
        check_file(file).map_err(|err| format!("{PROJECT_FILE}: [release] files: {err}"))?;
    }
    Ok(Some(Hook { command, files }))
}

/// `v1.2.3` as `1.2.3`: the tag from its first digit.
fn version(tag: &str) -> &str {
    tag.find(|c: char| c.is_ascii_digit())
        .map_or(tag, |start| &tag[start..])
}

/// Whether git sees a change in any of `files`.
fn changed(project: &Path, files: &[String]) -> Result<bool, String> {
    if files.is_empty() {
        return Ok(false);
    }
    let mut args = vec!["status", "--porcelain", "--"];
    args.extend(files.iter().map(String::as_str));
    Ok(!git::output(project, &args)?.trim().is_empty())
}

/// Whether the hook left a change in its files for the release to commit.
pub fn changes(project: &Path, hook: &Hook) -> bool {
    changed(project, &hook.files).unwrap_or(false)
}

/// Runs the hook in the project, through the user's login shell so the tools
/// its command needs are on the `PATH` even when the app was started from the
/// Finder. It refuses to start over uncommitted changes in its own files.
pub fn prepare(project: &Path, hook: &Hook, env: &Env) -> Result<(), String> {
    if changed(project, &hook.files)? {
        return Err(format!(
            "{} has uncommitted changes. Commit or discard them before releasing.",
            hook.files.join(", ")
        ));
    }
    let shell = std::env::var("SHELL")
        .ok()
        .filter(|shell| !shell.is_empty())
        .unwrap_or_else(|| "/bin/sh".to_string());
    let mut command = Command::new(shell);
    command
        .args(["-l", "-c", &hook.command])
        .current_dir(project)
        .env("IBOARD_TAG", env.tag)
        .env("IBOARD_VERSION", version(env.tag))
        .env("IBOARD_PREVIOUS_TAG", env.previous.unwrap_or_default())
        .env("IBOARD_BUMP", env.bump.name())
        .env("IBOARD_NOTE_FILE", env.note_file.unwrap_or_default());
    let output = process::run(command, "the prepare hook", &Cancel::default(), TIMEOUT)?;
    output
        .into_result("the prepare hook")
        .map(|_| ())
        .map_err(|err| format!("The prepare hook failed: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scratch_git::git;

    fn project(settings: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-q", "-b", "main"]);
        git(dir.path(), &["config", "user.name", "t"]);
        git(dir.path(), &["config", "user.email", "t@t"]);
        std::fs::write(dir.path().join("version.txt"), "0.1.0\n").unwrap();
        std::fs::write(dir.path().join(PROJECT_FILE), settings).unwrap();
        git(dir.path(), &["add", "version.txt"]);
        git(dir.path(), &["commit", "-q", "-m", "first"]);
        dir
    }

    fn env(tag: &str) -> Env<'_> {
        Env {
            tag,
            previous: Some("v0.1.0"),
            bump: Bump::Minor,
            note_file: Some("CHANGELOG.md"),
        }
    }

    #[test]
    fn the_hook_is_what_the_release_section_names() {
        let dir = project(
            "[release]\nprepare = ' make version '\nfiles = [\"Cargo.toml\", \"Cargo.lock\"]\n",
        );
        assert_eq!(
            read(dir.path()),
            Ok(Some(Hook {
                command: "make version".to_string(),
                files: vec!["Cargo.toml".to_string(), "Cargo.lock".to_string()],
            }))
        );
    }

    #[test]
    fn iboards_own_release_sets_the_version_in_its_manifest() {
        let hook = read(Path::new(env!("CARGO_MANIFEST_DIR")))
            .unwrap()
            .expect("the repository names its hook");
        assert_eq!(hook.command, "make version V=\"$IBOARD_VERSION\"");
        assert_eq!(hook.files, ["Cargo.toml", "Cargo.lock"]);
    }

    #[test]
    fn the_hook_shares_its_file_with_the_boards_own_settings() {
        let dir = project(
            "root = \"issues\"\ntitle = \"Moon\"\n\n[release]\nprepare = \"make version\"\n",
        );
        let hook = read(dir.path()).unwrap().unwrap();
        assert_eq!(hook.command, "make version");
        assert!(hook.files.is_empty());
    }

    #[test]
    fn a_project_without_a_hook_releases_as_before() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read(dir.path()), Ok(None), "no file");
        std::fs::write(dir.path().join(PROJECT_FILE), "[release]\ntags = \"v*\"\n").unwrap();
        assert_eq!(read(dir.path()), Ok(None), "no prepare");
    }

    #[test]
    fn a_file_the_release_cannot_use_is_an_error_not_a_skipped_hook() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(PROJECT_FILE);
        for text in [
            "[release\n",
            "[release]\nprepare = \"  \"\n",
            "[release]\nprepare = \"x\"\nfiles = [\"../up\"]\n",
            "[release]\nprepare = 3\n",
        ] {
            std::fs::write(&path, text).unwrap();
            let error = read(dir.path()).unwrap_err();
            assert!(error.starts_with(PROJECT_FILE), "{error}");
        }
    }

    #[test]
    fn trust_follows_the_command_and_its_files() {
        let hook = Hook {
            command: "make version".to_string(),
            files: vec!["Cargo.toml".to_string()],
        };
        let other_command = Hook {
            command: "make other".to_string(),
            ..hook.clone()
        };
        let other_files = Hook {
            files: vec!["Cargo.lock".to_string()],
            ..hook.clone()
        };
        assert_ne!(hook.fingerprint(), other_command.fingerprint());
        assert_ne!(hook.fingerprint(), other_files.fingerprint());
    }

    #[cfg(unix)]
    #[test]
    fn the_command_is_told_the_release() {
        let dir = project("");
        let hook = Hook {
            command: "printf '%s|%s|%s|%s|%s' \"$IBOARD_TAG\" \"$IBOARD_VERSION\" \
                      \"$IBOARD_PREVIOUS_TAG\" \"$IBOARD_BUMP\" \"$IBOARD_NOTE_FILE\" \
                      > version.txt"
                .to_string(),
            files: vec!["version.txt".to_string()],
        };
        prepare(dir.path(), &hook, &env("v0.2.0")).unwrap();
        let text = std::fs::read_to_string(dir.path().join("version.txt")).unwrap();
        assert_eq!(text, "v0.2.0|0.2.0|v0.1.0|minor|CHANGELOG.md");
        assert!(changes(dir.path(), &hook));
    }

    #[cfg(unix)]
    #[test]
    fn a_file_with_uncommitted_changes_stops_the_hook_before_it_runs() {
        let dir = project("");
        std::fs::write(dir.path().join("version.txt"), "mine\n").unwrap();
        let hook = Hook {
            command: "echo ran > ran.txt".to_string(),
            files: vec!["version.txt".to_string()],
        };
        let error = prepare(dir.path(), &hook, &env("v0.2.0")).unwrap_err();
        assert!(
            error.contains("version.txt has uncommitted changes"),
            "{error}"
        );
        assert!(!dir.path().join("ran.txt").exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_failing_command_says_why() {
        let dir = project("");
        let hook = Hook {
            command: "echo no such tool >&2; exit 3".to_string(),
            files: Vec::new(),
        };
        let error = prepare(dir.path(), &hook, &env("v0.2.0")).unwrap_err();
        assert_eq!(error, "The prepare hook failed: no such tool");
    }
}
