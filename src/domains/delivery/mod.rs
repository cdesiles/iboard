//! What Done's Release flag writes: the next tag, the release note that goes
//! into the project's changelog, and the git commands that commit, tag and
//! push it. The modal's choices are kept in the user configuration.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use crate::platform::git;

/// The note file at the project root, newest release first.
pub const NOTE_FILE: &str = "CHANGELOG.md";
/// The note file names a project may already use, lower case.
const NOTE_FILES: [&str; 2] = ["changelog.md", "release-notes.md"];
const SECTION: &str = "release";
const FIRST_TAG: &str = "v0.1.0";

/// What the release modal opens on: whether to write a note, in which file,
/// and whether to push afterwards. Kept in `[release]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseChoices {
    pub note: bool,
    pub file: String,
    pub push: bool,
}

impl Default for ReleaseChoices {
    fn default() -> Self {
        Self {
            note: true,
            file: NOTE_FILE.to_string(),
            push: true,
        }
    }
}

/// The `[release]` section of the user configuration.
#[derive(Default, Deserialize, Serialize)]
pub struct ReleaseSection {
    note: Option<bool>,
    file: Option<String>,
    push: Option<bool>,
}

impl ReleaseSection {
    /// The choices it names; an empty file name is reported and keeps the
    /// default.
    pub fn choices(self) -> (ReleaseChoices, Vec<String>) {
        let defaults = ReleaseChoices::default();
        let file = match self.file {
            None => defaults.file,
            Some(file) if file.trim().is_empty() => {
                return (
                    ReleaseChoices {
                        note: self.note.unwrap_or(defaults.note),
                        file: defaults.file,
                        push: self.push.unwrap_or(defaults.push),
                    },
                    vec![format!("[{SECTION}]: file is empty")],
                );
            }
            Some(file) => file.trim().to_string(),
        };
        (
            ReleaseChoices {
                note: self.note.unwrap_or(defaults.note),
                file,
                push: self.push.unwrap_or(defaults.push),
            },
            Vec::new(),
        )
    }
}

impl ReleaseChoices {
    /// The `[release]` section that stores them.
    pub fn section(&self) -> ReleaseSection {
        ReleaseSection {
            note: Some(self.note),
            file: Some(self.file.clone()),
            push: Some(self.push),
        }
    }
}

/// The note file the modal opens on: `preferred` when the project has it,
/// else the `CHANGELOG.md` or `release-notes.md` it already uses, else
/// `preferred`.
pub fn note_file(project: &Path, preferred: &str) -> String {
    let preferred = preferred.trim();
    if project.join(preferred).is_file() {
        return preferred.to_string();
    }
    used_note_file(project).unwrap_or_else(|| preferred.to_string())
}

fn used_note_file(project: &Path) -> Option<String> {
    let entries = std::fs::read_dir(project).ok()?;
    entries.filter_map(Result::ok).find_map(|entry| {
        let name = entry.file_name().to_string_lossy().into_owned();
        NOTE_FILES
            .contains(&name.to_lowercase().as_str())
            .then_some(name)
    })
}

/// The tag after the newest one matching `pattern`, `bump` applied: `v0.3.0`
/// gives `v0.3.1`, `v0.4.0` or `v1.0.0`. No matching version gives
/// [`FIRST_TAG`].
pub fn next_tag(dir: &Path, pattern: &str, bump: Bump) -> String {
    newest_tag(dir, pattern)
        .and_then(|tag| bumped(&tag, bump))
        .unwrap_or_else(|| FIRST_TAG.to_string())
}

/// Which part of the version a release moves, and so what its number says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bump {
    Patch,
    Minor,
    Major,
}

impl Bump {
    /// What the subjects bump: a `feat` in the window makes a minor, anything
    /// less a patch.
    pub fn of(subjects: &[String]) -> Self {
        match subjects
            .iter()
            .filter_map(|s| entry(s))
            .any(|e| e.kind == Kind::Added)
        {
            true => Self::Minor,
            false => Self::Patch,
        }
    }
}

