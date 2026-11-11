//! Running git in a folder. Paths come out as they are rather than quoted,
//! and each git starts through [`process::spawn`], so no other child inherits
//! its pipes.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use crate::process;

/// `git -C dir`, printing non-ASCII paths as they are, not as octal escapes,
/// and deaf to the `GIT_DIR`, `GIT_WORK_TREE` and `GIT_INDEX_FILE` that a git
/// hook would pass down.
pub fn command(dir: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(dir)
        .args(["-c", "core.quotePath=false"])
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE");
    command
}

/// What `git args` prints in `dir`, or `git <subcommand>: <its error>`.
pub fn output(dir: &Path, args: &[&str]) -> Result<String, String> {
    let mut command = command(dir);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = process::spawn(&mut command)
        .and_then(Child::wait_with_output)
        .map_err(|err| format!("git: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "git {}: {}",
            subcommand(args),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The subcommand of `args`, past git's own options such as `-c name=value`.
fn subcommand<'a>(args: &[&'a str]) -> &'a str {
    let mut args = args.iter().copied();
    while let Some(arg) = args.next() {
        if arg == "-c" || arg == "-C" {
            args.next();
        } else if !arg.starts_with('-') {
            return arg;
        }
    }
    ""
}

/// One `git cat-file --batch`, reading objects by name.
pub struct Batch {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}

impl Batch {
    pub fn open(dir: &Path) -> Result<Self, String> {
        let mut command = command(dir);
        command
            .args(["cat-file", "--batch"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = process::spawn(&mut command).map_err(|err| format!("git: {err}"))?;
        let (Some(input), Some(output)) = (child.stdin.take(), child.stdout.take()) else {
            return Err("git cat-file: no pipe".to_string());
        };
        Ok(Self {
            child,
            input,
            output: BufReader::new(output),
        })
    }

    /// The object's bytes, or `None` when it does not exist.
    pub fn read(&mut self, object: &str) -> Result<Option<Vec<u8>>, String> {
        let broken = |err: std::io::Error| format!("git cat-file: {err}");
        writeln!(self.input, "{object}").map_err(broken)?;
        self.input.flush().map_err(broken)?;
        let mut header = String::new();
        self.output.read_line(&mut header).map_err(broken)?;
        let header = header.trim_end();
        if header.ends_with(" missing") || header.ends_with(" ambiguous") {
            return Ok(None);
        }
        let size: usize = header
            .rsplit(' ')
            .next()
            .and_then(|size| size.parse().ok())
            .ok_or_else(|| format!("git cat-file: unexpected `{header}`"))?;
        let mut bytes = vec![0; size + 1];
        self.output.read_exact(&mut bytes).map_err(broken)?;
        bytes.pop();
        Ok(Some(bytes))
    }
}

impl Drop for Batch {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scratch_git::git;

    fn repo(subject: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-q"]);
        git(
            dir.path(),
            &["commit", "-q", "--allow-empty", "-m", subject],
        );
        dir
    }

    #[test]
    fn an_inherited_git_dir_is_ignored() {
        const NAME: &str = "platform::git::tests::an_inherited_git_dir_is_ignored";
        if std::env::var_os("IBOARD_DECOY_GIT_DIR").is_some() {
            let dir = repo("real");
            let subject = output(dir.path(), &["log", "-1", "--format=%s"]).unwrap();
            assert_eq!(subject.trim(), "real");
            return;
        }
        let decoy = repo("decoy");
        let child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture"])
            .env("IBOARD_DECOY_GIT_DIR", "1")
            .env("GIT_DIR", decoy.path().join(".git"))
            .env("GIT_WORK_TREE", decoy.path())
            .env("GIT_INDEX_FILE", decoy.path().join(".git/index"))
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&child.stdout);
        assert!(child.status.success(), "{stdout}");
        assert!(stdout.contains("1 passed"), "{stdout}");
    }

    #[test]
    fn a_failure_names_its_subcommand() {
        let dir = tempfile::tempdir().unwrap();
        let args = ["-c", "log.showSignature=false", "log", "--no-such-option"];
        let error = output(dir.path(), &args).unwrap_err();
        assert!(error.starts_with("git log: "), "{error}");
    }
}
