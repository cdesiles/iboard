//! The AI assist that challenges an issue before it is implemented.
//!
//! The agent is chosen once in the ReviewSettings dialog (`⌘,`) and stored in
//! `$XDG_CONFIG_HOME/iboard/iboard.toml` (`$HOME/.config/iboard/iboard.toml`
//! when `XDG_CONFIG_HOME` is unset):
//!
//! ```toml
//! [ai]
//! tool = "opencode"        # "opencode" | "claude" | "custom"
//! model = "default"        # or a model ID from the tool's list
//! command = ""             # custom only, e.g. "llm -m {model} < {prompt}"
//! ```
//!
//! `default` is the tool's first favorite model, so it follows the tool's own
//! settings. [`start`] runs the tool non-interactively against the issue text
//! and hands the answer back over a channel; the GUI polls it. The agent only
//! ever returns text: writing the file stays the user's decision.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, channel};

use serde::{Deserialize, Serialize};

const CONFIG_DIR: &str = "iboard";
const CONFIG_FILE: &str = "iboard.toml";

/// What a [`AgentConfig::model`] of "the tool's first favorite" is written as.
const DEFAULT_MODEL: &str = "default";

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

/// The agents the settings offer, in the order they list them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Tool {
    #[default]
    OpenCode,
    Claude,
    /// Any command the user writes, with `{prompt}`, `{model}` and `{issue}`.
    Custom,
}

impl Tool {
    pub const ALL: [Tool; 3] = [Tool::OpenCode, Tool::Claude, Tool::Custom];

    /// The name stored in the configuration and shown in "Runs with".
    pub fn key(self) -> &'static str {
        match self {
            Tool::OpenCode => "opencode",
            Tool::Claude => "claude",
            Tool::Custom => "custom",
        }
    }

    /// The name shown in the Tool select.
    pub fn name(self) -> &'static str {
        match self {
            Tool::OpenCode => "OpenCode",
            Tool::Claude => "Claude Code",
            Tool::Custom => "Custom command",
        }
    }

    /// The executable iboard looks for on `PATH`; `None` for a custom command.
    pub fn binary(self) -> Option<&'static str> {
        match self {
            Tool::OpenCode => Some("opencode"),
            Tool::Claude => Some("claude"),
            Tool::Custom => None,
        }
    }

    fn from_key(key: &str) -> Option<Tool> {
        Tool::ALL.into_iter().find(|tool| tool.key() == key)
    }
}

/// The `[ai]` section of the user's configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentConfig {
    pub tool: Tool,
    /// Model ID as the tool names it. Empty means Default: the tool's first
    /// favorite model.
    pub model: String,
    /// The custom command, with its placeholders. Unused by the other tools.
    pub command: String,
    /// Whether an agent was ever saved, so the Review button can run.
    pub configured: bool,
}

impl AgentConfig {
    /// Whether a tool is configured, so the Review button can be enabled.
    pub fn configured(&self) -> bool {
        self.configured
    }

    /// What the Review menu and the panel say will run: `opencode · default`.
    pub fn label(&self) -> String {
        let model = self.model.trim();
        let model = if model.is_empty() {
            DEFAULT_MODEL
        } else {
            model
        };
        format!("{} · {model}", self.tool.key())
    }

    /// Checks what can only be checked on Save: a custom command must say
    /// where the issue goes.
    pub fn validate(&self) -> Result<(), String> {
        if self.tool == Tool::Custom && !self.command.contains("{prompt}") {
            return Err("The command needs {prompt} to receive the issue.".to_string());
        }
        Ok(())
    }

    fn from_raw(raw: RawAgent) -> Self {
        let model = raw.model.trim();
        let model = if model.eq_ignore_ascii_case(DEFAULT_MODEL) {
            String::new()
        } else {
            model.to_string()
        };
        let tool = raw.tool.trim();
        if tool.is_empty() {
            return Self::default();
        }
        if let Some(known) = Tool::from_key(tool) {
            return Self {
                tool: known,
                model,
                command: raw.command,
                configured: true,
            };
        }
        // Before the settings dialog, `tool` held a program name or path.
        let stem = Path::new(tool)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(tool);
        if let Some(known) = Tool::from_key(stem).filter(|known| *known != Tool::Custom) {
            return Self {
                tool: known,
                model,
                command: String::new(),
                configured: true,
            };
        }
        let model_flag = if model.is_empty() {
            ""
        } else {
            "--model {model} "
        };
        Self {
            tool: Tool::Custom,
            command: format!("{} {model_flag}\"$(cat {{prompt}})\"", shell_quote(tool)),
            model,
            configured: true,
        }
    }