fn newest_tag(dir: &Path, pattern: &str) -> Option<String> {
    let pattern = match pattern.trim() {
        "" => "*",
        pattern => pattern,
    };
    let out = git::output(dir, &["tag", "--sort=-v:refname", "--list", pattern]).ok()?;
    out.lines()
        .map(str::trim)
        .find(|tag| !tag.is_empty())
        .map(str::to_string)
}

/// The newest tag matching `pattern`, and when it was made: where the
/// release window of the next version starts.
pub fn last_release(dir: &Path, pattern: &str) -> Option<(String, DateTime<Local>)> {
    let tag = newest_tag(dir, pattern)?;
    let out = git::output(
        dir,
        &[
            "for-each-ref",
            "--format=%(creatordate:iso-strict)",
            &format!("refs/tags/{tag}"),
        ],
    )
    .ok()?;
    let date = DateTime::parse_from_rfc3339(out.trim())
        .ok()?
        .with_timezone(&Local);
    Some((tag, date))
}

/// Per `(folder, pattern)`: the `HEAD` it was read at, whether a commit waits
/// for release, and when it was read.
type Cached = HashMap<(PathBuf, String), (String, bool, Instant)>;
static CACHE: Mutex<Option<Cached>> = Mutex::new(None);

/// How long a lookup is trusted. The Done header asks every frame, so without
/// this each frame would spawn `git`.
const TTL: Duration = Duration::from_secs(2);

/// Whether `dir` has at least one commit since the newest tag matching
/// `pattern`. An untagged repository counts from its first commit.
pub fn has_commits_since_last_release(dir: &Path, pattern: &str) -> bool {
    let head = git::output(dir, &["rev-parse", "HEAD"])
        .map(|out| out.trim().to_string())
        .unwrap_or_default();
    let key = (dir.to_path_buf(), pattern.to_string());
    {
        let cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((cached_head, unreleased, at)) =
            cache.as_ref().and_then(|cache| cache.get(&key))
            && *cached_head == head
            && at.elapsed() < TTL
        {
            return *unreleased;
        }
    }
    let unreleased = !head.is_empty() && commits_since(dir, newest_tag(dir, pattern).as_deref());
    CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashMap::new)
        .insert(key, (head, unreleased, Instant::now()));
    unreleased
}

fn commits_since(dir: &Path, tag: Option<&str>) -> bool {
    let range = match tag {
        Some(tag) => format!("{tag}..HEAD"),
        None => "HEAD".to_string(),
    };
    git::output(dir, &["rev-list", "--count", &range])
        .ok()
        .and_then(|out| out.trim().parse::<u64>().ok())
        .is_some_and(|count| count > 0)
}

/// `v1.2.3` as `v1.2.4`, `v1.3.0` or `v2.0.0`, keeping the prefix before its
/// first digit.
fn bumped(tag: &str, bump: Bump) -> Option<String> {
    let start = tag.find(|c: char| c.is_ascii_digit())?;
    let (prefix, version) = tag.split_at(start);
    let core = version.split(['-', '+']).next().unwrap_or_default();
    let parts: Option<Vec<u64>> = core.split('.').map(|part| part.parse().ok()).collect();
    let mut parts = parts?;
    if parts.len() > 3 {
        return None;
    }
    parts.resize(3, 0);
    let moved = match bump {
        Bump::Patch => [parts[0], parts[1], parts[2] + 1],
        Bump::Minor => [parts[0], parts[1] + 1, 0],
        Bump::Major => [parts[0] + 1, 0, 0],
    };
    Some(format!("{prefix}{}.{}.{}", moved[0], moved[1], moved[2]))
}

/// Whether `tag` already names a tag in the repository.
pub fn tag_exists(project: &Path, tag: &str) -> bool {
    git::output(project, &["tag", "--list", tag])
        .map(|out| out.lines().any(|line| line.trim() == tag))
        .unwrap_or(false)
}

