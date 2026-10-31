//! Agent runs in a project with an `.envrc` go through `direnv exec`, so a
//! build sees the project's environment as a terminal in it would.

use std::path::Path;
use std::process::Command;

/// Whether a run in `project` goes through direnv: it is on, and the project
/// has an `.envrc`.
pub(super) fn applies(enabled: bool, project: &Path) -> bool {
    enabled && project.join(".envrc").is_file()
}

/// `command` as `direnv exec <project> <program> <args>`, keeping its
/// environment and working directory.
pub(super) fn wrap(command: &Command, direnv: &Path, project: &Path) -> Command {
    let mut wrapped = Command::new(direnv);
    wrapped
        .arg("exec")
        .arg(project)
        .arg(command.get_program())
        .args(command.get_args());
    for (key, value) in command.get_envs() {
        match value {
            Some(value) => wrapped.env(key, value),
            None => wrapped.env_remove(key),
        };
    }
    if let Some(dir) = command.get_current_dir() {
        wrapped.current_dir(dir);
    }
    wrapped
}
