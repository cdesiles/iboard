//! The release tags drawn on the Trends, and the `[stats]` section that picks
//! them:
//!
//! ```toml
//! [stats]
//! show_releases = true  # false: no release lines on the Trends
//! release_tags = "v*"   # which tags are releases
//! ```
//!
//! The trends are one sample per commit, so a release sits on a commit, not at
//! its day: the commit its tag points to when the page lists it, else, for a
//! tag off the first-parent line, the page's newest commit no younger than the
//! tagged one.

use std::path::Path;

use chrono::{DateTime, FixedOffset, NaiveDate};
use serde::{Deserialize, Serialize};

use super::{Listed, STOPPED};
use crate::git;
use crate::job::Cancel;

const SECTION: &str = "stats";
const DEFAULT_TAGS: &str = "v*";

/// The `[stats]` section of the user configuration.
#[derive(Default, Deserialize, Serialize)]
pub struct ReleasePrefsSection {
    show_releases: Option<bool>,
    release_tags: Option<String>,
}

/// `[stats]`: whether the Trends draw releases, and which tags are releases.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleasePrefs {
    pub show: bool,
    /// A pattern as `git for-each-ref` takes it, under `refs/tags/`.
    pub tags: String,
}

impl Default for ReleasePrefs {
    fn default() -> Self {
        Self {
            show: true,
            tags: DEFAULT_TAGS.to_string(),
        }
    }
}

impl ReleasePrefsSection {
    /// The preferences it names; empty `release_tags` is reported and keeps
    /// the default.
    pub fn prefs(self) -> (ReleasePrefs, Vec<String>) {
        let defaults = ReleasePrefs::default();
        let tags = match self.release_tags {
            None => defaults.tags,
            Some(tags) if tags.trim().is_empty() => {
                return (
                    ReleasePrefs {
                        show: self.show_releases.unwrap_or(defaults.show),
                        tags: defaults.tags,
                    },
                    vec![format!("[{SECTION}]: release_tags is empty")],
                );
            }
            Some(tags) => tags.trim().to_string(),
        };
        (
            ReleasePrefs {
                show: self.show_releases.unwrap_or(defaults.show),
                tags,
            },
            Vec::new(),
        )
    }
}

impl ReleasePrefs {
    /// The `[stats]` section that stores them.
    pub fn section(&self) -> ReleasePrefsSection {
        ReleasePrefsSection {
            show_releases: Some(self.show),
            release_tags: Some(self.tags.clone()),
        }
    }
}

/// A release on the page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub tag: String,
    /// The tagger's day, or the commit's for a lightweight tag.
    pub day: NaiveDate,
    /// The commit of the page it sits on, an index into its snapshots.
    pub at: usize,
    /// A version `x.y.Z` with Z > 0.
    pub is_patch: bool,
    /// The commits since `previous`, or in all without one.
    pub commits: usize,
    /// The release before it, on this page or not.
    pub previous: Option<String>,
}

impl Release {
    /// "v0.4.0 · 14 Aug · 37 commits since v0.3.1".
    pub fn describe(&self) -> String {
        let commits = match self.commits {
            1 => "1 commit".to_string(),
            count => format!("{count} commits"),
        };
        let since = self
            .previous
            .as_ref()
            .map(|tag| format!(" since {tag}"))
            .unwrap_or_default();
        format!(
            "{} · {} · {commits}{since}",
            self.tag,
            self.day.format("%-d %b")
        )
    }
}

/// The releases of a page.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Releases {
    /// The tags matching `release_tags` anywhere in the repository.
    pub matched: usize,
    /// Those on the page, left to right.
    pub shown: Vec<Release>,
}

/// A tag as `git for-each-ref` lists it.
struct Tag {
    name: String,
    date: DateTime<FixedOffset>,
    commit: String,
    commit_date: DateTime<FixedOffset>,
}

fn date(text: &str) -> Option<DateTime<FixedOffset>> {
    DateTime::parse_from_rfc3339(text).ok()
}

/// Whether `tag` reads as `x.y.Z`, Z > 0, whatever comes before the first digit
/// or after a `-` or `+`.
fn is_patch(tag: &str) -> bool {
    let version = tag.trim_start_matches(|c: char| !c.is_ascii_digit());
    let core = version.split(['-', '+']).next().unwrap_or_default();
    let parts: Option<Vec<u64>> = core.split('.').map(|part| part.parse().ok()).collect();
    matches!(parts.as_deref(), Some([_, _, patch]) if *patch > 0)
}

