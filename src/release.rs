//! What Done's Release flag writes: the next tag, the release note that goes
//! into `CHANGELOG.md`, and the git commands that commit, tag and push it.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};

use crate::git;

/// The note file at the project root, newest release first.
pub const NOTE_FILE: &str = "CHANGELOG.md";
const FIRST_TAG: &str = "v0.1.0";

/// The tag after the newest one matching `pattern`, patch bumped: `v0.3.0`
/// gives `v0.3.1`, `rel-1.2` gives `rel-1.2.1`. No matching version gives
/// [`FIRST_TAG`].
pub fn next_tag(dir: &Path, pattern: &str) -> String {
    newest_tag(dir, pattern)
        .and_then(|tag| bumped(&tag))
        .unwrap_or_else(|| FIRST_TAG.to_string())
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

/// `v1.2.3` as `v1.2.4`, keeping the prefix before its first digit.
fn bumped(tag: &str) -> Option<String> {
    let start = tag.find(|c: char| c.is_ascii_digit())?;
    let (prefix, version) = tag.split_at(start);
    let core = version.split(['-', '+']).next().unwrap_or_default();
    let parts: Option<Vec<u64>> = core.split('.').map(|part| part.parse().ok()).collect();
    match parts.as_deref() {
        Some([major, minor, patch]) => Some(format!("{prefix}{major}.{minor}.{}", patch + 1)),
        Some([major, minor]) => Some(format!("{prefix}{major}.{minor}.1")),
        Some([major]) => Some(format!("{prefix}{major}.0.1")),
        _ => None,
    }
}

/// Whether `tag` already names a tag in the repository.
pub fn tag_exists(project: &Path, tag: &str) -> bool {
    git::output(project, &["tag", "--list", tag])
        .map(|out| out.lines().any(|line| line.trim() == tag))
        .unwrap_or(false)
}

/// The Done issues as the note's list, one `- Title` per issue.
pub fn note_body(titles: &[String]) -> String {
    let mut body = String::new();
    for title in titles {
        body.push_str("- ");
        body.push_str(title.trim());
        body.push('\n');
    }
    body
}

/// The instruction for the agent that groups the note: the release's issue
/// titles into sections, rather than handing back the raw list.
pub fn note_prompt(titles: &[String]) -> String {
    format!(
        "You write the release note of a software project from the titles of the issues it \
         closes. Group them under a few `### ` headings that fit what they do, such as Added, \
         Changed, Fixed or Removed. Keep every title's words exactly as written, list every \
         title once, and invent nothing: the titles below are all there is, so read no file. \
         Answer with nothing but the sections, each a `### Heading` line then its `- Title` \
         lines, with no introduction or closing.\n\
         \n\
         ---\n\
         {}",
        note_body(titles)
    )
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

/// Writes `section` to the project's changelog, newest first, and says where
/// it went.
pub fn write_note(project: &Path, section: &str) -> Result<PathBuf, String> {
    let path = project.join(NOTE_FILE);
    let existing = std::fs::read_to_string(&path).ok();
    let text = with_section(existing.as_deref(), section);
    crate::files::atomic_write(&path, text)
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
    fn the_next_tag_bumps_the_newest_matching_version() {
        let dir = repo();
        let dir = dir.path();
        for tag in ["v0.2.9", "v0.3.0", "rel-2.0.0", "nightly"] {
            git(dir, &["tag", tag]);
        }
        assert_eq!(next_tag(dir, "v*"), "v0.3.1");
        assert_eq!(next_tag(dir, "rel-*"), "rel-2.0.1");
        assert_eq!(next_tag(dir, ""), "v0.3.1", "every tag matches");
        assert_eq!(next_tag(dir, "nightly*"), FIRST_TAG);
        assert!(tag_exists(dir, "v0.3.0"));
        assert!(!tag_exists(dir, "v0.3.1"));
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
    fn a_note_lists_the_done_issues_under_a_dated_heading() {
        let titles = ["Add a flag".to_string(), "Fix the box".to_string()];
        let body = note_body(&titles);
        let note = section("v1.2.0", "7 Nov 2026", &body);
        assert_eq!(
            note,
            "## v1.2.0 — 7 Nov 2026\n\n- Add a flag\n- Fix the box\n"
        );
        assert_eq!(
            section("v1.2.1", "8 Nov 2026", "  "),
            "## v1.2.1 — 8 Nov 2026\n"
        );
    }

    #[test]
    fn the_note_prompt_asks_for_sections_of_the_same_titles() {
        let titles = ["Add a flag".to_string(), "Fix the box".to_string()];
        insta::assert_snapshot!(note_prompt(&titles));
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
        let path = write_note(project, &first).unwrap();
        let second = section("v1.1.0", "7 Nov 2026", "- Two");
        write_note(project, &second).unwrap();
        let text = std::fs::read_to_string(path).unwrap();
        assert_eq!(
            text,
            "# Changelog\n\n## v1.1.0 — 7 Nov 2026\n\n- Two\n\n\
             ## v1.0.0 — 1 Nov 2026\n\n- One\n"
        );
    }
}
