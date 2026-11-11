//! Finding the tools: where `opencode` and `claude` are installed, their
//! version, and the models each one offers.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use super::Tool;
use crate::platform::files;
use crate::platform::job::Cancel;
use crate::platform::process;

/// Models Claude Code accepts for `--model`. It has no command that lists
/// them, so the aliases come first, then the current full IDs.
const CLAUDE_MODELS: [&str; 7] = [
    "opus",
    "sonnet",
    "haiku",
    "claude-fable-5-1",
    "claude-opus-5-5",
    "claude-sonnet-5-5",
    "claude-haiku-4-5-20251001",
];

/// How long `--version` and `opencode models` may take.
const PROBE_TIMEOUT: Duration = Duration::from_secs(20);

/// The folders searched for a tool: `PATH`, then where installers put them.
/// An app started from the Finder inherits a minimal `PATH`, so without the
/// extra folders a Homebrew or npm install would look missing.
fn search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    let mut extra = vec![
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ];
    if let Some(home) = files::home() {
        for dir in [
            ".local/bin",
            ".opencode/bin",
            ".claude/local",
            ".bun/bin",
            ".npm-global/bin",
        ] {
            extra.push(home.join(dir));
        }
    }
    for dir in extra {
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    dirs
}

/// The `PATH` agents run with: the search folders above.
pub(super) fn search_path() -> OsString {
    std::env::join_paths(search_dirs()).unwrap_or_default()
}

/// The full path of an executable named `name`, if one is installed.
pub(super) fn find_binary(name: &str) -> Option<PathBuf> {
    search_dirs()
        .into_iter()
        .map(|dir| dir.join(name))
        .find(|candidate| is_executable(candidate))
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

/// What the settings know about an installed tool.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Detected {
    /// Where the binary is; `None` when it is not on `PATH`.
    pub path: Option<PathBuf>,
    /// Its version, as `v1.2.3`, when it reports one.
    pub version: Option<String>,
}

/// Looks for `tool` and asks it for its version. Blocking: it runs the tool.
pub fn detect(tool: Tool) -> Detected {
    let Some(path) = tool.binary().and_then(find_binary) else {
        return Detected::default();
    };
    let mut command = Command::new(&path);
    command.arg("--version").env("PATH", search_path());
    let version = process::run(command, tool.key(), &Cancel::default(), PROBE_TIMEOUT)
        .and_then(|output| output.into_result(tool.key()))
        .ok()
        .and_then(|stdout| parse_version(&stdout));
    Detected {
        path: Some(path),
        version,
    }
}

/// The first version-looking word of `text` (`2.1.0 (Claude Code)`,
/// `opencode v2.0.20`), as `v2.1.0`.
pub(super) fn parse_version(text: &str) -> Option<String> {
    text.split_whitespace()
        .map(|word| word.trim_start_matches(['v', 'V']))
        .find(|word| word.starts_with(|c: char| c.is_ascii_digit()) && word.contains('.'))
        .map(|word| format!("v{word}"))
}

/// One model a tool can run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    pub id: String,
    /// Marked as a favorite in the tool's own settings.
    pub favorite: bool,
}

/// The models a tool offers, and what Default resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Models {
    /// The list, or why it could not be read.
    pub list: Result<Vec<Model>, String>,
    /// The tool's first favorite model, which Default runs; `None` when the
    /// tool has none and picks for itself.
    pub default: Option<String>,
}

/// Lists the models of `tool`. Blocking: for OpenCode it runs `opencode models`.
pub fn list_models(tool: Tool) -> Models {
    let favorites = favorites(tool);
    let ids: Result<Vec<String>, String> = match tool {
        Tool::OpenCode => opencode_models(),
        Tool::Claude => Ok(CLAUDE_MODELS.iter().map(|id| id.to_string()).collect()),
        Tool::Custom => Ok(Vec::new()),
    };
    let list = ids.map(|mut ids| {
        // A favorite the list forgot is still a model the tool runs.
        for favorite in favorites.iter().rev() {
            if !ids.contains(favorite) {
                ids.insert(0, favorite.clone());
            }
        }
        ids.into_iter()
            .map(|id| Model {
                favorite: favorites.contains(&id),
                id,
            })
            .collect()
    });
    Models {
        list,
        default: favorites.into_iter().next(),
    }
}

/// The model Default stands for: the tool's first favorite.
pub(super) fn default_model(tool: Tool) -> Option<String> {
    favorites(tool).into_iter().next()
}

fn favorites(tool: Tool) -> Vec<String> {
    match tool {
        Tool::OpenCode => opencode_state_file()
            .and_then(|file| std::fs::read_to_string(file).ok())
            .map(|text| parse_opencode_favorites(&text))
            .unwrap_or_default(),
        Tool::Claude => claude_settings_file()
            .and_then(|file| std::fs::read_to_string(file).ok())
            .and_then(|text| parse_claude_model(&text))
            .into_iter()
            .collect(),
        Tool::Custom => Vec::new(),
    }
}

fn opencode_models() -> Result<Vec<String>, String> {
    let binary =
        find_binary("opencode").ok_or_else(|| "opencode is not on your PATH".to_string())?;
    let mut command = Command::new(binary);
    command.arg("models").env("PATH", search_path());
    let what = "opencode models";
    let stdout =
        process::run(command, what, &Cancel::default(), PROBE_TIMEOUT)?.into_result(what)?;
    Ok(parse_model_list(&stdout))
}

/// One model ID per line; anything with a space is a banner, not an ID.
pub(super) fn parse_model_list(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.contains(char::is_whitespace))
        .map(str::to_string)
        .collect()
}

/// OpenCode keeps favorites in its state folder, in `model.json`.
fn opencode_state_file() -> Option<PathBuf> {
    files::dir_from_env("XDG_STATE_HOME", ".local/state")
        .map(|state| state.join("opencode/model.json"))
}

/// The `favorite` list of OpenCode's `model.json`, as `provider/model` IDs.
pub(super) fn parse_opencode_favorites(text: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return Vec::new();
    };
    value["favorite"]
        .as_array()
        .map(|favorites| {
            favorites
                .iter()
                .filter_map(|entry| {
                    let provider = entry["providerID"].as_str()?;
                    let model = entry["modelID"].as_str()?;
                    Some(format!("{provider}/{model}"))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Claude Code's user settings, where `model` is its chosen default.
fn claude_settings_file() -> Option<PathBuf> {
    files::dir_from_env("CLAUDE_CONFIG_DIR", ".claude").map(|dir| dir.join("settings.json"))
}

pub(super) fn parse_claude_model(text: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(text).ok()?;
    value["model"]
        .as_str()
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(str::to_string)
}