    fn to_raw(&self) -> RawAgent {
        let model = self.model.trim();
        RawAgent {
            tool: self.tool.key().to_string(),
            model: if model.is_empty() {
                DEFAULT_MODEL.to_string()
            } else {
                model.to_string()
            },
            command: if self.tool == Tool::Custom {
                self.command.clone()
            } else {
                String::new()
            },
        }
    }
}

/// The `[ai]` section as written on disk.
#[derive(Debug, Default, Deserialize, Serialize)]
struct RawAgent {
    #[serde(default)]
    tool: String,
    #[serde(default)]
    model: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    command: String,
}

#[derive(Debug, Default, Deserialize)]
struct FileConfig {
    #[serde(default)]
    ai: RawAgent,
}

/// Where the user configuration lives, or `None` when neither
/// `XDG_CONFIG_HOME` nor `HOME` is set.
pub fn config_file() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| home().map(|home| home.join(".config")))?;
    Some(base.join(CONFIG_DIR).join(CONFIG_FILE))
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// Reads the `[ai]` configuration from `file`, defaulting to an unconfigured
/// agent when the file is missing or unreadable.
pub fn load_from(file: Option<&Path>) -> AgentConfig {
    let Some(file) = file else {
        return AgentConfig::default();
    };
    let Ok(text) = std::fs::read_to_string(file) else {
        return AgentConfig::default();
    };
    toml::from_str::<FileConfig>(&text)
        .map(|config| AgentConfig::from_raw(config.ai))
        .unwrap_or_default()
}

/// Writes the `[ai]` configuration, creating the directory. Other sections of
/// the file, if any, are preserved.
pub fn save(file: &Path, config: &AgentConfig) -> Result<(), String> {
    let parent = file
        .parent()
        .ok_or_else(|| "the configuration has no folder".to_string())?;
    std::fs::create_dir_all(parent)
        .map_err(|err| format!("Couldn't create {}: {err}", parent.display()))?;

    let mut document = std::fs::read_to_string(file)
        .ok()
        .and_then(|text| toml::from_str::<toml::Value>(&text).ok())
        .and_then(|value| value.as_table().cloned())
        .unwrap_or_default();
    let ai = toml::Value::try_from(config.to_raw())
        .map_err(|err| format!("Couldn't encode the config: {err}"))?;
    document.insert("ai".to_string(), ai);
    let body =
        toml::to_string(&document).map_err(|err| format!("Couldn't write the config: {err}"))?;

    let temp = file.with_extension("tmp");
    std::fs::write(&temp, body)
        .map_err(|err| format!("Couldn't write {}: {err}", file.display()))?;
    std::fs::rename(&temp, file).map_err(|err| format!("Couldn't save {}: {err}", file.display()))
}

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
    if let Some(home) = home() {
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
fn search_path() -> OsString {
    std::env::join_paths(search_dirs()).unwrap_or_default()
}

/// The full path of an executable named `name`, if one is installed.
pub fn find_binary(name: &str) -> Option<PathBuf> {
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
    let version = Command::new(&path)
        .arg("--version")
        .env("PATH", search_path())
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| parse_version(&String::from_utf8_lossy(&output.stdout)));
    Detected {
        path: Some(path),
        version,
    }
}

/// The first version-looking word of `text` (`2.1.0 (Claude Code)`,
/// `opencode v2.0.20`), as `v2.1.0`.
fn parse_version(text: &str) -> Option<String> {
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
pub fn default_model(tool: Tool) -> Option<String> {
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
    let output = Command::new(binary)
        .arg("models")
        .env("PATH", search_path())
        .stdin(Stdio::null())
        .output()
        .map_err(|err| format!("Couldn't run opencode models: {err}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("opencode models exited with {}", output.status)
        } else {
            stderr
        });
    }
    Ok(parse_model_list(&String::from_utf8_lossy(&output.stdout)))
}

