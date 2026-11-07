//! What an agent's event stream says: the step it is on, the answer it gave,
//! and the error it reported. OpenCode streams one event per line; Claude
//! Code, `--output-format stream-json`, the same shape with other names.

/// What one streamed event says the agent is doing: a tool call from
/// OpenCode (`tool_use`) or Claude Code (`assistant` with a `tool_use` part).
pub(crate) fn step_of(line: &str) -> Option<String> {
    let event: serde_json::Value = serde_json::from_str(line).ok()?;
    match event["type"].as_str()? {
        "tool_use" => describe_tool(
            event["part"]["tool"].as_str()?,
            &event["part"]["state"]["input"],
        ),
        "assistant" => event["message"]["content"]
            .as_array()?
            .iter()
            .rev()
            .find(|part| part["type"] == "tool_use")
            .and_then(|part| describe_tool(part["name"].as_str()?, &part["input"])),
        _ => None,
    }
}

/// The session an event names: OpenCode's `sessionID`, or Claude Code's
/// `session_id` on its init and result events.
pub(crate) fn session_of(line: &str) -> Option<String> {
    let event: serde_json::Value = serde_json::from_str(line).ok()?;
    event["sessionID"]
        .as_str()
        .or_else(|| event["session_id"].as_str())
        .or_else(|| event["data"]["sessionID"].as_str())
        .or_else(|| event["data"]["session_id"].as_str())
        .map(str::to_string)
}

/// "Reading app/review.rs…": the tool as a verb, and the tail of its target.
pub(crate) fn describe_tool(tool: &str, input: &serde_json::Value) -> Option<String> {
    let verb = match tool.to_ascii_lowercase().as_str() {
        "read" => "Reading".to_string(),
        "grep" | "glob" | "list" | "ls" | "search" => "Searching".to_string(),
        "bash" => "Running".to_string(),
        "webfetch" | "fetch" | "websearch" => "Looking up".to_string(),
        _ => format!("Using {tool}"),
    };
    let target = ["file_path", "path", "pattern", "command", "url", "query"]
        .iter()
        .find_map(|key| input[*key].as_str());
    Some(match target {
        Some(target) => {
            let parts: Vec<&str> = target.trim_end_matches('/').rsplit('/').take(2).collect();
            let tail: Vec<&str> = parts.into_iter().rev().collect();
            format!("{verb} {}…", tail.join("/"))
        }
        None => format!("{verb}…"),
    })
}

/// The answer in a stream of events: Claude Code's `result`, else the text
/// of OpenCode's last step. Plain text that is no event stream stays as is.
pub(crate) fn final_answer(stdout: &str) -> String {
    let events: Vec<serde_json::Value> = stdout
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    if events.is_empty() {
        return stdout.to_string();
    }
    let mut texts: Vec<&str> = Vec::new();
    for event in &events {
        match event["type"].as_str() {
            Some("result") if event["result"].is_string() => {
                return event["result"].as_str().unwrap_or_default().to_string();
            }
            Some("step_start") => texts.clear(),
            Some("text") => texts.extend(event["part"]["text"].as_str()),
            _ => {}
        }
    }
    texts.concat()
}

/// The error an agent reported in its event stream, where its error output
/// stays empty: OpenCode's `error` event, or Claude Code's failed `result`.
pub(crate) fn stream_error(stdout: &str) -> Option<String> {
    stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter_map(|event| match event["type"].as_str()? {
            "error" => event["error"]["message"]
                .as_str()
                .or(event["error"]["data"]["message"].as_str())
                .or(event["error"].as_str())
                .or(event["message"].as_str())
                .map(str::to_string),
            "result" if event["is_error"] == true => event["result"]
                .as_str()
                .or(event["subtype"].as_str())
                .map(str::to_string),
            _ => None,
        })
        .map(|message| match message.strip_prefix('[') {
            Some(rest) => rest
                .split_once("] ")
                .map_or(message.clone(), |(_, text)| text.to_string()),
            None => message,
        })
        .next_back()
}
