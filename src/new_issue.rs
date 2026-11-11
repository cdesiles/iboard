//! A new issue before it is a file: the agent's draft as it streams in, what
//! `↩` writes, and the notes parked between two openings of the modal. See
//! `design/components/NewIssueModal`.

use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use crate::labels;
use crate::markdown;

/// One `##` section of a draft.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Section {
    pub heading: String,
    pub text: String,
}

/// What the agent has written so far, from title to last section.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Draft {
    pub title: Option<String>,
    pub labels: Vec<String>,
    pub summary: String,
    pub sections: Vec<Section>,
    /// The section still being written, while the agent works.
    pub writing: Option<usize>,
}

impl Draft {
    /// Nothing worth showing has arrived yet.
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.labels.is_empty()
            && self.summary.trim().is_empty()
            && self.sections.iter().all(|s| s.text.trim().is_empty())
    }

    /// The headings of `wanted` the draft has not written.
    pub fn unwritten<'a>(&self, wanted: &'a [String]) -> Vec<&'a str> {
        wanted
            .iter()
            .filter(|heading| {
                !self.sections.iter().any(|section| {
                    section.heading.eq_ignore_ascii_case(heading) && !section.text.trim().is_empty()
                })
            })
            .map(String::as_str)
            .collect()
    }
}

/// Reads the agent's answer so far: JSON lines (`{"title"}`, `{"labels"}`,
/// `{"summary"}`, `{"section","text"}`), or plain markdown split on its
/// `## ` headings. Until `done`, the last line may be cut mid-way, and text
/// is shown up to its last whole word.
pub fn parse(text: &str, done: bool) -> Draft {
    let first = text.lines().map(str::trim).find(|line| !line.is_empty());
    let mut draft = match first {
        Some(line) if line.starts_with('{') => parse_events(text, done),
        _ => parse_markdown(text, done),
    };
    if done {
        draft.writing = None;
    }
    draft
}

fn parse_events(text: &str, done: bool) -> Draft {
    let mut draft = Draft::default();
    let lines: Vec<&str> = text.lines().collect();
    let complete = if done || text.ends_with('\n') {
        lines.len()
    } else {
        lines.len().saturating_sub(1)
    };
    for (index, line) in lines.iter().enumerate() {
        let line = line.trim();
        if !line.starts_with('{') {
            continue;
        }
        let whole = index < complete;
        let Some(event) = event(line, whole) else {
            continue;
        };
        apply(&mut draft, &event, whole);
    }
    draft
}

/// One JSON line, or the start of one whose end has not arrived.
fn event(line: &str, whole: bool) -> Option<serde_json::Value> {
    if let Ok(value) = serde_json::from_str(line) {
        return Some(value);
    }
    if whole {
        return None;
    }
    let open = line.trim_end_matches('\\');
    ["\"}", "\"]}", "]}", "}"]
        .iter()
        .find_map(|end| serde_json::from_str(&format!("{open}{end}")).ok())
}

fn apply(draft: &mut Draft, event: &serde_json::Value, whole: bool) {
    let cut = |text: &str| {
        if whole {
            text.trim().to_string()
        } else {
            whole_words(text)
        }
    };
    if let Some(title) = event["title"].as_str() {
        draft.title = whole.then(|| title.trim().to_string());
    }
    if let Some(labels) = event["labels"].as_array() {
        draft.labels = labels
            .iter()
            .filter_map(|label| label.as_str())
            .map(|label| label.trim().trim_matches(['[', ']']).to_string())
            .filter(|label| !label.is_empty())
            .collect();
    }
    if let Some(summary) = event["summary"].as_str() {
        draft.summary = cut(summary);
    }
    if let Some(heading) = event["section"].as_str() {
        let text = cut(event["text"].as_str().unwrap_or_default());
        draft.sections.push(Section {
            heading: heading.trim().to_string(),
            text,
        });
        draft.writing = (!whole).then(|| draft.sections.len() - 1);
    }
}