/// One model ID per line; anything with a space is a banner, not an ID.
fn parse_model_list(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.contains(char::is_whitespace))
        .map(str::to_string)
        .collect()
}

/// OpenCode keeps favorites in its state folder, in `model.json`.
fn opencode_state_file() -> Option<PathBuf> {
    let state = std::env::var_os("XDG_STATE_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| home().map(|home| home.join(".local/state")))?;
    Some(state.join("opencode/model.json"))
}

/// The `favorite` list of OpenCode's `model.json`, as `provider/model` IDs.
fn parse_opencode_favorites(text: &str) -> Vec<String> {
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
    let dir = std::env::var_os("CLAUDE_CONFIG_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| home().map(|home| home.join(".claude")))?;
    Some(dir.join("settings.json"))
}

fn parse_claude_model(text: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(text).ok()?;
    value["model"]
        .as_str()
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(str::to_string)
}

/// Quotes `text` for `sh`, so a path or model with spaces stays one word.
fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// A review running in the background.
pub struct Running {
    /// The agent's answer, or why it failed.
    pub answer: Receiver<Result<String, String>>,
    /// The child's process id, so the run can be cancelled.
    pub pid: Option<u32>,
}

/// Builds the command for `config`, ready to spawn. `model` is the resolved
/// one (empty for the tool's own choice); `prompt_file` holds `prompt` for a
/// custom command. Factored out so the argument shape per tool is testable
/// without running anything.
pub fn command(
    config: &AgentConfig,
    model: &str,
    prompt: &str,
    prompt_file: &Path,
    issue: &Path,
) -> Command {
    let mut command = match config.tool {
        Tool::Custom => {
            let script = config
                .command
                .replace("{prompt}", &shell_quote(&prompt_file.display().to_string()))
                .replace("{model}", &shell_quote(model))
                .replace("{issue}", &shell_quote(&issue.display().to_string()));
            let mut command = Command::new("sh");
            command.arg("-c").arg(script);
            command
        }
        tool => {
            let name = tool.binary().unwrap_or_default();
            let mut command =
                Command::new(find_binary(name).unwrap_or_else(|| PathBuf::from(name)));
            // `opencode run "…"` and `claude -p "…"` answer once and exit.
            command.arg(if tool == Tool::OpenCode { "run" } else { "-p" });
            if !model.is_empty() {
                command.arg("--model").arg(model);
            }
            command.arg(prompt);
            command
        }
    };
    command.env("PATH", search_path());
    command
}

/// Starts a review of the issue at `issue`. The tool is run without a terminal
/// (`stdin` from `/dev/null`), which is what one-shot modes need; a failure to
/// spawn is reported on the returned channel.
pub fn start(config: &AgentConfig, prompt: &str, issue: &Path) -> Running {
    let (sender, answer) = channel();
    let model = match config.model.trim() {
        "" if config.tool != Tool::Custom => default_model(config.tool).unwrap_or_default(),
        model => model.to_string(),
    };
    // A custom command reads the prompt from a file: it may be too long for
    // an argument, and `{prompt}` can then go anywhere in the command.
    let prompt_file = std::env::temp_dir().join(format!(
        "iboard-review-{}-{}.md",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or_default()
    ));
    let custom = config.tool == Tool::Custom;
    if custom && let Err(err) = std::fs::write(&prompt_file, prompt) {
        let _ = sender.send(Err(format!(
            "Couldn't write {}: {err}",
            prompt_file.display()
        )));
        return Running { answer, pid: None };
    }
    let cleanup = move |file: &Path| {
        if custom {
            let _ = std::fs::remove_file(file);
        }
    };

    let mut command = command(config, &model, prompt, &prompt_file, issue);
    match command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => {
            let pid = Some(child.id());
            std::thread::spawn(move || {
                let result = match child.wait_with_output() {
                    Ok(output) if output.status.success() => {
                        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
                        if text.is_empty() {
                            Err("the agent returned no answer".to_string())
                        } else {
                            Ok(text)
                        }
                    }
                    Ok(output) => {
                        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                        if stderr.is_empty() {
                            Err(format!("the agent exited with {}", output.status))
                        } else {
                            Err(stderr)
                        }
                    }
                    Err(err) => Err(format!("the agent could not be read: {err}")),
                };
                cleanup(&prompt_file);
                let _ = sender.send(result);
            });
            Running { answer, pid }
        }
        Err(err) => {
            cleanup(&prompt_file);
            let what = match config.tool {
                Tool::Custom => "the custom command",
                tool => tool.key(),
            };
            let _ = sender.send(Err(format!("Couldn't run {what}: {err}")));
            Running { answer, pid: None }
        }
    }
}

/// Asks the tool to stop, best effort.
pub fn cancel(pid: Option<u32>) {
    #[cfg(unix)]
    if let Some(pid) = pid {
        let _ = Command::new("kill")
            .arg("-TERM")
            .arg(pid.to_string())
            .status();
    }
    #[cfg(not(unix))]
    let _ = pid;
}

/// The verdict on one axis of the review.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Challenge,
    Block,
}

