//! The newer release GitHub publishes, and the bundle swap that installs it.
//!
//! CI attaches `iboard.app.zip` to every tag. The app asks the releases API for
//! the newest one, offers it in the status bar, downloads and unpacks it into
//! the cache, then hands the swap to a small shell script that waits for this
//! process to quit, replaces `iboard.app` and reopens it on the same project.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use serde_json::Value;

use crate::platform::job::Cancel;
use crate::platform::{download, files, process};

/// The repository the releases come from.
pub const REPO: &str = "cdesiles/iboard";
/// The asset CI attaches to a tag.
pub const ASSET: &str = "iboard.app.zip";
/// The version this copy runs.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// How long unpacking and moving the bundle may take.
const UNPACK: Duration = Duration::from_secs(120);

/// A release newer than the running one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Update {
    /// The version without its leading `v`: `0.5.0`.
    pub version: String,
    /// The URL of its `iboard.app.zip` asset.
    pub asset: String,
}

/// The newest release, when it is newer than `current`. A draft, a prerelease,
/// an older tag or one without the app asset offers nothing.
pub fn latest(current: &str, cancel: &Cancel) -> Result<Option<Update>, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let reply = download::text(&url, cancel)?;
    latest_from(&reply, current)
}

fn latest_from(reply: &str, current: &str) -> Result<Option<Update>, String> {
    let release: Value =
        serde_json::from_str(reply).map_err(|err| format!("GitHub sent no JSON: {err}"))?;
    if release["draft"].as_bool().unwrap_or(false)
        || release["prerelease"].as_bool().unwrap_or(false)
    {
        return Ok(None);
    }
    let tag = release["tag_name"]
        .as_str()
        .ok_or_else(|| "The release has no tag.".to_string())?;
    if !is_newer(tag, current) {
        return Ok(None);
    }
    let asset = release["assets"]
        .as_array()
        .and_then(|assets| {
            assets.iter().find_map(|asset| {
                let name = asset["name"].as_str()?;
                if name != ASSET {
                    return None;
                }
                asset["browser_download_url"].as_str()
            })
        })
        .ok_or_else(|| format!("{tag} has no {ASSET} asset."))?;
    Ok(Some(Update {
        version: version_of(tag).unwrap_or_else(|| tag.trim_start_matches('v').to_string()),
        asset: asset.to_string(),
    }))
}

/// Whether `tag` names a version newer than `current`.
pub fn is_newer(tag: &str, current: &str) -> bool {
    match (parsed(tag), parsed(current)) {
        (Some(newer), Some(current)) => newer > current,
        _ => false,
    }
}

/// `v0.5.0` as `0.5.0`, the version the dialog and the status bar show.
fn version_of(tag: &str) -> Option<String> {
    let start = tag.find(|c: char| c.is_ascii_digit())?;
    let version = &tag[start..];
    Some(version.split(['-', '+']).next()?.to_string())
}

/// A version's numbers, then its prerelease identifiers.
#[derive(Debug, PartialEq, Eq)]
struct Version {
    numbers: Vec<u64>,
    pre: Vec<Part>,
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let numbers = self.numbers.cmp(&other.numbers);
        if numbers != std::cmp::Ordering::Equal {
            return numbers;
        }
        match (self.pre.is_empty(), other.pre.is_empty()) {
            (true, true) => std::cmp::Ordering::Equal,
            (true, false) => std::cmp::Ordering::Greater,
            (false, true) => std::cmp::Ordering::Less,
            (false, false) => self.pre.cmp(&other.pre),
        }
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// The parts a version compares on: its numbers, then its prerelease.
fn parsed(text: &str) -> Option<Version> {
    let start = text.find(|c: char| c.is_ascii_digit())?;
    let version = &text[start..];
    let (core, pre) = match version.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (version, None),
    };
    let core = core.split('+').next()?;
    let numbers: Option<Vec<u64>> = core.split('.').map(|part| part.parse().ok()).collect();
    let mut numbers = numbers?;
    numbers.resize(numbers.len().max(3), 0);
    let pre = pre
        .map(|pre| pre.split('.').map(Part::of).collect())
        .unwrap_or_default();
    Some(Version { numbers, pre })
}