/// The tags matching `pattern` that point to a commit, oldest first.
fn tags(dir: &Path, pattern: &str) -> Result<Vec<Tag>, String> {
    let out = git::output(
        dir,
        &[
            "for-each-ref",
            "--sort=creatordate",
            "--format=%(refname:strip=2)%1f%(creatordate:iso-strict)%1f\
             %(objecttype)%1f%(objectname)%1f%(committerdate:iso-strict)%1f\
             %(*objecttype)%1f%(*objectname)%1f%(*committerdate:iso-strict)",
            &format!("refs/tags/{pattern}"),
        ],
    )?;
    Ok(out.lines().filter_map(parse_tag).collect())
}

fn parse_tag(line: &str) -> Option<Tag> {
    let fields: Vec<&str> = line.split('\x1f').collect();
    let [
        name,
        created,
        kind,
        object,
        committed,
        peeled_kind,
        peeled,
        peeled_committed,
    ] = fields[..]
    else {
        return None;
    };
    let (commit, commit_date) = match (kind, peeled_kind) {
        ("commit", _) => (object, committed),
        (_, "commit") => (peeled, peeled_committed),
        _ => return None,
    };
    Some(Tag {
        name: name.to_string(),
        date: date(created)?,
        commit: commit.to_string(),
        commit_date: date(commit_date)?,
    })
}

/// Where `tag` sits among `commits`, newest first, as an index oldest first.
fn position(tag: &Tag, commits: &[Listed]) -> Option<usize> {
    let from_newest = commits
        .iter()
        .position(|commit| commit.hash == tag.commit)
        .or_else(|| {
            if date(&commits.first()?.date)? < tag.commit_date {
                return None;
            }
            commits
                .iter()
                .position(|commit| date(&commit.date).is_some_and(|d| d <= tag.commit_date))
        })?;
    Some(commits.len() - 1 - from_newest)
}

/// The tags matching `pattern`, placed on the page's `commits`, newest first.
pub(super) fn read(
    dir: &Path,
    pattern: &str,
    commits: &[Listed],
    cancel: &Cancel,
) -> Result<Releases, String> {
    let tags = tags(dir, pattern)?;
    let mut shown = Vec::new();
    for (index, tag) in tags.iter().enumerate() {
        let Some(at) = position(tag, commits) else {
            continue;
        };
        if cancel.is_cancelled() {
            return Err(STOPPED.to_string());
        }
        let previous = index.checked_sub(1).and_then(|before| tags.get(before));
        let range = match previous {
            Some(previous) => format!("{}..{}", previous.commit, tag.commit),
            None => tag.commit.clone(),
        };
        let count = git::output(dir, &["rev-list", "--count", &range])?;
        shown.push(Release {
            tag: tag.name.clone(),
            day: tag.date.date_naive(),
            at,
            is_patch: is_patch(&tag.name),
            commits: count.trim().parse().unwrap_or_default(),
            previous: previous.map(|previous| previous.name.clone()),
        });
    }
    shown.sort_by_key(|release| release.at);
    Ok(Releases {
        matched: tags.len(),
        shown,
    })
}

#[cfg(test)]
mod tests {
    use super::super::{Measures, Need, Source};
    use super::*;
    use crate::scratch_git::{git, git_env};
    use crate::settings;
    use std::sync::Arc;

