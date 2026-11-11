//! The note a release writes: its sections, the opening an agent may write,
//! and the labels the commit subjects are read through.

use std::collections::HashMap;

/// The section a commit subject's kind label names, in the order the note
/// prints them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    New,
    Fixed,
    Technical,
}

impl Kind {
    /// The `### ` heading the note prints for `self`.
    pub fn heading(self) -> &'static str {
        match self {
            Self::New => "New",
            Self::Fixed => "Fixed",
            Self::Technical => "Technical",
        }
    }

    /// The kind a subject's first label names; `None` for a label the note
    /// does not know, which leaves the commit out of the note.
    fn of(label: &str) -> Option<Self> {
        match label {
            "feat" => Some(Self::New),
            "bug" => Some(Self::Fixed),
            "chore" | "refactor" | "perf" | "discovery" => Some(Self::Technical),
            _ => None,
        }
    }
}

/// One subject read as the note prints it: its kind, the domain of its second
/// label, and what it says once both labels are gone.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    kind: Kind,
    domain: Option<String>,
    text: String,
}

/// The `[label]` a subject opens with, and what follows it.
fn take_label(subject: &str) -> Option<(&str, &str)> {
    let rest = subject.trim_start().strip_prefix('[')?;
    let end = rest.find(']')?;
    let label = &rest[..end];
    let label = (!label.is_empty() && label.chars().all(|c| c.is_ascii_lowercase() || c == '-'))
        .then_some(label)?;
    Some((label, rest[end + 1..].trim_start()))
}

/// The subject as an [`Entry`]: its first label is the kind, its second the
/// domain, and any label after them says nothing the note prints. A subject
/// with no known kind, or nothing left to say, is `None`.
fn entry(subject: &str) -> Option<Entry> {
    let (kind, rest) = take_label(subject)?;
    let kind = Kind::of(kind)?;
    let (domain, text) = match take_label(rest) {
        Some((domain, text)) => (Some(domain.to_string()), text),
        None => (None, rest),
    };
    let text = without_labels(text);
    (!text.is_empty()).then(|| Entry {
        kind,
        domain,
        text: text.to_string(),
    })
}

/// `text` with every `[label]` it opens with taken off.
fn without_labels(mut text: &str) -> &str {
    while let Some((_, rest)) = take_label(text) {
        text = rest;
    }
    text
}

/// The note a release writes from `subjects`: one `### ` section per kind,
/// each grouped by domain, most used domain first.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Note {
    pub body: String,
    /// The subjects the note leaves out: no known kind label.
    pub unclassified: usize,
    /// The technical subjects, printed only when `technical` is asked.
    pub technical: usize,
    /// The `feat` subjects: what the note's version bump follows.
    pub features: usize,
}

/// The subjects as [`Note`], its technical section printed when `technical`.
pub fn note(subjects: &[String], technical: bool) -> Note {
    let entries: Vec<Entry> = subjects.iter().filter_map(|s| entry(s)).collect();
    let mut sections = Vec::new();
    for kind in [Kind::New, Kind::Fixed, Kind::Technical] {
        if kind == Kind::Technical && !technical {
            continue;
        }
        let section = section_of(kind, &entries);
        if !section.is_empty() {
            sections.push(section);
        }
    }
    Note {
        body: sections.join("\n"),
        unclassified: subjects.len() - entries.len(),
        technical: entries.iter().filter(|e| e.kind == Kind::Technical).count(),
        features: entries.iter().filter(|e| e.kind == Kind::New).count(),
    }
}

/// Whether the window holds a `feat`: what the version bump follows.
pub(super) fn has_feature(subjects: &[String]) -> bool {
    subjects
        .iter()
        .filter_map(|subject| entry(subject))
        .any(|e| e.kind == Kind::New)
}

/// The instruction for the agent that writes the note's opening sentences,
/// from the note's own lines and nothing else.
pub fn preamble_prompt(body: &str) -> String {
    format!(
        "You write the opening of a software release note, from the note's own lines below. Say \
         what the release brings in one or two sentences, at most forty words, naming the changes \
         that matter most. Use only what the lines say and invent nothing: no version number, no \
         date, no promise. Answer with the sentences alone, no heading, no list, no markdown.\n\
         \n\
         ---\n\
         {}",
        body.trim()
    )
}

/// The opening a note takes from `text`: its first paragraph, unfolded onto one
/// line, without the quotes or markers a model wraps an answer in.
pub fn opening(text: &str) -> String {
    let paragraph = text
        .split("\n\n")
        .map(str::trim)
        .find(|paragraph| !paragraph.is_empty())
        .unwrap_or_default();
    let line = paragraph
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    line.trim_matches(|c: char| matches!(c, '"' | '\'' | '`' | '*' | '#' | ' '))
        .trim()
        .to_string()
}