/// How far a release got: the note committed, the tag made, the push done.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReleaseProgress {
    pub committed: bool,
    pub tagged: bool,
    pub pushed: bool,
}

impl ReleaseProgress {
    /// Whether the release is made, its push included when one was asked.
    pub fn finished(&self, push: bool) -> bool {
        self.tagged && (self.pushed || !push)
    }
}

/// Whether `tag` is a name git accepts.
pub fn check_tag(tag: &str) -> Result<(), String> {
    let tag = tag.trim();
    if tag.is_empty() {
        return Err("The tag can't be empty.".to_string());
    }
    if tag.starts_with('-') || tag.contains(char::is_whitespace) {
        return Err(format!("`{tag}` is not a tag git accepts."));
    }
    Ok(())
}

/// Whether `file` stays inside the project.
pub fn check_file(file: &str) -> Result<(), String> {
    let file = file.trim();
    if file.is_empty() {
        return Err("The release note file can't be empty.".to_string());
    }
    let path = Path::new(file);
    if path.is_absolute()
        || path
            .components()
            .any(|part| part == std::path::Component::ParentDir)
    {
        return Err("The release note file must stay inside the project.".to_string());
    }
    Ok(())
}

/// Makes the release in order: the note and its commit, the tag, the push.
/// An error stops it where it is, and a later confirm resumes from there.
pub fn confirm(
    project: &Path,
    name: &str,
    note: &str,
    file: &str,
    update_note: bool,
    push_after: bool,
    progress: &mut ReleaseProgress,
) -> Result<(), String> {
    let version = name.trim();
    let file = file.trim();
    if !progress.tagged {
        check_tag(version)?;
        if tag_exists(project, version) {
            return Err(format!("{version} is already a tag."));
        }
    }
    if update_note && !progress.committed {
        check_file(file)?;
        let path = write_note(project, file, note)?;
        let message = format!("Release {version}");
        commit(project, Some(&path), &message)?;
        progress.committed = true;
    }
    if !progress.tagged {
        tag(project, version, note)?;
        progress.tagged = true;
    }
    if push_after && !progress.pushed {
        push(project, true)?;
        progress.pushed = true;
    }
    Ok(())
}

/// The subjects of the commits since the newest tag matching `pattern`, oldest
/// first: the list the release note is written from. An untagged repository
/// lists every commit.
pub fn commit_subjects(dir: &Path, pattern: &str) -> Vec<String> {
    let range = match newest_tag(dir, pattern) {
        Some(tag) => format!("{tag}..HEAD"),
        None => "HEAD".to_string(),
    };
    let Ok(out) = git::output(dir, &["log", "--reverse", "--format=%s", &range]) else {
        return Vec::new();
    };
    out.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

/// The section a commit subject's kind label names, in the order the note
/// prints them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Added,
    Fixed,
    Technical,
}

