//! Names a new issue from its description.
//!
//! The engine is chosen in Settings › Titles and stored in the user
//! configuration (`~/.config/iboard/iboard.toml`):
//!
//! ```toml
//! [titles]
//! engine = "apple"   # "apple" | "agent" | "manual"
//! ```
//!
//! `apple` is the on-device model (Foundation Models, macOS 26+ with Apple
//! Intelligence), reached through the Swift shim in `macos/titles.swift`;
//! `agent` asks the tool configured for AI review. Neither is ever required:
//! without a title, [`first_line_title`] names the issue from the description.

use std::sync::mpsc::{Receiver, channel};

use serde::{Deserialize, Serialize};

use crate::agent::{self, AgentConfig};
use crate::settings::{Section, UserConfig};

/// The longest title the composer accepts.
pub const MAX_TITLE_CHARS: usize = 80;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Engine {
    /// Apple's on-device model.
    #[default]
    Apple,
    /// The tool and model configured for AI review.
    Agent,
    /// The user types every title.
    Manual,
}

impl Engine {
    pub const ALL: [Engine; 3] = [Engine::Apple, Engine::Agent, Engine::Manual];

    pub fn key(self) -> &'static str {
        match self {
            Engine::Apple => "apple",
            Engine::Agent => "agent",
            Engine::Manual => "manual",
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        // `off` is what Manual used to be called.
        if key == "off" {
            return Some(Engine::Manual);
        }
        Engine::ALL.into_iter().find(|engine| engine.key() == key)
    }

    /// The name in Settings › Titles.
    pub fn name(self) -> &'static str {
        match self {
            Engine::Apple => "On-device (Apple)",
            Engine::Agent => "Review agent",
            Engine::Manual => "Manual",
        }
    }

    /// The composer's tag for a working engine.
    pub fn tag(self) -> &'static str {
        match self {
            Engine::Apple => "on-device",
            Engine::Agent => "review agent",
            Engine::Manual => "manual",
        }
    }

    pub fn help(self) -> &'static str {
        match self {
            Engine::Apple => "Offline, private and free. Nothing leaves this Mac.",
            Engine::Agent => {
                "Uses the tool and model from AI review. Any OS; needs that tool set up and a network."
            }
            Engine::Manual => {
                "You type every title. The title line is always shown in the composer."
            }
        }
    }
}

/// Whether an engine can name an issue right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Availability {
    Ready,
    /// `state` is the short word on the right of the row; `why` the sentence
    /// under it; `fixable` says whether the user can change it here.
    Unavailable {
        state: &'static str,
        why: String,
        fixable: bool,
    },
}

impl Availability {
    pub fn is_ready(&self) -> bool {
        matches!(self, Availability::Ready)
    }
}

/// Checks `engine` now; never cached, since the user can turn Apple
/// Intelligence on or set up an agent while iboard runs.
pub fn availability(engine: Engine, agent: &AgentConfig) -> Availability {
    match engine {
        Engine::Apple => apple::availability(),
        Engine::Agent if agent.configured() => Availability::Ready,
        Engine::Agent => Availability::Unavailable {
            state: "Set up first",
            why: "No tool is set up for AI review yet.".to_string(),
            fixable: true,
        },
        Engine::Manual => Availability::Ready,
    }
}

/// The languages the on-device model reads, by their own names, as macOS
/// reports them.
pub fn apple_languages() -> Vec<String> {
    apple::languages()
}

/// The Mac's own language by its own name, and whether the on-device model
/// reads it.
pub fn user_language() -> Option<(String, bool)> {
    apple::user_language()
}

/// Loads the on-device model ahead of a request, so the first title does not
/// pay for it. Cheap when already loaded; does nothing elsewhere.
pub fn prewarm(engine: Engine) {
    if engine == Engine::Apple {
        apple::prewarm();
    }
}

/// Names `description` on a background thread; the answer arrives on the
/// returned channel, already cleaned.
pub fn start(
    engine: Engine,
    agent: &AgentConfig,
    description: &str,
) -> Receiver<Result<String, String>> {
    let (sender, answer) = channel();
    let description = description.to_string();
    match engine {
        Engine::Apple => {
            std::thread::spawn(move || {
                let result = apple::infer(&description).and_then(|raw| clean(&raw));
                let _ = sender.send(result);
            });
        }
        Engine::Agent => {
            // A custom command may read the description from `{issue}`.
            let file = std::env::temp_dir().join(format!(
                "iboard-title-{}-{}.md",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|elapsed| elapsed.as_nanos())
                    .unwrap_or_default()
            ));
            if let Err(err) = std::fs::write(&file, &description) {
                let _ = sender.send(Err(format!("Couldn't write {}: {err}", file.display())));
                return answer;
            }
            let running = agent::start(agent, &agent_prompt(&description), &file);
            std::thread::spawn(move || {
                let result = running
                    .answer
                    .recv()
                    .unwrap_or_else(|_| Err("the agent stopped".to_string()))
                    .and_then(|raw| clean(&raw));
                let _ = std::fs::remove_file(&file);
                let _ = sender.send(result);
            });
        }
        Engine::Manual => {
            let _ = sender.send(Err("automatic titles are off".to_string()));
        }
    }
    answer
}

