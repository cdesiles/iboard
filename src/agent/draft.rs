//! Draft with Review: the agent writes a new issue from its author's notes,
//! and its text is shared as it arrives so the sheet fills in place.

use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use super::{
    AgentConfig, Purpose, Tool, Workspace, command, default_model, final_answer, prompt_file,
    stream_error,
};
use crate::files;
use crate::job::{Cancel, Job};
use crate::process;

/// The agent's text so far, shared with the modal that shows it.
pub type DraftText = Arc<Mutex<String>>;

/// A copy of what the agent has written so far.
pub fn draft_so_far(text: &DraftText) -> String {
    text.lock().unwrap_or_else(PoisonError::into_inner).clone()
}

/// Runs the agent on `prompt` from `project` on a background thread; its text
/// lands in the returned [`DraftText`] as it comes. The job answers with the
/// whole text. Dropping the job stops the agent.
pub fn start_draft(
    config: &AgentConfig,
    prompt: &str,
    project: Option<&Path>,
) -> (Job<Result<String, String>>, DraftText) {
    let (config, prompt) = (config.clone(), prompt.to_string());
    let project = project.map(Path::to_path_buf);
    let text = DraftText::default();
    let shared = Arc::clone(&text);
    let job =
        Job::spawn(move |cancel| run_draft(&config, &prompt, project.as_deref(), cancel, &shared));
    (job, text)
}

fn run_draft(
    config: &AgentConfig,
    prompt: &str,
    project: Option<&Path>,
    cancel: &Cancel,
    text: &DraftText,
) -> Result<String, String> {
    let model = match config.model.trim() {
        "" if config.tool != Tool::Custom => default_model(config.tool).unwrap_or_default(),
        model => model.to_string(),
    };
    let custom = config.tool == Tool::Custom;
    let prompt_file = prompt_file(config, prompt)?;
    let what = match config.tool {
        Tool::Custom => "the custom command",
        tool => tool.key(),
    };
    let issue = project.unwrap_or(Path::new("."));
    let prompt_path = prompt_file
        .as_ref()
        .map_or(Path::new(""), files::TempFile::path);
    let mut command = command(
        config,
        &model,
        prompt,
        prompt_path,
        issue,
        Purpose::Draft,
        None,
    );
    if let Some(dir) = project.map(Workspace::of) {
        command.current_dir(dir.dir);
    }
    let mut partial = false;
    let output = process::run_with(command, what, cancel, Purpose::Draft.timeout(), |line| {
        let piece = if custom {
            Some(format!("{line}\n"))
        } else {
            piece_of(line, &mut partial)
        };
        if let Some(piece) = piece {
            text.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push_str(&piece);
        }
    });
    drop(prompt_file);
    let output = output?;
    let failure = stream_error(&output.stdout);
    let answer = output
        .into_result("the agent")
        .map_err(|err| failure.clone().unwrap_or(err))?;
    let answer = if custom {
        answer
    } else {
        final_answer(&answer)
    };
    match answer.trim() {
        "" => Err(failure.unwrap_or_else(|| "the agent returned no answer".to_string())),
        answer => Ok(answer.to_string()),
    }
}

/// The text one streamed event adds: Claude Code's text deltas (or its whole
/// message when it sent none), OpenCode's text parts.
fn piece_of(line: &str, partial: &mut bool) -> Option<String> {
    let event: serde_json::Value = serde_json::from_str(line).ok()?;
    match event["type"].as_str()? {
        "stream_event" => {
            let delta = &event["event"]["delta"];
            (delta["type"] == "text_delta").then(|| {
                *partial = true;
                delta["text"].as_str().unwrap_or_default().to_string()
            })
        }
        "assistant" if !*partial => {
            let parts = event["message"]["content"].as_array()?;
            Some(
                parts
                    .iter()
                    .filter(|part| part["type"] == "text")
                    .filter_map(|part| part["text"].as_str())
                    .collect(),
            )
        }
        "text" => event["part"]["text"].as_str().map(str::to_string),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deltas_stream_and_the_whole_message_is_not_doubled() {
        let mut partial = false;
        let delta = r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"{\"title\""}}}"#;
        assert_eq!(piece_of(delta, &mut partial).as_deref(), Some("{\"title\""));
        let whole = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"all"}]}}"#;
        assert_eq!(piece_of(whole, &mut partial), None);
        assert_eq!(piece_of(whole, &mut false).as_deref(), Some("all"));
        let opencode = "{\"type\":\"text\",\"part\":{\"text\":\"Hi there\"}}";
        assert_eq!(piece_of(opencode, &mut false).as_deref(), Some("Hi there"));
    }

    #[test]
    fn a_custom_command_streams_its_lines() {
        let config = AgentConfig {
            tool: Tool::Custom,
            command: "printf '{\"title\":\"Export\"}\\n{\"section\":\"Context\",\"text\":\"Hi\"}\\n' # {prompt}"
                .to_string(),
            ..AgentConfig::default()
        };
        let (job, text) = start_draft(&config, "notes", None);
        let answer = job.wait().expect("answered").expect("drafted");
        assert!(answer.contains("Context"));
        assert!(text.lock().unwrap().contains("{\"title\":\"Export\"}\n"));
    }
}