impl Kind {
    /// The `### ` heading the note prints for `self`.
    pub fn heading(self) -> &'static str {
        match self {
            Self::Added => "Added",
            Self::Fixed => "Fixed",
            Self::Technical => "Technical",
        }
    }

    /// The kind a subject's first label names; `None` for a label the note
    /// does not know, which leaves the commit out of the note.
    fn of(label: &str) -> Option<Self> {
        match label {
            "feat" => Some(Self::Added),
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
    for kind in [Kind::Added, Kind::Fixed, Kind::Technical] {
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
        features: entries.iter().filter(|e| e.kind == Kind::Added).count(),
    }
}

/// The `### ` section of `kind`, empty when no entry has it.
fn section_of(kind: Kind, entries: &[Entry]) -> String {
    let mine: Vec<&Entry> = entries.iter().filter(|e| e.kind == kind).collect();
    if mine.is_empty() {
        return String::new();
    }
    let mut section = format!("### {}\n", kind.heading());
    for entry in by_domain(&mine) {
        section.push_str(&line(entry));
        section.push('\n');
    }
    section
}

/// `entries` grouped by domain, the most used domain first then by name; the
/// ones with no domain last, in commit order.
fn by_domain<'a>(entries: &[&'a Entry]) -> Vec<&'a Entry> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for entry in entries {
        if let Some(domain) = &entry.domain {
            *counts.entry(domain.as_str()).or_default() += 1;
        }
    }
    let mut domains: Vec<(&str, usize)> = counts.into_iter().collect();
    domains.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    let mut ordered = Vec::with_capacity(entries.len());
    for (domain, _) in domains {
        ordered.extend(
            entries
                .iter()
                .filter(|entry| entry.domain.as_deref() == Some(domain))
                .copied(),
        );
    }
    ordered.extend(entries.iter().copied().filter(|e| e.domain.is_none()));
    ordered
}

/// One `- **Domain** — text` line, or `- text` without a domain.
fn line(entry: &Entry) -> String {
    match &entry.domain {
        Some(domain) => format!("- **{}** — {}", capitalised(domain), entry.text),
        None => format!("- {}", entry.text),
    }
}

