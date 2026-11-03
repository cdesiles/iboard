//! Continue in terminal: an ask's Claude Code session goes on interactively,
//! in Terminal, from the issue's project, through direnv when its runs are.

use std::path::{Path, PathBuf};

/// `text` as one single-quoted shell word.
fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// The script Terminal runs: into `project`, then `claude --resume`.
pub(super) fn script(
    project: &Path,
    session: &str,
    claude: &Path,
    direnv: Option<&Path>,
) -> String {
    let claude = quoted(&claude.to_string_lossy());
    let run = match direnv {
        Some(direnv) => format!("{} exec . {claude}", quoted(&direnv.to_string_lossy())),
        None => claude,
    };
    format!(
        "#!/bin/sh\ncd {} || exit 1\nexec {run} --resume {}\n",
        quoted(&project.to_string_lossy()),
        quoted(session),
    )
}

/// Writes the script as an executable `.command` file in `dir`, which `open`
/// hands to Terminal.
pub(super) fn write(dir: &Path, script: &str, session: &str) -> Result<PathBuf, String> {
    use std::os::unix::fs::PermissionsExt;
    let name: String = session
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    let file = dir.join(format!("iboard-{name}.command"));
    let fail = |err: std::io::Error| format!("Couldn't write {}: {err}", file.display());
    std::fs::write(&file, script).map_err(fail)?;
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).map_err(fail)?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_script_resumes_the_session_in_the_project() {
        let claude = Path::new("/bin/claude");
        let plain = script(Path::new("/p/it's"), "s-42", claude, None);
        assert_eq!(
            plain,
            "#!/bin/sh\ncd '/p/it'\\''s' || exit 1\nexec '/bin/claude' --resume 's-42'\n"
        );
        let direnv = script(
            Path::new("/p"),
            "s-42",
            claude,
            Some(Path::new("/bin/direnv")),
        );
        assert!(direnv.ends_with("exec '/bin/direnv' exec . '/bin/claude' --resume 's-42'\n"));
    }

    #[test]
    fn the_script_is_written_executable() {
        use std::os::unix::fs::PermissionsExt;
        let scratch = tempfile::tempdir().unwrap();
        let file = write(scratch.path(), "#!/bin/sh\n", "s/42;rm").unwrap();
        assert_eq!(file, scratch.path().join("iboard-s42rm.command"));
        let mode = std::fs::metadata(&file).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o755);
    }
}