fn parse_markdown(text: &str, done: bool) -> Draft {
    let mut draft = Draft::default();
    let text = if done {
        text.to_string()
    } else {
        whole_words(text)
    };
    let mut current: Option<Section> = None;
    let mut summary = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    let title = markdown::title_line(&lines);
    let prose = markdown::prose(&lines);
    for (index, line) in lines.into_iter().enumerate() {
        if Some(index) == title {
            let (labels, title) = split_labels(markdown::heading_text(line));
            draft.labels = labels;
            draft.title = Some(title);
        } else if let Some(heading) = line.strip_prefix("## ").filter(|_| prose[index]) {
            if let Some(section) = current.take() {
                draft.sections.push(finish(section));
            }
            current = Some(Section {
                heading: heading.trim().to_string(),
                text: String::new(),
            });
        } else if let Some(section) = current.as_mut() {
            section.text.push_str(line);
            section.text.push('\n');
        } else {
            summary.push(line);
        }
    }
    draft.summary = summary.join("\n").trim().to_string();
    if let Some(section) = current {
        draft.sections.push(finish(section));
        draft.writing = (!done).then(|| draft.sections.len() - 1);
    }
    draft
}

fn finish(mut section: Section) -> Section {
    section.text = section.text.trim().to_string();
    section
}

/// `[Bug] [ux] Fix login` → (`["Bug", "ux"]`, `Fix login`), the labels as
/// typed. A group that is no label (`[]`) ends the run and stays in the title.
pub fn split_labels(title: &str) -> (Vec<String>, String) {
    let (groups, rest) = labels::bracket_groups(title);
    let found = groups
        .iter()
        .map(|group| group.trim().to_string())
        .collect();
    (found, rest.trim().to_string())
}

/// Nothing typed in the title, its tags or the notes.
pub fn is_blank(title_labels: &[String], title: &str, notes: &str) -> bool {
    title.trim().is_empty() && title_labels.is_empty() && notes.trim().is_empty()
}

/// The title as the file has it: the tags in brackets, then the field.
pub fn written_title(title_labels: &[String], title: &str) -> String {
    labelled_title(title_labels, title).trim_end().to_string()
}

/// After typing: `[bug]` ending at `cursor`, at the start of the field,
/// becomes a tag, and a space right after a tag is swallowed. Returns the
/// cursor's new place when the field changed.
pub fn tag_typed_label(
    title_labels: &mut Vec<String>,
    title: &mut String,
    cursor: usize,
) -> Option<usize> {
    if cursor == 1 && !title_labels.is_empty() && title.starts_with(' ') {
        title.remove(0);
        return Some(0);
    }
    let head: String = title.chars().take(cursor).collect();
    let inner = head.strip_prefix('[')?.strip_suffix(']')?;
    if inner.contains(['[', ']']) {
        return None;
    }
    labels::normalize(inner)?;
    title_labels.push(inner.to_string());
    *title = title[head.len()..].trim_start().to_string();
    Some(0)
}

/// `⌫` right after a tag: the last tag is text again. Returns the cursor's
/// new place, after the `]`.
pub fn untag_label(title_labels: &mut Vec<String>, title: &mut String) -> Option<usize> {
    let text = format!("[{}]", title_labels.pop()?);
    let cursor = text.chars().count();
    *title = if title.is_empty() {
        text
    } else {
        format!("{text} {title}")
    };
    Some(cursor)
}

/// `text` up to its last whole word, so lines never jitter as characters
/// arrive.
fn whole_words(text: &str) -> String {
    match text.rfind(char::is_whitespace) {
        Some(at) => text[..at].trim_end().to_string(),
        None => String::new(),
    }
}

/// The text of the `# ` line: the labels in brackets, then the title.
fn labelled_title(labels: &[String], title: &str) -> String {
    let mut line = String::new();
    for label in labels {
        line.push_str(&format!("[{label}] "));
    }
    line.push_str(title.trim());
    line
}