/// One prerelease identifier, which numbers before words.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Part {
    Number(u64),
    Word(String),
}

impl Part {
    fn of(text: &str) -> Self {
        match text.parse() {
            Ok(number) => Self::Number(number),
            Err(_) => Self::Word(text.to_string()),
        }
    }
}

/// The folder of the running `iboard.app`, found from its executable.
pub fn bundle_of(executable: &Path) -> Option<PathBuf> {
    let bundle = executable
        .ancestors()
        .find(|path| path.extension().is_some_and(|extension| extension == "app"))?;
    bundle
        .join("Contents/MacOS")
        .is_dir()
        .then(|| bundle.into())
}

/// Downloads `update` and unpacks its app into the cache; the app bundle, ready
/// to take the running one's place.
pub fn stage(update: &Update, cancel: &Cancel) -> Result<PathBuf, String> {
    let root = files::cache_dir()
        .ok_or_else(|| "Couldn't find the cache folder.".to_string())?
        .join("updates")
        .join(&update.version);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root)
        .map_err(|err| format!("Couldn't make {}: {err}", root.display()))?;
    let zip = root.join(ASSET);
    download::file(&update.asset, &zip, cancel)?;
    unpack(&zip, &root.join("unpacked"), cancel)
}

fn unpack(zip: &Path, destination: &Path, cancel: &Cancel) -> Result<PathBuf, String> {
    std::fs::create_dir_all(destination)
        .map_err(|err| format!("Couldn't make {}: {err}", destination.display()))?;
    let mut command = Command::new("ditto");
    command.args(["-x", "-k"]).arg(zip).arg(destination);
    process::run(command, "the update", cancel, UNPACK)?.into_result("the update")?;
    let app = destination.join("iboard.app");
    if !app.join("Contents/MacOS/iboard").is_file() {
        return Err(format!(
            "The update has no iboard.app in {}.",
            zip.display()
        ));
    }
    Ok(app)
}

/// Replaces `bundle` with `staged` once this process (pid) is gone, then opens
/// it with `args`: the project it was showing, or `--all` for a group board.
pub fn relaunch(bundle: &Path, staged: &Path, args: &[String]) -> Result<(), String> {
    let dir = files::cache_dir()
        .ok_or_else(|| "Couldn't find the cache folder.".to_string())?
        .join("updates");
    std::fs::create_dir_all(&dir)
        .map_err(|err| format!("Couldn't make {}: {err}", dir.display()))?;
    let script = dir.join("relaunch.sh");
    files::atomic_write(&script, RELAUNCH)
        .map_err(|err| format!("Couldn't write {}: {err}", script.display()))?;
    let mut command = Command::new("/bin/sh");
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .arg(&script)
        .arg(std::process::id().to_string())
        .arg(bundle)
        .arg(staged)
        .args(args);
    process::spawn(&mut command)
        .map(|_| ())
        .map_err(|err| format!("Couldn't restart iboard: {err}"))
}

/// Waits for this process to exit so the running binary is not swapped under
/// it, keeps the old bundle until the new one is in place, and reopens it.
const RELAUNCH: &str = r#"#!/bin/sh
set -u
pid=$1
app=$2
staged=$3
shift 3
tries=0
while kill -0 "$pid" 2>/dev/null; do
    tries=$((tries + 1))
    [ "$tries" -gt 600 ] && break
    sleep 0.1
done
backup="$app.old.$$"
rm -rf "$backup"
if ! mv "$app" "$backup"; then
    exit 1
fi
if ! mv "$staged" "$app"; then
    mv "$backup" "$app"
    exit 1
fi
rm -rf "$backup"
if [ "$#" -gt 0 ]; then
    /usr/bin/open -n "$app" --args "$@"