/// The `### ` section of `kind`, empty when no entry has it. The added ones
/// are split by domain under a `#### ` heading; fixed and technical stay flat.
fn section_of(kind: Kind, entries: &[Entry]) -> String {
    let mine: Vec<&Entry> = entries.iter().filter(|e| e.kind == kind).collect();
    if mine.is_empty() {
        return String::new();
    }
    let mut section = format!("### {}\n", kind.heading());
    if kind != Kind::New {
        for entry in &mine {
            section.push_str(&line(entry));
        }
        return section;
    }
    for entry in mine.iter().filter(|e| e.domain.is_none()) {
        section.push_str(&line(entry));
    }
    for (domain, group) in by_domain(&mine) {
        section.push_str(&format!("#### {}\n", capitalised(domain)));
        for entry in group {
            section.push_str(&line(entry));
        }
    }
    section
}

/// `entries` grouped by domain, the most used domain first then by name; the
/// ones with no domain are not part of a group.
fn by_domain<'a>(entries: &[&'a Entry]) -> Vec<(&'a str, Vec<&'a Entry>)> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for entry in entries {
        if let Some(domain) = &entry.domain {
            *counts.entry(domain.as_str()).or_default() += 1;
        }
    }
    let mut domains: Vec<(&str, usize)> = counts.into_iter().collect();
    domains.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    domains
        .into_iter()
        .map(|(domain, _)| {
            let group = entries
                .iter()
                .filter(|entry| entry.domain.as_deref() == Some(domain))
                .copied()
                .collect();
            (domain, group)
        })
        .collect()
}

/// One `- text` line.
fn line(entry: &Entry) -> String {
    format!("- {}\n", entry.text)
}

/// `board` as `Board`; anything else stays as it is.
fn capitalised(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_note_groups_the_subjects_by_kind_then_domain() {
        let subjects = [
            "[feat] [board] Save the panel".to_string(),
            "[feat] [agent] Plan mode".to_string(),
            "[bug] [board] The filter".to_string(),
            "[feat] [board] The bell".to_string(),
        ];
        let note = note(&subjects, false);
        assert_eq!(
            note.body,
            "### New\n\
             #### Board\n\
             - Save the panel\n\
             - The bell\n\
             #### Agent\n\
             - Plan mode\n\
             \n\
             ### Fixed\n\
             - The filter\n"
        );
        assert_eq!(note.features, 3);
        assert_eq!(note.technical, 0);
        assert_eq!(note.unclassified, 0);
    }

    #[test]
    fn a_subject_without_a_known_kind_stays_out_and_is_counted() {
        let subjects = [
            "Some chore".to_string(),
            "[ux] Column actions name themselves".to_string(),
            "[feat] [board] Save the panel".to_string(),
        ];
        let note = note(&subjects, false);
        assert_eq!(note.body, "### New\n#### Board\n- Save the panel\n");
        assert_eq!(note.unclassified, 2, "no kind, or a topic where it belongs");
    }

    #[test]
    fn the_technical_section_comes_last_and_only_when_asked() {
        let subjects = [
            "[refactor] [board] Split the module".to_string(),
            "[feat] [board] Save the panel".to_string(),
        ];
        let quiet = note(&subjects, false);
        assert_eq!(quiet.technical, 1);
        assert!(!quiet.body.contains("Technical"), "{}", quiet.body);
        let loud = note(&subjects, true);
        assert_eq!(
            loud.body,
            "### New\n#### Board\n- Save the panel\n\n\
             ### Technical\n- Split the module\n"
        );
    }

    #[test]
    fn a_subject_without_a_domain_lists_plain_before_the_domains() {
        let subjects = [
            "[feat] Ship it".to_string(),
            "[feat] [board] Save the panel".to_string(),
        ];
        assert_eq!(
            note(&subjects, false).body,
            "### New\n- Ship it\n#### Board\n- Save the panel\n"
        );
    }

    #[test]
    fn a_label_after_the_domain_says_nothing_the_note_prints() {
        let subjects = ["[feat] [board] [ux] Save the panel".to_string()];
        assert_eq!(
            note(&subjects, false).body,
            "### New\n#### Board\n- Save the panel\n"
        );
    }

    #[test]
    fn an_opening_is_the_first_paragraph_without_its_wrappers() {
        assert_eq!(
            opening("\"A flag arrives.\"\n\nIf you want more, say so."),
            "A flag arrives."
        );
        assert_eq!(opening("\n\n`Two lines\nin one.`\n"), "Two lines in one.");
        assert_eq!(opening("   "), "");
    }
}
