//! The arguments of one agent run, and a custom command's placeholders.

use std::path::Path;
use std::process::Command;

use super::{AgentConfig, Leash, PLACEHOLDERS, Purpose, ReviewMode, Tool, search_path};

/// Builds the command for `config`, ready to spawn. `model` is the resolved
/// one (empty for the tool's own choice); `prompt_file` holds `prompt` for a
/// custom command. Factored out so the argument shape per tool is testable
/// without running anything.
pub(super) fn command(
    config: &AgentConfig,
    model: &str,
    prompt: &str,
    prompt_file: &Path,
    issue: &Path,
    purpose: Purpose,
    session: Option<&str>,
) -> Command {
    let mut command = match config.tool {
        Tool::Custom => {
            let mut command = Command::new("sh");
            command
                .arg("-c")
                .arg(shell_script(&config.command))
                .env("IBOARD_PROMPT", prompt_file)
                .env("IBOARD_MODEL", model)
                .env("IBOARD_ISSUE", issue)
                .env("IBOARD_MODE", purpose.key())
                .env("IBOARD_SESSION", session.unwrap_or_default());
            command
        }
        tool => {
            let mut command = Command::new(tool.program());
            // `opencode run "…"` and `claude -p "…"` answer once and exit.
            command.arg(if tool == Tool::OpenCode { "run" } else { "-p" });
            // A run starts in a worktree of its own, so the checkout stays as
            // it was; a tool without the flag just runs in the checkout.
            if tool == Tool::OpenCode && config.worktree.unwrap_or(true) {
                command.arg("--worktree");
            }
            // A review, a build or a plan streams its steps as JSON events, so
            // iboard can say what the agent is doing; a quick review gets no
            // built-in tools, a build may edit files.
            match (tool, purpose) {
                (_, Purpose::Answer(_)) => {}
                (Tool::OpenCode, _) => {
                    command.args(["--format", "json"]);
                }
                (_, _) => {
                    command.args(["--output-format", "stream-json", "--verbose"]);
                    match purpose {
                        Purpose::Draft => {
                            command.args(["--include-partial-messages", "--tools", ""]);
                        }
                        Purpose::Review {
                            mode: ReviewMode::Quick,
                            ..
                        } => {
                            command.args(["--tools", ""]);
                        }
                        Purpose::Build(_) | Purpose::Ask(Leash { edits: true, .. }, _) => {
                            command.args(["--permission-mode", "acceptEdits"]);
                        }
                        _ => {}
                    }
                }
            }
            if let Purpose::Review { plan: true, .. }
            | Purpose::Ask(Leash { edits: false, .. }, _)
            | Purpose::Plan {
                plan_mode: true, ..
            } = purpose
            {
                command.args(tool.plan_args());
            }
            if let (Tool::Claude, Purpose::Ask(leash, _)) = (tool, purpose) {
                let denied = leash.denied();
                if !denied.is_empty() {
                    command.arg("--disallowedTools").arg(denied.join(","));
                }
            }
            if let Some(session) = session.filter(|_| matches!(purpose, Purpose::Plan { .. })) {
                match tool {
                    Tool::OpenCode => {
                        command.arg("--session").arg(session);
                    }
                    Tool::Claude => {
                        command.arg("--resume").arg(session);
                    }
                    Tool::Custom => {}
                }
            }
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

/// A custom command with its placeholders turned into quoted shell variables,
/// so no file name or model is ever read as code. A placeholder the command
/// already wrapped in quotes keeps one pair of them.
fn shell_script(command: &str) -> String {
    let mut script = command.to_string();
    for (placeholder, variable) in PLACEHOLDERS {
        let quoted = format!("\"${variable}\"");
        for wrapped in [format!("\"{placeholder}\""), format!("'{placeholder}'")] {
            script = script.replace(&wrapped, &quoted);
        }
        script = script.replace(placeholder, &quoted);
    }
    script
}
