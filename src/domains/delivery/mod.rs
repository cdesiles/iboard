//! What Done's Release flag writes: the next tag, the release note that goes
//! into the project's changelog, and the git commands that commit, tag and
//! push it. The modal's choices are kept in the user configuration.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use crate::platform::git;

mod note;

pub mod hook;
pub mod update;

pub use hook::Hook;
pub use note::{Kind, Note, note, opening, preamble_prompt};

/// The note file at the project root, newest release first.
pub const NOTE_FILE: &str = "CHANGELOG.md";
/// The note file names a project may already use, lower case.
const NOTE_FILES: [&str; 2] = ["changelog.md", "release-notes.md"];
const SECTION: &str = "release";
const FIRST_TAG: &str = "v0.1.0";

/// What the release modal opens on: whether to write a note, in which file,
/// whether an agent opens it, and whether to push afterwards, and which
/// projects' hooks the user trusts. Kept in `[release]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseChoices {
    pub note: bool,
    pub file: String,
    pub preamble: bool,
    pub push: bool,
    /// The [`Hook::fingerprint`] the user trusted, per project folder.
    pub trusted: BTreeMap<String, String>,
}

impl Default for ReleaseChoices {
    fn default() -> Self {
        Self {
            note: true,
            file: NOTE_FILE.to_string(),
            preamble: false,
            push: true,
            trusted: BTreeMap::new(),
        }
    }
}

/// The `[release]` section of the user configuration.
#[derive(Default, Deserialize, Serialize)]
pub struct ReleaseSection {
    note: Option<bool>,
    file: Option<String>,
    preamble: Option<bool>,
    push: Option<bool>,
    trusted: Option<BTreeMap<String, String>>,
}

impl ReleaseSection {
    /// The choices it names; an empty file name is reported and keeps the
    /// default.
    pub fn choices(self) -> (ReleaseChoices, Vec<String>) {
        let defaults = ReleaseChoices::default();
        let mut problems = Vec::new();
        let file = match self.file.map(|file| file.trim().to_string()) {
            None => defaults.file,
            Some(file) if file.is_empty() => {
                problems.push(format!("[{SECTION}]: file is empty"));
                defaults.file
            }
            Some(file) => file,
        };
        let choices = ReleaseChoices {
            note: self.note.unwrap_or(defaults.note),
            file,
            preamble: self.preamble.unwrap_or(defaults.preamble),
            push: self.push.unwrap_or(defaults.push),
            trusted: self.trusted.unwrap_or_default(),
        };
        (choices, problems)
    }
}

impl ReleaseChoices {
    /// The `[release]` section that stores them.
    pub fn section(&self) -> ReleaseSection {
        ReleaseSection {
            note: Some(self.note),
            file: Some(self.file.clone()),
            preamble: Some(self.preamble),
            push: Some(self.push),
            trusted: (!self.trusted.is_empty()).then(|| self.trusted.clone()),
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
    /// `patch`, `minor` or `major`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Patch => "patch",
            Self::Minor => "minor",
            Self::Major => "major",
        }
    }