fn agent_prompt(description: &str) -> String {
    format!(
        "Write the title of this issue for a developer's issue tracker: what should be done \
         or what is wrong, in at most 10 words, in the same language as the description. \
         Keep the meaning; do not add facts. Answer with the title only: no quotes, no ticket \
         number, no final period.\n\nDescription:\n{description}\n"
    )
}

/// One line, no wrapping quotes, label or final period, at most
/// [`MAX_TITLE_CHARS`]. An empty result is an error.
fn clean(raw: &str) -> Result<String, String> {
    let line = raw
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    let line = line
        .strip_prefix("Title:")
        .or_else(|| line.strip_prefix("Titre :"))
        .or_else(|| line.strip_prefix("Titre:"))
        .unwrap_or(line);
    let quotes: &[char] = &['"', '\'', '`', '«', '»', '“', '”', '‘', '’', '*', '#', ' '];
    let title = line
        .trim()
        .trim_matches(quotes)
        .trim_end_matches('.')
        .trim();
    let title = cut(title, MAX_TITLE_CHARS);
    if title.is_empty() {
        Err("the engine returned no title".to_string())
    } else {
        Ok(title)
    }
}

/// The title used when nothing named the issue: the description's first
/// non-empty line, cut at a word boundary to [`MAX_TITLE_CHARS`].
pub fn first_line_title(description: &str) -> String {
    let line = description
        .lines()
        .map(|line| line.trim().trim_start_matches('#').trim())
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    cut(line, MAX_TITLE_CHARS)
}

/// At most `max` characters, cut at the last space when there is one.
fn cut(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let head: String = text.chars().take(max).collect();
    match head.rfind(' ') {
        Some(at) if at > 0 => head[..at].trim_end().to_string(),
        _ => head,
    }
}

/// The `[titles]` section of the user configuration.
#[derive(Default, Deserialize, Serialize)]
struct RawTitles {
    engine: Option<String>,
}

/// The engine `[titles]` names; Apple by default.
pub fn load_engine(config: &mut UserConfig) -> Engine {
    let raw: RawTitles = config.section("titles");
    let Some(key) = raw.engine else {
        return Engine::default();
    };
    Engine::from_key(key.trim()).unwrap_or_else(|| {
        config.problem(format!(
            "[titles]: engine \"{key}\" is not apple, agent or manual"
        ));
        Engine::default()
    })
}

/// The `[titles]` section that stores `engine`.
pub fn section(engine: Engine) -> Result<Section, String> {
    Section::new(
        "titles",
        &RawTitles {
            engine: Some(engine.key().to_string()),
        },
    )
}

#[cfg(apple_titles)]
mod apple {
    use super::Availability;
    use std::ffi::{CStr, CString, c_char};

    unsafe extern "C" {
        fn iboard_titles_availability() -> i32;
        fn iboard_titles_prewarm();
        fn iboard_titles_languages(out: *mut c_char, capacity: isize) -> isize;
        fn iboard_titles_user_language(out: *mut c_char, capacity: isize) -> isize;
        fn iboard_titles_infer(
            description: *const c_char,
            out: *mut c_char,
            capacity: isize,
        ) -> isize;
    }

    pub fn availability() -> Availability {
        // SAFETY: takes no arguments and only reads system state.
        let code = unsafe { iboard_titles_availability() };
        let unavailable = |state, why: &str, fixable| Availability::Unavailable {
            state,
            why: why.to_string(),
            fixable,
        };
        match code {
            0 => Availability::Ready,
            1 => unavailable(
                "Not on this Mac",
                "This Mac can't run Apple Intelligence.",
                false,
            ),
            2 => unavailable(
                "Apple Intelligence off",
                "It is turned off on this Mac. Turn it on, then come back.",
                true,
            ),
            3 => unavailable(
                "Preparing",
                "Apple Intelligence is still downloading its model.",
                false,
            ),
            4 => unavailable(
                "Needs macOS 26",
                &format!("This Mac runs macOS {}.", macos_version()),
                false,
            ),
            _ => unavailable("Unavailable", "macOS didn't say why.", false),
        }
    }

    pub fn prewarm() {
        // SAFETY: takes no arguments and returns at once.
        unsafe { iboard_titles_prewarm() }
    }

    pub fn languages() -> Vec<String> {
        read(iboard_titles_languages)
            .map(|text| text.lines().map(str::to_string).collect())
            .unwrap_or_default()
    }

    pub fn user_language() -> Option<(String, bool)> {
        let text = read(iboard_titles_user_language)?;
        let (name, supported) = text.split_once('\t')?;
        Some((name.to_string(), supported == "1"))
    }

    /// Calls a shim function that fills a buffer.
    fn read(call: unsafe extern "C" fn(*mut c_char, isize) -> isize) -> Option<String> {
        let mut buffer = vec![0 as c_char; 4096];
        // SAFETY: the shim writes at most `capacity` bytes, NUL included.
        let written = unsafe { call(buffer.as_mut_ptr(), buffer.len() as isize) };
        if written < 0 {
            return None;
        }
        // SAFETY: the shim NUL-terminates what it writes.
        Some(
            unsafe { CStr::from_ptr(buffer.as_ptr()) }
                .to_string_lossy()
                .into_owned(),
        )
    }