impl Verdict {
    fn parse(word: &str) -> Option<Self> {
        match word.trim().to_ascii_lowercase().as_str() {
            "pass" => Some(Self::Pass),
            "challenge" => Some(Self::Challenge),
            "block" => Some(Self::Block),
            _ => None,
        }
    }
}

/// One finding: an axis, a verdict and the agent's explanation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub axis: String,
    pub verdict: Verdict,
    pub body: String,
}

/// A parsed review: the three findings and, when the issue needed it, a
/// proposed rewrite. `proposal` is `None` for the word `NONE`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub findings: Vec<Finding>,
    pub proposal: Option<String>,
}

/// The instruction sent to the agent. It carries only the issue, per the
/// design: if that is not enough context, the issue is the problem.
pub fn review_prompt(content: &str) -> String {
    format!(
        "You review software issues before they are implemented. Judge the issue \
         below on three axes and answer with nothing but this exact format:\n\
         \n\
         RELEVANCE: Pass|Challenge|Block\n\
         <one or two sentences: is it still worth doing, does it duplicate another issue, \
         and is it tied to a clear user-facing outcome (its product impact)>\n\
         SIZE: Pass|Challenge|Block\n\
         <one or two sentences: can one person or agent finish it in one sitting, and if not, \
         the split; an issue over about 80 lines should be cut into smaller ones>\n\
         CLARITY: Pass|Challenge|Block\n\
         <one or two sentences: could a human or an agent implement it without a follow-up \
         question; is there enough guidance on the user experience; list what is missing, \
         contradictory or assumed>\n\
         PROPOSAL:\n\
         <a full improved issue in markdown, or the single word NONE>\n\
         \n\
         ---\n\
         {content}"
    )
}