    /// What the subjects bump: a `feat` in the window makes a minor, anything
    /// less a patch.
    pub fn of(subjects: &[String]) -> Self {
        match note::has_feature(subjects) {
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
fn tag_exists(project: &Path, tag: &str) -> bool {
    git::output(project, &["tag", "--list", tag])
        .map(|out| out.lines().any(|line| line.trim() == tag))
        .unwrap_or(false)
}

/// How far a release got: the hook run, the note committed, the tag made, the
/// push done.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReleaseProgress {
    pub prepared: bool,
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
fn check_tag(tag: &str) -> Result<(), String> {
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
fn check_file(file: &str) -> Result<(), String> {
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

/// A release to make: where, under which tag, with which note, and what the
/// project's own hook adds.
pub struct Release<'a> {
    pub project: &'a Path,
    pub tag: &'a str,
    /// The section the note file gets, and the tag's message.
    pub note: &'a str,
    pub file: &'a str,
    pub update_note: bool,
    pub push: bool,
    pub bump: Bump,
    /// The tag the release follows, if any.
    pub previous: Option<&'a str>,
    pub hook: Option<&'a Hook>,
}

/// Makes the release in order: the hook, the note and the commit of both, the
/// tag, the push. An error stops it where it is, and a later confirm resumes
/// from there.
pub fn confirm(release: &Release, progress: &mut ReleaseProgress) -> Result<(), String> {
    let project = release.project;
    let version = release.tag.trim();
    let file = release.file.trim();
    if !progress.tagged {
        check_tag(version)?;
        if tag_exists(project, version) {
            return Err(format!("{version} is already a tag."));
        }
    }
    let writes_note = release.update_note && !progress.committed;
    if writes_note {
        check_file(file)?;
    }
    if let Some(hook) = release.hook
        && !progress.prepared
        && !progress.committed
    {
        let env = hook::Env {
            tag: version,
            previous: release.previous,
            bump: release.bump,
            note_file: release.update_note.then_some(file),
        };
        hook::prepare(project, hook, &env)?;
        progress.prepared = true;
    }
    if !progress.committed {
        let mut paths = Vec::new();
        if let Some(hook) = release.hook.filter(|hook| hook::changes(project, hook)) {
            paths.extend(hook.files.iter().cloned());
        }
        if writes_note {
            write_note(project, file, release.note)?;
            paths.push(file.to_string());
        }
        if !paths.is_empty() {
            commit(project, &paths, &format!("Release {version}"))?;
        }
        progress.committed = true;
    }
    if !progress.tagged {
        tag(project, version, release.note)?;
        progress.tagged = true;
    }
    if release.push && !progress.pushed {
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

/// The subjects and bodies of the commits since the newest tag matching
/// `pattern`, oldest first, without their `closes` lines: the why the note's
/// own lines leave out.
pub fn commit_context(dir: &Path, pattern: &str) -> String {
    let range = match newest_tag(dir, pattern) {
        Some(tag) => format!("{tag}..HEAD"),
        None => "HEAD".to_string(),
    };
    let Ok(out) = git::output(dir, &["log", "--reverse", "--format=%s%n%b%n----", &range]) else {
        return String::new();
    };
    out.split("----")
        .map(str::trim)
        .filter(|block| !block.is_empty())
        .map(|block| {
            block
                .lines()
                .filter(|line| !line.trim_start().starts_with("closes "))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .filter(|block| !block.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
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
fn with_section(existing: Option<&str>, section: &str) -> String {
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
fn write_note(project: &Path, file: &str, section: &str) -> Result<PathBuf, String> {
    let path = project.join(file.trim());
    let existing = std::fs::read_to_string(&path).ok();
    let text = with_section(existing.as_deref(), section);
    crate::platform::files::atomic_write(&path, text)
        .map_err(|err| format!("Couldn't write {}: {err}", path.display()))?;
    Ok(path)
}

/// Commits `paths`, relative to the project, with `message`, and nothing
/// else that is staged.
fn commit(project: &Path, paths: &[String], message: &str) -> Result<(), String> {
    let paths = paths.iter().map(String::as_str);
    let add = ["add", "--"].into_iter().chain(paths.clone());
    git::output(project, &add.collect::<Vec<_>>())?;
    let commit = ["commit", "-m", message, "--"].into_iter().chain(paths);
    git::output(project, &commit.collect::<Vec<_>>())?;
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
    fn a_note_lists_the_commits_under_a_dated_heading() {
        let subjects = [
            "[feat] Add a flag".to_string(),
            "[bug] Fix the box".to_string(),
        ];
        let body = note(&subjects, false).body;
        let note = section("v1.2.0", "7 Nov 2026", &body);
        assert_eq!(
            note,
            "## v1.2.0 — 7 Nov 2026\n\n### New\n- Add a flag\n\n### Fixed\n- Fix the box\n"
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

    fn request<'a>(project: &'a Path, tag: &'a str, note: &'a str) -> Release<'a> {
        Release {
            project,
            tag,
            note,
            file: NOTE_FILE,
            update_note: true,
            push: false,
            bump: Bump::Minor,
            previous: None,
            hook: None,
        }
    }

    fn hook(command: &str, files: &[&str]) -> Hook {
        Hook {
            command: command.to_string(),
            files: files.iter().map(ToString::to_string).collect(),
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_hook_changes_land_in_the_release_commit_with_the_note() {
        let dir = repo();
        let project = dir.path();
        std::fs::write(project.join("version.txt"), "0.1.0\n").unwrap();
        git(project, &["add", "version.txt"]);
        git(project, &["commit", "-q", "-m", "version"]);
        std::fs::write(project.join("other.txt"), "mine\n").unwrap();
        git(project, &["add", "other.txt"]);
        let hook = hook("echo \"$IBOARD_VERSION\" > version.txt", &["version.txt"]);
        let note = section("v0.2.0", "8 Nov 2026", "- One");
        let mut progress = ReleaseProgress::default();

        let mut release = request(project, "v0.2.0", &note);
        release.hook = Some(&hook);
        confirm(&release, &mut progress).unwrap();

        assert!(progress.prepared && progress.committed && progress.tagged);
        assert_eq!(
            std::fs::read_to_string(project.join("version.txt")).unwrap(),
            "0.2.0\n"
        );
        assert_eq!(
            git(project, &["show", "--name-only", "--format=%s", "HEAD"]),
            "Release v0.2.0\n\nCHANGELOG.md\nversion.txt"
        );
        assert_eq!(
            git(project, &["diff", "--name-only", "--cached"]),
            "other.txt",
            "what else was staged stays staged"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_hook_runs_without_a_note_and_a_retry_does_not_run_it_twice() {
        let dir = repo();
        let project = dir.path();
        std::fs::write(project.join("version.txt"), "0.1.0\n").unwrap();
        std::fs::write(project.join("count.txt"), "").unwrap();
        git(project, &["add", "."]);
        git(project, &["commit", "-q", "-m", "version"]);
        let hook = hook(
            "echo \"$IBOARD_VERSION\" > version.txt; echo ran >> count.txt",
            &["version.txt"],
        );
        let mut release = request(project, "v0.2.0", "- One");
        release.update_note = false;
        release.hook = Some(&hook);
        let mut progress = ReleaseProgress::default();

        confirm(&release, &mut progress).unwrap();
        assert_eq!(
            git(project, &["show", "--name-only", "--format=%s", "HEAD"]),
            "Release v0.2.0\n\nversion.txt",
            "a tag-only release still commits what its hook changed"
        );

        let mut again = ReleaseProgress {
            prepared: true,
            ..ReleaseProgress::default()
        };
        release.tag = "v0.3.0";
        confirm(&release, &mut again).unwrap();
        assert_eq!(
            std::fs::read_to_string(project.join("count.txt")).unwrap(),
            "ran\n",
            "a prepared release is not prepared again"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_hook_with_nothing_to_change_adds_no_commit() {
        let dir = repo();
        let project = dir.path();
        std::fs::write(project.join("version.txt"), "0.2.0\n").unwrap();
        git(project, &["add", "."]);
        git(project, &["commit", "-q", "-m", "version"]);
        let head = git(project, &["rev-parse", "HEAD"]);
        let hook = hook("echo \"$IBOARD_VERSION\" > version.txt", &["version.txt"]);
        let mut release = request(project, "v0.2.0", "- One");
        release.update_note = false;
        release.hook = Some(&hook);

        confirm(&release, &mut ReleaseProgress::default()).unwrap();
        assert_eq!(git(project, &["rev-parse", "HEAD"]), head);
        assert_eq!(git(project, &["tag", "--list", "v0.2.0"]), "v0.2.0");
    }

    #[cfg(unix)]
    #[test]
    fn a_failing_hook_stops_the_release_before_the_note_is_written() {
        let dir = repo();
        let project = dir.path();
        let hook = hook("echo broken >&2; exit 1", &[]);
        let mut release = request(project, "v0.2.0", "- One");
        release.hook = Some(&hook);
        let mut progress = ReleaseProgress::default();

        let error = confirm(&release, &mut progress).unwrap_err();
        assert!(error.contains("broken"), "{error}");
        assert_eq!(progress, ReleaseProgress::default());
        assert!(!project.join(NOTE_FILE).exists());
        assert_eq!(git(project, &["tag", "--list"]), "");
    }

    #[test]
    fn a_hook_is_trusted_for_one_project_until_it_changes() {
        let mut choices = ReleaseChoices::default();
        let project = Path::new("/work/iboard");
        let hook = hook("make version", &["Cargo.toml"]);
        assert!(!hook.trusted(&choices.trusted, project));

        hook.trust(&mut choices.trusted, project);
        assert!(hook.trusted(&choices.trusted, project));
        assert!(!hook.trusted(&choices.trusted, Path::new("/work/other")));
        let changed = self::hook("make other", &["Cargo.toml"]);
        assert!(!changed.trusted(&choices.trusted, project));

        let (read, problems) = choices.section().choices();
        assert_eq!(read, choices, "the trust survives the configuration file");
        assert!(problems.is_empty());
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

        confirm(&request(project, "v0.2.0", &note), &mut progress).unwrap();
        assert_eq!(
            progress,
            ReleaseProgress {
                prepared: false,
                committed: true,
                tagged: true,
                pushed: false,
            }
        );
        assert!(progress.finished(false));
        assert!(project.join(NOTE_FILE).is_file());
        let head = git(project, &["rev-parse", "HEAD"]);

        confirm(&request(project, "v0.2.0", &note), &mut progress).unwrap();
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

        let error = confirm(&request(project, "v0.2.0", &note), &mut progress).unwrap_err();
        assert!(error.contains("already a tag"), "{error}");
        assert_eq!(progress, ReleaseProgress::default());
        assert!(!project.join(NOTE_FILE).exists());
    }

    #[test]
    fn the_choices_are_read_from_the_release_section_and_written_back() {
        let (choices, problems) = release(
            "[release]\nnote = false\nfile = \"release-notes.md\"\npreamble = true\npush = false\n",
        )
        .choices();
        assert_eq!(
            choices,
            ReleaseChoices {
                note: false,
                file: "release-notes.md".to_string(),
                preamble: true,
                push: false,
                trusted: BTreeMap::new(),
            }
        );
        assert!(problems.is_empty());

        let (choices, problems) = release("[release]\nfile = \"  \"\n").choices();
        assert_eq!(choices, ReleaseChoices::default());
        assert_eq!(problems.len(), 1, "an empty file is reported");
    }
}
