//! Continue in terminal: the command that resumes an ask's Claude Code
//! session from the issue's project, copied for the author to paste where
//! they work, or typed in a new tab of their terminal.

use std::path::Path;
use std::process::Command;

/// Where Continue in terminal takes the session: `[terminal] app`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Terminal {
    /// The command goes to the clipboard.
    #[default]
    Copy,
    /// A new Terminal.app window types it.
    Terminal,
    /// A new iTerm2 tab types it.
    ITerm,
}

impl Terminal {
    /// The `[terminal] app` value, as `"iTerm"`; `None` for an unknown one.
    pub fn parse(app: &str) -> Option<Self> {
        match app.trim().to_ascii_lowercase().as_str() {
            "" | "copy" => Some(Terminal::Copy),
            "terminal" => Some(Terminal::Terminal),
            "iterm" | "iterm2" => Some(Terminal::ITerm),
            _ => None,
        }
    }

    /// What the panel's button says.
    pub fn label(self) -> &'static str {
        match self {
            Terminal::Copy => "Copy resume command",
            Terminal::Terminal => "Continue in Terminal",
            Terminal::ITerm => "Continue in iTerm",
        }
    }

    /// The AppleScript that types `command` in a new window or tab; `None`
    /// when the command is only copied.
    fn script(self, command: &str) -> Option<String> {
        let typed = command.replace('\\', "\\\\").replace('"', "\\\"");
        match self {
            Terminal::Copy => None,
            Terminal::Terminal => Some(format!(
                "tell application \"Terminal\"\nactivate\ndo script \"{typed}\"\nend tell"
            )),
            Terminal::ITerm => Some(format!(
                "tell application \"iTerm\"\nactivate\n\
                 if (count of windows) is 0 then\ncreate window with default profile\n\
                 else\ntell current window to create tab with default profile\nend if\n\
                 tell current session of current window to write text \"{typed}\"\nend tell"
            )),
        }
    }

    /// Types `command` in the terminal; nothing for [`Terminal::Copy`].
    pub fn open(self, command: &str) -> Result<(), String> {
        let Some(script) = self.script(command) else {
            return Ok(());
        };
        let output = Command::new("osascript")
            .arg("-e")
            .arg(script)
            .output()
            .map_err(|err| format!("Couldn't run osascript: {err}"))?;
        match output.status.success() {
            true => Ok(()),
            false => Err(format!(
                "{} failed: {}",
                self.label(),
                String::from_utf8_lossy(&output.stderr).trim()
            )),
        }
    }
}

/// `text` as one single-quoted shell word.
fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// The command that resumes `session` from `project`. An interactive shell
/// runs it, so its direnv hook loads the project's `.envrc`.
pub fn resume_command(project: &Path, session: &str) -> String {
    format!(
        "cd {} && claude --resume {}",
        quoted(&project.to_string_lossy()),
        quoted(session)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_resumes_the_session_in_the_project() {
        assert_eq!(
            resume_command(Path::new("/p/it's"), "s-42"),
            "cd '/p/it'\\''s' && claude --resume 's-42'"
        );
    }

    #[test]
    fn the_terminal_comes_from_the_config_and_types_the_command() {
        assert_eq!(Terminal::parse("iTerm2"), Some(Terminal::ITerm));
        assert_eq!(Terminal::parse(" Terminal "), Some(Terminal::Terminal));
        assert_eq!(Terminal::parse(""), Some(Terminal::Copy));
        assert_eq!(Terminal::parse("kitty"), None);
        assert_eq!(Terminal::Copy.script("x"), None);
        let script = Terminal::ITerm.script(r#"cd "a\b""#).unwrap();
        assert!(script.contains(r#"write text "cd \"a\\b\"""#), "{script}");
        assert!(script.contains("create tab with default profile"));
        let script = Terminal::Terminal.script("ls").unwrap();
        assert!(script.contains("do script \"ls\""), "{script}");
    }
}
