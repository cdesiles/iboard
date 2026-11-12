//! HTTPS reads through the system `curl`: a JSON reply, or a file.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use super::job::Cancel;
use super::process;

/// How long a read or a download may take.
pub const TIMEOUT: Duration = Duration::from_secs(120);

/// `url`'s body as text.
pub fn text(url: &str, cancel: &Cancel) -> Result<String, String> {
    let command = curl(url);
    process::run(command, "the download", cancel, TIMEOUT)?.into_result("the download")
}

/// `url`'s body written to `path`.
pub fn file(url: &str, path: &Path, cancel: &Cancel) -> Result<(), String> {
    let mut command = curl(url);
    command.arg("--output").arg(path);
    process::run(command, "the download", cancel, TIMEOUT)?.into_result("the download")?;
    Ok(())
}

fn curl(url: &str) -> Command {
    let mut command = Command::new("curl");
    command.args([
        "--silent",
        "--show-error",
        "--fail",
        "--location",
        "--max-time",
        "120",
    ]);
    command.arg(url);
    command
}