    /// Blocks until the model answers; call it off the UI thread.
    pub fn infer(description: &str) -> Result<String, String> {
        let input = CString::new(description.replace('\0', " ")).map_err(|err| err.to_string())?;
        let mut buffer = vec![0 as c_char; 2048];
        // SAFETY: `input` is NUL-terminated; the shim writes at most
        // `capacity` bytes, NUL included, and blocks until done.
        let written = unsafe {
            iboard_titles_infer(input.as_ptr(), buffer.as_mut_ptr(), buffer.len() as isize)
        };
        // SAFETY: the shim NUL-terminates what it writes.
        let text = unsafe { CStr::from_ptr(buffer.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        if written < 0 {
            Err(if text.is_empty() {
                "the on-device model is unavailable".to_string()
            } else {
                text
            })
        } else {
            Ok(text)
        }
    }

    fn macos_version() -> String {
        std::process::Command::new("sw_vers")
            .arg("-productVersion")
            .output()
            .ok()
            .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
            .filter(|version| !version.is_empty())
            .unwrap_or_else(|| "an older version".to_string())
    }
}

#[cfg(not(apple_titles))]
mod apple {
    use super::Availability;

    pub fn availability() -> Availability {
        if cfg!(target_os = "macos") {
            Availability::Unavailable {
                state: "Not in this build",
                why: "iboard was built without the Swift shim (swiftc was missing).".to_string(),
                fixable: false,
            }
        } else {
            Availability::Unavailable {
                state: "Not on this OS",
                why: "Needs macOS 26 or later with Apple Intelligence.".to_string(),
                fixable: false,
            }
        }
    }

    pub fn prewarm() {}

    pub fn languages() -> Vec<String> {
        Vec::new()
    }

    pub fn user_language() -> Option<(String, bool)> {
        None
    }

    pub fn infer(_description: &str) -> Result<String, String> {
        Err("the on-device model is not in this build".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_are_cleaned_to_one_short_line() {
        assert_eq!(
            clean("Title: \"Fix the login redirect.\"\n\nBecause…").unwrap(),
            "Fix the login redirect"
        );
        assert_eq!(
            clean("  « Exporter le tableau »  ").unwrap(),
            "Exporter le tableau"
        );
        assert!(clean("\n  \n").is_err());
        let long = "word ".repeat(30);
        assert!(clean(&long).unwrap().chars().count() <= MAX_TITLE_CHARS);
    }

    #[test]
    fn the_first_line_names_an_issue_at_a_word_boundary() {
        assert_eq!(
            first_line_title("\n# Export the board\nmore"),
            "Export the board"
        );
        let long = format!("{} tail", "abcdefghi ".repeat(8));
        let title = first_line_title(&long);
        assert!(title.chars().count() <= MAX_TITLE_CHARS);
        assert!(!title.ends_with(' '));
        assert!(title.ends_with("abcdefghi"));
    }

    #[test]
    fn the_engine_round_trips_through_the_config() {
        let dir = std::env::temp_dir().join(format!("iboard-titles-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("iboard.toml");
        let engine = || load_engine(&mut UserConfig::load(Some(&file)));
        std::fs::write(&file, "[ai]\ntool = \"claude\"\n").unwrap();
        assert_eq!(engine(), Engine::Apple);
        crate::settings::save(&file, &[section(Engine::Manual).unwrap()]).unwrap();
        assert_eq!(engine(), Engine::Manual);
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(
            text.contains("tool = \"claude\""),
            "other sections stay: {text}"
        );
        // The old name still reads as Manual.
        std::fs::write(&file, "[titles]\nengine = \"off\"\n").unwrap();
        assert_eq!(engine(), Engine::Manual);
        // An unknown one is reported.
        let mut config = UserConfig::parse("[titles]\nengine = \"gpt\"\n", &file);
        assert_eq!(load_engine(&mut config), Engine::Apple);
        assert_eq!(config.problems.len(), 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn off_is_always_ready_and_an_unset_agent_is_not() {
        let agent = AgentConfig::default();
        assert!(availability(Engine::Manual, &agent).is_ready());
        assert!(!availability(Engine::Agent, &agent).is_ready());
    }
}

/// Runs the real on-device model: `cargo test apple_names -- --ignored`.
#[cfg(all(test, apple_titles))]
mod apple_tests {
    use super::*;

    #[test]
    #[ignore = "needs Apple Intelligence"]
    fn apple_names_a_french_description() {
        if !apple::availability().is_ready() {
            return;
        }
        let agent = AgentConfig::default();
        let answer = start(
            Engine::Apple,
            &agent,
            "quand je double clique sur une colonne ça devrait créer un ticket directement dedans",
        )
        .recv()
        .unwrap();
        let title = answer.unwrap();
        assert!(!title.is_empty() && title.chars().count() <= MAX_TITLE_CHARS);
        assert!(!apple_languages().is_empty());
        eprintln!("title: {title}");
    }
}