/// What `↩` writes: `# <title>` and the notes without a draft; with one, the
/// labelled title, the summary, each written section and, when `keep_notes`,
/// the notes verbatim under `## Notes`.
pub fn render(title: &str, notes: &str, draft: Option<&Draft>, keep_notes: bool) -> String {
    let Some(draft) = draft else {
        let notes = notes.trim_end();
        return if notes.trim().is_empty() {
            format!("# {}\n", title.trim())
        } else {
            format!("# {}\n\n{notes}\n", title.trim())
        };
    };
    let mut out = format!("# {}\n", labelled_title(&draft.labels, title));
    if !draft.summary.trim().is_empty() {
        out.push_str(&format!("\n{}\n", draft.summary.trim()));
    }
    for section in &draft.sections {
        if !section.text.trim().is_empty() {
            out.push_str(&format!(
                "\n## {}\n\n{}\n",
                section.heading,
                section.text.trim()
            ));
        }
    }
    if keep_notes && !notes.trim().is_empty() {
        out.push_str(&format!("\n## Notes\n\n{}\n", notes.trim_end()));
    }
    out
}

/// A fingerprint of the notes a draft was made from, to tell when they
/// changed since.
pub fn notes_hash(notes: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    notes.trim().hash(&mut hasher);
    hasher.finish()
}

/// Where the notes of a closed modal wait for the next `⌘N`.
pub fn park_path(project: &Path) -> PathBuf {
    project.join(".iboard").join("new-issue.md")
}

/// The notes, title and draft parked on close.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Parked {
    pub title: String,
    pub notes: String,
    pub draft: Option<Draft>,
}

const NOTES_MARK: &str = "<!-- iboard: notes -->";
const DRAFT_MARK: &str = "<!-- iboard: draft -->";

impl Parked {
    fn to_text(&self) -> String {
        let mut out = format!("# {}\n\n{NOTES_MARK}\n{}\n", self.title.trim(), self.notes);
        if let Some(draft) = &self.draft {
            out.push_str(&format!(
                "{DRAFT_MARK}\n{}",
                render(self.title.trim(), "", Some(draft), false)
            ));
        }
        out
    }

    fn from_text(text: &str) -> Option<Self> {
        let (head, rest) = text.split_once(NOTES_MARK)?;
        let title = head
            .lines()
            .next()
            .map_or("", markdown::heading_text)
            .to_string();
        let (notes, draft) = match rest.split_once(DRAFT_MARK) {
            Some((notes, draft)) => (notes, Some(parse(draft, true))),
            None => (rest, None),
        };
        let notes = notes.strip_prefix('\n').unwrap_or(notes);
        let notes = notes.strip_suffix('\n').unwrap_or(notes).to_string();
        Some(Self {
            title,
            notes,
            draft,
        })
    }

    /// Writes it to `path`, or removes the file when there is nothing to keep.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if self.notes.trim().is_empty() {
            return clear(path);
        }
        crate::files::write_creating(path, self.to_text()).map_err(Into::into)
    }

    pub fn load(path: &Path) -> Option<Self> {
        Self::from_text(&std::fs::read_to_string(path).ok()?)
            .filter(|parked| !parked.notes.trim().is_empty())
    }
}