/// Parses the agent's answer. Returns `None` when the answer does not carry the
/// three findings, so the UI can show it verbatim instead.
pub fn parse(text: &str) -> Option<Report> {
    const AXES: [&str; 3] = ["RELEVANCE", "SIZE", "CLARITY"];
    let mut findings: Vec<Finding> = Vec::new();
    let mut current: Option<(String, Verdict)> = None;
    let mut body = String::new();
    let mut proposal_lines: Vec<&str> = Vec::new();
    let mut in_proposal = false;

    let flush = |findings: &mut Vec<Finding>,
                 current: &mut Option<(String, Verdict)>,
                 body: &mut String| {
        if let Some((axis, verdict)) = current.take() {
            findings.push(Finding {
                axis,
                verdict,
                body: body.trim().to_string(),
            });
        }
        body.clear();
    };

    for line in text.lines() {
        let trimmed = line.trim();
        if in_proposal {
            proposal_lines.push(line);
            continue;
        }
        if let Some((head, rest)) = trimmed.split_once(':') {
            let key = head.trim().to_ascii_uppercase();
            if key == "PROPOSAL" {
                flush(&mut findings, &mut current, &mut body);
                in_proposal = true;
                let rest = rest.trim();
                if !rest.is_empty() {
                    proposal_lines.push(rest);
                }
                continue;
            }
            if AXES.contains(&key.as_str())
                && let Some(verdict) = Verdict::parse(rest)
            {
                flush(&mut findings, &mut current, &mut body);
                let axis = match key.as_str() {
                    "RELEVANCE" => "Relevance",
                    "SIZE" => "Size",
                    _ => "Clarity",
                };
                current = Some((axis.to_string(), verdict));
                continue;
            }
        }
        if current.is_some() {
            body.push_str(line);
            body.push('\n');
        }
    }
    flush(&mut findings, &mut current, &mut body);

    if findings.len() != AXES.len() {
        return None;
    }
    let proposal = proposal_lines.join("\n").trim().to_string();
    let proposal = if proposal.is_empty() || proposal.eq_ignore_ascii_case("none") {
        None
    } else {
        Some(proposal)
    };
    Some(Report { findings, proposal })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn custom(command: &str) -> AgentConfig {
        AgentConfig {
            tool: Tool::Custom,
            model: String::new(),
            command: command.into(),
            configured: true,
        }
    }

    fn args(command: &Command) -> Vec<String> {
        command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn a_custom_command_reads_the_prompt_file_and_answers() {
        let running = start(
            &custom("cat {prompt}"),
            "hello there",
            Path::new("todo/a.md"),
        );
        let answer = running
            .answer
            .recv_timeout(Duration::from_secs(5))
            .expect("the channel closes")
            .expect("cat succeeds");
        assert_eq!(answer, "hello there");
    }

    #[test]
    fn a_failing_command_reports_its_error_instead_of_panicking() {
        let running = start(
            &custom("echo nope >&2; exit 3 # {prompt}"),
            "x",
            Path::new("a.md"),
        );
        let error = running
            .answer
            .recv_timeout(Duration::from_secs(5))
            .expect("the channel closes")
            .expect_err("the command fails");
        assert_eq!(error, "nope");
    }

    #[test]
    fn each_tool_gets_its_one_shot_arguments() {
        let file = Path::new("/tmp/prompt.md");
        let issue = Path::new("/p/todo/a b.md");
        let opencode = AgentConfig {
            tool: Tool::OpenCode,
            ..AgentConfig::default()
        };
        let built = command(&opencode, "vendor/model", "PROMPT", file, issue);
        assert_eq!(args(&built), ["run", "--model", "vendor/model", "PROMPT"]);

        let claude = AgentConfig {
            tool: Tool::Claude,
            ..AgentConfig::default()
        };
        let built = command(&claude, "", "PROMPT", file, issue);
        assert_eq!(args(&built), ["-p", "PROMPT"]);

        let built = command(
            &custom("llm -m {model} < {prompt} # {issue}"),
            "gpt",
            "PROMPT",
            file,
            issue,
        );
        assert_eq!(built.get_program(), "sh");
        assert_eq!(
            args(&built),
            ["-c", "llm -m 'gpt' < '/tmp/prompt.md' # '/p/todo/a b.md'"]
        );
    }

    #[test]
    fn a_custom_command_without_prompt_does_not_validate() {
        assert!(custom("llm -m {model}").validate().is_err());
        assert!(custom("llm < {prompt}").validate().is_ok());
        assert!(AgentConfig::default().validate().is_ok());
    }

    #[test]
    fn the_label_names_the_tool_and_model() {
        let mut config = AgentConfig {
            tool: Tool::OpenCode,
            ..AgentConfig::default()
        };
        assert_eq!(config.label(), "opencode · default");
        config.model = "vendor/model".into();
        assert_eq!(config.label(), "opencode · vendor/model");
    }

    #[test]
    fn an_older_configuration_still_loads() {
        let load = |tool: &str, model: &str| {
            AgentConfig::from_raw(RawAgent {
                tool: tool.into(),
                model: model.into(),
                command: String::new(),
            })
        };
        let claude = load("/usr/local/bin/claude", "");
        assert_eq!(claude.tool, Tool::Claude);
        assert!(claude.configured());

        let other = load("my-agent", "fast");
        assert_eq!(other.tool, Tool::Custom);
        assert_eq!(
            other.command,
            "'my-agent' --model {model} \"$(cat {prompt})\""
        );
        assert_eq!(other.model, "fast");
        assert!(other.validate().is_ok());

        assert!(!load("", "").configured());
    }

    #[test]
    fn versions_and_model_lists_parse() {
        assert_eq!(
            parse_version("opencode v2.0.20\n").as_deref(),
            Some("v2.0.20")
        );
        assert_eq!(
            parse_version("2.1.287 (Claude Code)").as_deref(),
            Some("v2.1.287")
        );
        assert_eq!(parse_version("no version here"), None);

        assert_eq!(
            parse_model_list("Models available:\nanthropic/a\n\n  openai/b \n"),
            ["anthropic/a", "openai/b"]
        );
        assert_eq!(
            parse_opencode_favorites(
                r#"{"recent":[],"favorite":[{"providerID":"deepseek","modelID":"flash"}]}"#
            ),
            ["deepseek/flash"]
        );
        assert!(parse_opencode_favorites("not json").is_empty());
        assert_eq!(
            parse_claude_model(r#"{"model": "opus"}"#).as_deref(),
            Some("opus")
        );
        assert_eq!(parse_claude_model("{}"), None);
    }

    #[test]
    fn a_full_answer_parses_into_three_findings_and_a_proposal() {
        let answer = "\
RELEVANCE: Pass
Still needed, nothing else covers it.
SIZE: Challenge
Two features in one. Split them.
CLARITY: Block
An implementer would ask which tool runs it.
PROPOSAL:
# Better issue

Some context.";
        let report = parse(answer).expect("the shape is complete");
        assert_eq!(report.findings.len(), 3);
        assert_eq!(report.findings[0].axis, "Relevance");
        assert_eq!(report.findings[0].verdict, Verdict::Pass);
        assert_eq!(report.findings[1].verdict, Verdict::Challenge);
        assert_eq!(report.findings[2].verdict, Verdict::Block);
        assert!(report.findings[2].body.contains("which tool"));
        assert!(
            report
                .proposal
                .as_deref()
                .unwrap()
                .starts_with("# Better issue")
        );
    }

    #[test]
    fn a_none_proposal_is_dropped() {
        let answer = "RELEVANCE: Pass\nok\nSIZE: Pass\nok\nCLARITY: Pass\nok\nPROPOSAL:\nnone";
        let report = parse(answer).unwrap();
        assert!(report.proposal.is_none());
    }

    #[test]
    fn free_form_answers_do_not_parse() {
        assert!(parse("This is just prose the agent wrote.").is_none());
        let partial = "RELEVANCE: Pass\nok\nSIZE: Pass\nok";
        assert!(parse(partial).is_none());
    }

    #[test]
    fn the_configuration_round_trips_through_a_file() {
        let dir = std::env::temp_dir().join(format!(
            "iboard-agent-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join(CONFIG_FILE);

        let config = AgentConfig {
            tool: Tool::OpenCode,
            model: "vendor/model".into(),
            command: String::new(),
            configured: true,
        };
        save(&file, &config).unwrap();
        assert_eq!(load_from(Some(&file)), config);

        // A missing file is simply an unconfigured agent.
        assert!(!load_from(Some(&dir.join("absent.toml"))).configured());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn saving_the_agent_keeps_other_sections() {
        let dir = std::env::temp_dir().join(format!(
            "iboard-agent-merge-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join(CONFIG_FILE);
        std::fs::write(&file, "[other]\nkey = 1\n").unwrap();

        save(
            &file,
            &AgentConfig {
                tool: Tool::OpenCode,
                configured: true,
                ..AgentConfig::default()
            },
        )
        .unwrap();

        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("[other]"), "the other section survives");
        assert!(text.contains("tool = \"opencode\""));
        assert!(text.contains("model = \"default\""));
        assert_eq!(load_from(Some(&file)).tool, Tool::OpenCode);

        std::fs::remove_dir_all(&dir).ok();
    }
}