/// `board` as `Board`; anything else stays as it is.
fn capitalised(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// A release section: `## v0.3.1 — 7 Nov 2026`, then `body`.
pub fn section(tag: &str, day: &str, body: &str) -> String {
    let body = body.trim();
    if body.is_empty() {
        format!("## {tag} — {day}\n")
    } else {
        format!("## {tag} — {day}\n\n{body}\n")
    }
}

/// `section` put at the top of a changelog, under one `# Changelog` heading.
pub fn with_section(existing: Option<&str>, section: &str) -> String {
    let section = section.trim_end();
    let Some(existing) = existing.map(str::trim_end).filter(|text| !text.is_empty()) else {
        return format!("# Changelog\n\n{section}\n");
    };
    match existing.strip_prefix("# Changelog") {
        Some(rest) => {
            let rest = rest.trim_start_matches(['\r', '\n']);
            match rest.is_empty() {
                true => format!("# Changelog\n\n{section}\n"),
                false => format!("# Changelog\n\n{section}\n\n{rest}\n"),
            }
        }
        None => format!("{section}\n\n{existing}\n"),
    }
}

/// Writes `section` to `file` in the project, newest first, and says where it
/// went.
pub fn write_note(project: &Path, file: &str, section: &str) -> Result<PathBuf, String> {
    let path = project.join(file.trim());
    let existing = std::fs::read_to_string(&path).ok();
    let text = with_section(existing.as_deref(), section);
    crate::platform::files::atomic_write(&path, text)
        .map_err(|err| format!("Couldn't write {}: {err}", path.display()))?;
    Ok(path)
}

/// Commits `note` with `message`, or an empty commit without one.
pub fn commit(project: &Path, note: Option<&Path>, message: &str) -> Result<(), String> {
    match note.and_then(|note| note.strip_prefix(project).ok()) {
        Some(note) => {
            let note = note.to_string_lossy();
            git::output(project, &["add", "--", &note])?;
            git::output(project, &["commit", "-m", message])?;
        }
        None => {
            git::output(project, &["commit", "--allow-empty", "-m", message])?;
        }
    }
    Ok(())
}

/// Tags `HEAD` with `tag`, its message the release note.
pub fn tag(project: &Path, tag: &str, note: &str) -> Result<(), String> {
    git::output(project, &["tag", "-a", tag, "-m", note])?;
    Ok(())
}

/// Pushes the branch, with its annotated tags when it has one.
pub fn push(project: &Path, with_tags: bool) -> Result<(), String> {
    match with_tags {
        true => git::output(project, &["push", "--follow-tags"]),
        false => git::output(project, &["push"]),
    }?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scratch_git::{git, git_env};

    fn repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-q", "-b", "main"]);
        git(
            dir.path(),
            &["commit", "-q", "--allow-empty", "-m", "first"],
        );
        dir
    }

    #[test]
    fn the_next_tag_moves_the_part_the_bump_names() {
        let dir = repo();
        let dir = dir.path();
        for tag in ["v0.2.9", "v0.3.0", "rel-2.0.0", "nightly"] {
            git(dir, &["tag", tag]);
        }
        assert_eq!(next_tag(dir, "v*", Bump::Patch), "v0.3.1");
        assert_eq!(next_tag(dir, "v*", Bump::Minor), "v0.4.0");
        assert_eq!(next_tag(dir, "v*", Bump::Major), "v1.0.0");
        assert_eq!(next_tag(dir, "rel-*", Bump::Patch), "rel-2.0.1");
        assert_eq!(next_tag(dir, "rel-*", Bump::Minor), "rel-2.1.0");
        assert_eq!(
            next_tag(dir, "", Bump::Patch),
            "v0.3.1",
            "every tag matches"
        );
        assert_eq!(next_tag(dir, "nightly*", Bump::Patch), FIRST_TAG);
        assert!(tag_exists(dir, "v0.3.0"));
        assert!(!tag_exists(dir, "v0.3.1"));
    }

    #[test]
    fn a_short_version_is_read_as_a_full_one() {
        let dir = repo();
        let dir = dir.path();
        git(dir, &["tag", "rel-1.2"]);
        assert_eq!(next_tag(dir, "rel-*", Bump::Patch), "rel-1.2.1");
        assert_eq!(next_tag(dir, "rel-*", Bump::Minor), "rel-1.3.0");
    }

    #[test]
    fn the_bump_follows_the_features_in_the_window() {
        let without = ["[bug] [board] Fix the box".to_string()];
        let with = [
            "[bug] [board] Fix the box".to_string(),
            "[feat] [board] Add a flag".to_string(),
        ];
        assert_eq!(Bump::of(&without), Bump::Patch);
        assert_eq!(Bump::of(&with), Bump::Minor);
        assert_eq!(
            Bump::of(&["Fix the box".to_string()]),
            Bump::Patch,
            "an unclassified subject bumps nothing"
        );
    }

    #[test]
    fn the_last_release_names_the_newest_tag_and_when_it_was_made() {
        let dir = repo();
        let dir = dir.path();
        let at = "2026-01-02T03:04:05+00:00";
        git_env(
            dir,
            &[("GIT_COMMITTER_DATE", at)],
            &["commit", "-q", "--allow-empty", "-m", "second"],
        );
        git(dir, &["tag", "v0.3.0"]);
        let (tag, made) = last_release(dir, "v*").unwrap();
        assert_eq!(tag, "v0.3.0");
        assert_eq!(
            made,
            DateTime::parse_from_rfc3339(at)
                .unwrap()
                .with_timezone(&Local)
        );
        assert!(last_release(dir, "rel-*").is_none());
    }

    #[test]
    fn an_untagged_repository_has_every_commit_to_release() {
        let dir = repo();
        assert!(has_commits_since_last_release(dir.path(), "v*"));
    }

    #[test]
    fn a_tag_at_head_leaves_nothing_to_release() {
        let dir = repo();
        let dir = dir.path();
        git(dir, &["tag", "v0.1.0"]);
        assert!(!has_commits_since_last_release(dir, "v*"));
        git(dir, &["commit", "-q", "--allow-empty", "-m", "after"]);
        assert!(has_commits_since_last_release(dir, "v*"));
    }

    #[test]
    fn the_commit_subjects_are_the_ones_since_the_newest_tag_oldest_first() {
        let dir = repo();
        let dir = dir.path();
        git(dir, &["tag", "v0.1.0"]);
        for subject in ["Add a flag", "Fix the box"] {
            git(dir, &["commit", "-q", "--allow-empty", "-m", subject]);
        }
        assert_eq!(
            commit_subjects(dir, "v*"),
            ["Add a flag", "Fix the box"],
            "the tag's own commit stays out"
        );
        assert_eq!(
            commit_subjects(dir, "rel-*"),
            ["first", "Add a flag", "Fix the box"]
        );
    }

    #[test]
    fn an_untagged_repository_lists_every_commit() {
        let dir = repo();
        let dir = dir.path();
        git(dir, &["commit", "-q", "--allow-empty", "-m", "second"]);
        assert_eq!(commit_subjects(dir, "v*"), ["first", "second"]);
    }

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
            "### Added\n\
             - **Board** — Save the panel\n\
             - **Board** — The bell\n\
             - **Agent** — Plan mode\n\
             \n\
             ### Fixed\n\
             - **Board** — The filter\n"
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
        assert_eq!(note.body, "### Added\n- **Board** — Save the panel\n");
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
            "### Added\n- **Board** — Save the panel\n\n\
             ### Technical\n- **Board** — Split the module\n"
        );
    }

    #[test]
    fn a_subject_without_a_domain_lists_plain_and_last() {
        let subjects = [
            "[feat] Ship it".to_string(),
            "[feat] [board] Save the panel".to_string(),
        ];
        assert_eq!(
            note(&subjects, false).body,
            "### Added\n- **Board** — Save the panel\n- Ship it\n"
        );
    }

    #[test]
    fn a_label_after_the_domain_says_nothing_the_note_prints() {
        let subjects = ["[feat] [board] [ux] Save the panel".to_string()];
        assert_eq!(
            note(&subjects, false).body,
            "### Added\n- **Board** — Save the panel\n"
        );
    }

    #[test]
    fn a_note_lists_the_commits_under_a_dated_heading() {
        let subjects = [
            "[feat] Add a flag".to_string(),
            "[bug] Fix the box".to_string(),
        ];
        let body = note(&subjects, false).body;
        let note = section("v1.2.0", "7 Nov 2026", &body);
        assert_eq!(
            note,
            "## v1.2.0 — 7 Nov 2026\n\n### Added\n- Add a flag\n\n### Fixed\n- Fix the box\n"
        );
        assert_eq!(
            section("v1.2.1", "8 Nov 2026", "  "),
            "## v1.2.1 — 8 Nov 2026\n"
        );
    }

    #[test]
    fn a_new_section_goes_under_the_one_changelog_heading() {
        let first = section("v1.0.0", "1 Nov 2026", "- One");
        let second = section("v1.1.0", "7 Nov 2026", "- Two");
        let none = with_section(None, &first);
        assert_eq!(none, "# Changelog\n\n## v1.0.0 — 1 Nov 2026\n\n- One\n");
        assert_eq!(
            with_section(Some(&none), &second),
            "# Changelog\n\n## v1.1.0 — 7 Nov 2026\n\n- Two\n\n\
             ## v1.0.0 — 1 Nov 2026\n\n- One\n"
        );
        assert_eq!(
            with_section(Some("# Mine\n\n- Old\n"), &first),
            "## v1.0.0 — 1 Nov 2026\n\n- One\n\n# Mine\n\n- Old\n"
        );
    }

    #[test]
    fn writing_a_note_prepends_it_to_the_changelog() {
        let dir = repo();
        let project = dir.path();
        let first = section("v1.0.0", "1 Nov 2026", "- One");
        let path = write_note(project, NOTE_FILE, &first).unwrap();
        let second = section("v1.1.0", "7 Nov 2026", "- Two");
        write_note(project, NOTE_FILE, &second).unwrap();
        let text = std::fs::read_to_string(path).unwrap();
        assert_eq!(
            text,
            "# Changelog\n\n## v1.1.0 — 7 Nov 2026\n\n- Two\n\n\
             ## v1.0.0 — 1 Nov 2026\n\n- One\n"
        );
    }

    #[test]
    fn the_note_file_is_the_one_the_project_already_uses() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path();
        assert_eq!(note_file(project, NOTE_FILE), NOTE_FILE);

        std::fs::write(project.join("release-notes.md"), "").unwrap();
        assert_eq!(note_file(project, NOTE_FILE), "release-notes.md");
        assert_eq!(note_file(project, "NOTES.md"), "release-notes.md");

        std::fs::write(project.join("CHANGELOG.md"), "").unwrap();
        assert_eq!(note_file(project, NOTE_FILE), "CHANGELOG.md");

        std::fs::write(project.join("NOTES.md"), "").unwrap();
        assert_eq!(note_file(project, "NOTES.md"), "NOTES.md");
    }

    fn release(text: &str) -> ReleaseSection {
        let document: toml::Table = toml::from_str(text).unwrap();
        document
            .get("release")
            .and_then(|value| value.clone().try_into().ok())
            .unwrap_or_default()
    }

    #[test]
    fn a_release_refuses_a_tag_or_note_file_the_project_cannot_take() {
        assert!(check_tag("").is_err());
        assert!(check_tag("two words").is_err());
        assert!(check_tag("-oops").is_err());
        assert!(check_tag("v0.4.0").is_ok());

        assert!(check_file("").is_err());
        assert!(check_file("../NOTES.md").is_err());
        assert!(check_file("/tmp/NOTES.md").is_err());
        assert!(check_file("docs/CHANGELOG.md").is_ok());
    }

    #[test]
    fn a_release_runs_its_steps_once_and_resumes_where_it_stopped() {
        let dir = repo();
        let project = dir.path();
        let note = section("v0.2.0", "8 Nov 2026", "- One");
        let mut progress = ReleaseProgress::default();

        confirm(
            project,
            "v0.2.0",
            &note,
            NOTE_FILE,
            true,
            false,
            &mut progress,
        )
        .unwrap();
        assert_eq!(
            progress,
            ReleaseProgress {
                committed: true,
                tagged: true,
                pushed: false,
            }
        );
        assert!(progress.finished(false));
        assert!(project.join(NOTE_FILE).is_file());
        let head = git(project, &["rev-parse", "HEAD"]);

        confirm(
            project,
            "v0.2.0",
            &note,
            NOTE_FILE,
            true,
            false,
            &mut progress,
        )
        .unwrap();
        assert_eq!(
            git(project, &["rev-parse", "HEAD"]),
            head,
            "a later confirm adds nothing"
        );
        assert_eq!(git(project, &["tag", "--list", "v0.2.0"]), "v0.2.0");
    }

    #[test]
    fn a_tag_already_taken_stops_the_release() {
        let dir = repo();
        let project = dir.path();
        git(project, &["tag", "v0.2.0"]);
        let note = section("v0.2.0", "8 Nov 2026", "- One");
        let mut progress = ReleaseProgress::default();

        let error = confirm(
            project,
            "v0.2.0",
            &note,
            NOTE_FILE,
            true,
            false,
            &mut progress,
        )
        .unwrap_err();
        assert!(error.contains("already a tag"), "{error}");
        assert_eq!(progress, ReleaseProgress::default());
        assert!(!project.join(NOTE_FILE).exists());
    }

    #[test]
    fn the_choices_are_read_from_the_release_section_and_written_back() {
        let (choices, problems) =
            release("[release]\nnote = false\nfile = \"release-notes.md\"\npush = false\n")
                .choices();
        assert_eq!(
            choices,
            ReleaseChoices {
                note: false,
                file: "release-notes.md".to_string(),
                push: false,
            }
        );
        assert!(problems.is_empty());

        let (choices, problems) = release("[release]\nfile = \"  \"\n").choices();
        assert_eq!(choices, ReleaseChoices::default());
        assert_eq!(problems.len(), 1, "an empty file is reported");
    }
}