/// Removes the parked notes, if any.
pub fn clear(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => Err(err),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_lines_fill_the_draft_in_order() {
        let text = "{\"title\":\"Export the board\"}\n{\"labels\":[\"feat\"]}\n\
                    {\"summary\":\"One checklist per column.\"}\n\
                    {\"section\":\"Context\",\"text\":\"People print boards.\"}\n";
        let draft = parse(text, false);
        assert_eq!(draft.title.as_deref(), Some("Export the board"));
        assert_eq!(draft.labels, ["feat"]);
        assert_eq!(draft.summary, "One checklist per column.");
        assert_eq!(draft.sections[0].text, "People print boards.");
        assert_eq!(draft.writing, None, "a whole line is a finished section");
    }

    #[test]
    fn a_cut_json_line_shows_its_whole_words_and_is_being_written() {
        let text = "{\"title\":\"Export\"}\n{\"section\":\"Context\",\"text\":\"People print bo";
        let draft = parse(text, false);
        assert_eq!(draft.sections[0].text, "People print");
        assert_eq!(draft.writing, Some(0));
    }

    #[test]
    fn plain_markdown_splits_on_its_headings() {
        let text = "# [Bug] Fix login\n\nThe redirect loops.\n\n## Context\n\nSince 2.1.\n\n## Acceptance criteria\n\n- It red";
        let draft = parse(text, false);
        assert_eq!(draft.title.as_deref(), Some("Fix login"));
        assert_eq!(draft.labels, ["Bug"]);
        assert_eq!(draft.summary, "The redirect loops.");
        assert_eq!(draft.sections.len(), 2);
        assert_eq!(draft.sections[1].text, "- It");
        assert_eq!(draft.writing, Some(1));
        assert_eq!(parse(text, true).sections[1].text, "- It red");
    }

    #[test]
    fn only_label_brackets_leave_the_title() {
        let split = |title: &str| split_labels(title);
        assert_eq!(
            split("[Bug] [ux] Fix"),
            (vec!["Bug".into(), "ux".into()], "Fix".into())
        );
        assert_eq!(split("[bug] [] Fix"), (vec!["bug".into()], "[] Fix".into()));
        assert_eq!(split("[docs](url) Fix"), (vec![], "[docs](url) Fix".into()));
    }

    #[test]
    fn a_typed_label_becomes_a_tag_and_back() {
        let (mut tags, mut title) = (Vec::new(), "[bug]Fix".to_string());
        assert_eq!(tag_typed_label(&mut tags, &mut title, 5), Some(0));
        assert_eq!(
            (tags.as_slice(), title.as_str()),
            (["bug".to_string()].as_slice(), "Fix")
        );
        assert_eq!(written_title(&tags, &title), "[bug] Fix");
        assert_eq!(untag_label(&mut tags, &mut title), Some(5));
        assert_eq!(title, "[bug] Fix");
        assert!(is_blank(&tags, " ", ""));
    }

    #[test]
    fn without_a_draft_the_notes_are_written_as_typed() {
        assert_eq!(
            render("Fix", "line one\n", None, true),
            "# Fix\n\nline one\n"
        );
        assert_eq!(render("Fix", "  ", None, true), "# Fix\n");
    }

    #[test]
    fn a_draft_is_written_with_its_labels_sections_and_kept_notes() {
        let draft = Draft {
            title: Some("Fix login".into()),
            labels: vec!["bug".into()],
            summary: "It loops.".into(),
            sections: vec![
                Section {
                    heading: "Context".into(),
                    text: "Since 2.1.".into(),
                },
                Section {
                    heading: "Out of scope".into(),
                    text: String::new(),
                },
            ],
            writing: None,
        };
        assert_eq!(
            render("Fix login", "my notes", Some(&draft), true),
            "# [bug] Fix login\n\nIt loops.\n\n## Context\n\nSince 2.1.\n\n## Notes\n\nmy notes\n"
        );
        assert_eq!(
            draft.unwritten(&["Context".into(), "Out of scope".into()]),
            ["Out of scope"]
        );
    }

    #[test]
    fn parked_notes_and_draft_come_back() {
        let scratch = tempfile::tempdir().unwrap();
        let path = park_path(scratch.path());
        let parked = Parked {
            title: "Fix login".into(),
            notes: "line one\n\nline two".into(),
            draft: Some(parse(
                "# [bug] Fix login\n\nIt loops.\n\n## Context\n\nSince 2.1.\n",
                true,
            )),
        };
        parked.save(&path).unwrap();
        assert_eq!(Parked::load(&path), Some(parked));
        Parked::default().save(&path).unwrap();
        assert!(!path.exists(), "empty notes park nothing");
    }

    #[test]
    fn the_draft_title_is_the_heading_the_board_reads() {
        let draft = parse(
            "```sh\n# not this\n## Nor this\n```\n  # [Bug] Fix\n\n# Later\n",
            true,
        );
        assert_eq!(draft.title.as_deref(), Some("Fix"));
        assert_eq!(draft.labels, ["Bug"]);
        assert!(draft.sections.is_empty());
        let parked = Parked::from_text("# # Fix\n\n<!-- iboard: notes -->\nn\n").unwrap();
        assert_eq!(parked.title, "# Fix");
    }
}