else
    /usr/bin/open -n "$app"
fi
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_newer_version_is_offered() {
        assert!(is_newer("v0.4.0", "0.3.0"));
        assert!(!is_newer("v0.4.0", "0.4"));
        assert!(!is_newer("v0.4.0", "0.4.0"));
        assert!(!is_newer("v0.3.9", "0.4.0"));
        assert!(!is_newer("nightly", "0.4.0"));
        assert!(is_newer("v0.4.0", "0.4.0-rc.1"));
        assert!(!is_newer("v0.4.0-rc.1", "0.4.0"));
        assert!(is_newer("v0.4.0-rc.2", "0.4.0-rc.1"));
        assert_eq!(version_of("v0.4.0"), Some("0.4.0".to_string()));
        assert_eq!(version_of("rel-1.2"), Some("1.2".to_string()));
    }

    fn release(tag: &str, draft: bool, prerelease: bool, asset: Option<&str>) -> String {
        let assets = match asset {
            Some(name) => format!(
                r#"[{{"name":"{name}","browser_download_url":"https://example.com/{name}"}}]"#
            ),
            None => "[]".to_string(),
        };
        format!(
            r#"{{"tag_name":"{tag}","draft":{draft},"prerelease":{prerelease},"assets":{assets}}}"#
        )
    }

    #[test]
    fn a_newer_release_names_its_app_asset() {
        let reply = release("v0.5.0", false, false, Some(ASSET));
        assert_eq!(
            latest_from(&reply, "0.4.0").unwrap(),
            Some(Update {
                version: "0.5.0".to_string(),
                asset: format!("https://example.com/{ASSET}"),
            })
        );
    }

    #[test]
    fn no_newer_release_or_no_asset_offers_nothing() {
        assert_eq!(
            latest_from(&release("v0.4.0", false, false, Some(ASSET)), "0.4.0").unwrap(),
            None
        );
        assert_eq!(
            latest_from(&release("v0.5.0", true, false, Some(ASSET)), "0.4.0").unwrap(),
            None
        );
        assert_eq!(
            latest_from(&release("v0.5.0", false, true, Some(ASSET)), "0.4.0").unwrap(),
            None
        );
        assert!(latest_from(&release("v0.5.0", false, false, None), "0.4.0").is_err());
        assert!(latest_from("not json", "0.4.0").is_err());
    }

    #[test]
    fn the_bundle_is_the_running_executable_s_app() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("iboard.app");
        let macos = app.join("Contents/MacOS");
        std::fs::create_dir_all(&macos).unwrap();
        let exe = macos.join("iboard");
        std::fs::write(&exe, "").unwrap();
        assert_eq!(bundle_of(&exe), Some(app));
        assert_eq!(bundle_of(&dir.path().join("target/debug/iboard")), None);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn a_release_zip_unpacks_to_its_app() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("iboard.app");
        let macos = app.join("Contents/MacOS");
        std::fs::create_dir_all(&macos).unwrap();
        std::fs::write(macos.join("iboard"), "binary").unwrap();
        let zip = dir.path().join(ASSET);
        let packed = Command::new("ditto")
            .args(["-c", "-k", "--keepParent"])
            .arg(&app)
            .arg(&zip)
            .status()
            .unwrap();
        assert!(packed.success());

        let unpacked = unpack(&zip, &dir.path().join("out"), &Cancel::default()).unwrap();
        assert_eq!(
            std::fs::read_to_string(unpacked.join("Contents/MacOS/iboard")).unwrap(),
            "binary"
        );
        assert!(unpacked.ends_with("iboard.app"));
    }

    #[test]
    fn the_relaunch_waits_for_the_app_and_reopens_it() {
        assert!(RELAUNCH.contains("kill -0 \"$pid\""));
        assert!(RELAUNCH.contains("mv \"$staged\" \"$app\""));
        assert!(RELAUNCH.contains("/usr/bin/open -n \"$app\" --args \"$@\""));
    }
}