    fn repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-q", "-b", "main"]);
        dir
    }

    /// A commit on `day`, at noon UTC.
    fn commit(dir: &Path, day: &str) {
        let date = format!("{day}T12:00:00+00:00");
        git_env(
            dir,
            &[("GIT_COMMITTER_DATE", &date), ("GIT_AUTHOR_DATE", &date)],
            &["commit", "-q", "--allow-empty", "-m", day],
        );
    }

    fn releases(dir: &Path, pattern: Option<&str>) -> Releases {
        let source = Source::new(dir, dir, 0, Arc::new(Measures::new(dir)));
        let source = match pattern {
            Some(pattern) => source.with_release_tags(pattern),
            None => source,
        };
        source
            .read(&[Need::Releases], &Cancel::default())
            .unwrap()
            .releases
    }

    #[test]
    fn a_release_sits_on_its_commit_and_counts_since_the_previous_one() {
        let dir = repo();
        let dir = dir.path();
        commit(dir, "2026-06-01");
        commit(dir, "2026-06-02");
        git(dir, &["tag", "v0.1.0"]);
        commit(dir, "2026-06-03");
        commit(dir, "2026-06-04");
        commit(dir, "2026-06-05");
        git_env(
            dir,
            &[("GIT_COMMITTER_DATE", "2026-08-14T09:00:00+00:00")],
            &["tag", "-a", "v0.1.1", "-m", "patch"],
        );
        git(dir, &["tag", "nightly"]);

        let found = releases(dir, None);

        assert_eq!(found.matched, 2, "nightly doesn't match v*");
        let [first, second] = &found.shown[..] else {
            panic!("{found:?}");
        };
        assert_eq!(
            (first.tag.as_str(), first.at, first.is_patch),
            ("v0.1.0", 1, false)
        );
        assert_eq!(first.describe(), "v0.1.0 · 2 Jun · 2 commits");
        assert_eq!(
            (second.tag.as_str(), second.at, second.is_patch),
            ("v0.1.1", 4, true)
        );
        assert_eq!(
            second.describe(),
            "v0.1.1 · 14 Aug · 3 commits since v0.1.0",
            "an annotated tag has the tagger's date"
        );
    }

    #[test]
    fn a_tag_that_is_not_a_version_shows_under_a_wider_pattern() {
        let dir = repo();
        let dir = dir.path();
        commit(dir, "2026-06-01");
        git(dir, &["tag", "nightly"]);
        let found = releases(dir, Some("*"));
        assert_eq!(found.shown[0].tag, "nightly");
        assert!(!found.shown[0].is_patch);
        assert_eq!(found.shown[0].describe(), "nightly · 1 Jun · 1 commit");
    }

    #[test]
    fn no_matching_tag_is_nothing_and_no_tree_tag_counts() {
        let dir = repo();
        let dir = dir.path();
        commit(dir, "2026-06-01");
        git(dir, &["tag", "v1-tree", "HEAD^{tree}"]);
        git(dir, &["tag", "other"]);
        assert_eq!(releases(dir, None), Releases::default());
    }

    #[test]
    fn a_tag_off_the_first_parent_line_falls_at_the_commit_before_it() {
        let dir = repo();
        let dir = dir.path();
        commit(dir, "2026-06-01");
        commit(dir, "2026-06-03");
        git(dir, &["checkout", "-q", "-b", "maintenance", "HEAD~1"]);
        commit(dir, "2026-06-02");
        git(dir, &["tag", "v0.1.1"]);
        commit(dir, "2026-06-09");
        git(dir, &["tag", "v0.1.2"]);
        git(dir, &["checkout", "-q", "main"]);
        commit(dir, "2026-06-05");

        let found = releases(dir, None);

        assert_eq!(found.matched, 2);
        let [on_page] = &found.shown[..] else {
            panic!("{found:?}");
        };
        assert_eq!((on_page.tag.as_str(), on_page.at), ("v0.1.1", 0));
    }

    #[test]
    fn a_tag_older_than_the_page_stays_off_it() {
        let dir = repo();
        let dir = dir.path();
        commit(dir, "2026-06-01");
        git(dir, &["tag", "v0.1.0"]);
        for at in 0..super::super::PAGE {
            commit(dir, &format!("2026-07-{:02}", at % 28 + 1));
        }
        let found = releases(dir, None);
        assert_eq!(found.matched, 1);
        assert!(found.shown.is_empty());
    }

    #[test]
    fn patches_read_as_x_y_z_past_zero() {
        for tag in [
            "v0.3.1",
            "1.2.3",
            "release-2.0.1",
            "v1.0.1-rc.1",
            "v1.0.1+build",
        ] {
            assert!(is_patch(tag), "{tag}");
        }
        for tag in ["v0.4.0", "v1.2", "v1", "nightly", "v1.2.3.4", "v1.x.3"] {
            assert!(!is_patch(tag), "{tag}");
        }
    }

    fn stats(text: &str) -> ReleasePrefsSection {
        let document: toml::Table = toml::from_str(text).unwrap();
        document
            .get("stats")
            .and_then(|value| value.clone().try_into().ok())
            .unwrap_or_default()
    }

    #[test]
    fn the_section_keeps_comments_and_both_keys() {
        let scratch = tempfile::tempdir().unwrap();
        let file = scratch.path().join("iboard.toml");
        std::fs::write(
            &file,
            "# mine\n[stats]\nrelease_tags = \"rel-*\" # our scheme\n",
        )
        .unwrap();
        let mut prefs = stats(&std::fs::read_to_string(&file).unwrap()).prefs().0;
        assert_eq!(prefs.tags, "rel-*");
        assert!(prefs.show);
        prefs.show = false;
        settings::save(
            &file,
            &[settings::Section::new("stats", &prefs.section()).unwrap()],
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "# mine\n[stats]\nrelease_tags = \"rel-*\" # our scheme\nshow_releases = false\n"
        );
        assert_eq!(
            stats(&std::fs::read_to_string(&file).unwrap()).prefs().0,
            prefs
        );
    }

    #[test]
    fn empty_release_tags_are_reported() {
        let (prefs, problems) = stats("[stats]\nrelease_tags = \" \"\n").prefs();
        assert_eq!(prefs, ReleasePrefs::default());
        assert_eq!(problems, ["[stats]: release_tags is empty"]);
    }
}
