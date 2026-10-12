//! A commit's changes as `git show` prints them, parsed into files, hunks and
//! numbered lines for the Diff screen.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use crate::job::Cancel;
use crate::process;

/// How long `git show` may take before it is given up.
const GIT_TIMEOUT: Duration = Duration::from_secs(30);

/// One commit and the files it changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitDiff {
    /// The abbreviated hash.
    pub hash: String,
    pub subject: String,
    pub files: Vec<FileDiff>,
}

/// One file of a commit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileDiff {
    /// `None` for a file the commit added.
    pub old_path: Option<String>,
    /// `None` for a file the commit deleted.
    pub new_path: Option<String>,
    /// Git only says the bytes differ.
    pub binary: bool,
    pub hunks: Vec<Hunk>,
    pub added: usize,
    pub removed: usize,
}

/// What the commit did to a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    Added,
    Deleted,
    Renamed,
    Modified,
}

impl FileDiff {
    /// The file's path after the commit, or before it for a deleted file.
    pub fn path(&self) -> &str {
        self.new_path
            .as_deref()
            .or(self.old_path.as_deref())
            .unwrap_or("")
    }

    pub fn status(&self) -> FileStatus {
        match (&self.old_path, &self.new_path) {
            (None, _) => FileStatus::Added,
            (_, None) => FileStatus::Deleted,
            (Some(old), Some(new)) if old != new => FileStatus::Renamed,
            _ => FileStatus::Modified,
        }
    }

    /// How many diff lines the file has, hunk headers included.
    pub fn line_count(&self) -> usize {
        self.hunks.iter().map(|hunk| hunk.lines.len() + 1).sum()
    }

    /// The file's extension, for its syntax.
    pub fn extension(&self) -> &str {
        let path = self.path();
        let name = path.rsplit('/').next().unwrap_or(path);
        match name.rsplit_once('.') {
            Some((stem, ext)) if !stem.is_empty() => ext,
            // `Makefile`, `.bashrc`: the name stands for its syntax.
            _ => name.trim_start_matches('.'),
        }
    }
}

/// A hunk: its `@@` header and its lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    /// The whole `@@ -a,b +c,d @@ context` line.
    pub header: String,
    pub lines: Vec<Line>,
}

/// One line of a hunk, with its numbers in the old and new file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub kind: LineKind,
    pub old: Option<u32>,
    pub new: Option<u32>,
    /// The text, without its leading ` `, `+` or `-`.
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Context,
    Added,
    Removed,
    /// `\ No newline at end of file`.
    NoNewline,
}

/// The diffs of `hashes`, in that order, read in `dir`'s repository.
pub fn show(dir: &Path, hashes: &[String], cancel: &Cancel) -> Result<Vec<CommitDiff>, String> {
    hashes
        .iter()
        .map(|hash| show_one(dir, hash, cancel))
        .collect()
}

fn show_one(dir: &Path, hash: &str, cancel: &Cancel) -> Result<CommitDiff, String> {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(dir)
        .args([
            "show",
            "--no-color",
            "--no-ext-diff",
            "--find-renames",
            "--diff-merges=first-parent",
            "--format=%h%x09%s",
        ])
        .arg(hash)
        .arg("--");
    let output = process::run(command, "git show", cancel, GIT_TIMEOUT)?.into_result("git show")?;
    Ok(parse_show(&output))
}

/// A `git show --format=%h%x09%s` output: its first line, then the patch.
pub fn parse_show(output: &str) -> CommitDiff {
    let (first, patch) = output.split_once('\n').unwrap_or((output, ""));
    let (hash, subject) = first.split_once('\t').unwrap_or((first, ""));
    CommitDiff {
        hash: hash.trim().to_string(),
        subject: subject.trim().to_string(),
        files: parse(patch),
    }
}

/// The files of a unified diff (`git diff` or `git show` output).
pub fn parse(patch: &str) -> Vec<FileDiff> {
    let mut files: Vec<FileDiff> = Vec::new();
    // Lines still expected in the current hunk, old and new side.
    let mut left = (0u32, 0u32);
    let mut at = (0u32, 0u32);
    for raw in patch.lines() {
        if left != (0, 0) || raw.starts_with('\\') {
            let Some(FileDiff {
                hunks,
                added,
                removed,
                ..
            }) = files.last_mut()
            else {
                continue;
            };
            let Some(hunk) = hunks.last_mut() else {
                continue;
            };
            hunk_line((added, removed), hunk, raw, &mut left, &mut at);
            continue;
        }
        if let Some(rest) = raw.strip_prefix("diff --git ") {
            let (old, new) = git_paths(rest);
            files.push(FileDiff {
                old_path: Some(old),
                new_path: Some(new),
                ..FileDiff::default()
            });
            continue;
        }
        let Some(file) = files.last_mut() else {
            continue;
        };
        if raw.starts_with("new file mode") {
            file.old_path = None;
        } else if raw.starts_with("deleted file mode") {
            file.new_path = None;
        } else if let Some(path) = raw.strip_prefix("rename from ") {
            file.old_path = Some(path.to_string());
        } else if let Some(path) = raw.strip_prefix("rename to ") {
            file.new_path = Some(path.to_string());
        } else if let Some(path) = raw.strip_prefix("--- ") {
            file.old_path = side_path(path, "a/");
        } else if let Some(path) = raw.strip_prefix("+++ ") {
            file.new_path = side_path(path, "b/");
        } else if raw.starts_with("Binary files ") || raw.starts_with("GIT binary patch") {
            file.binary = true;
        } else if raw.starts_with("@@") {
            let (old, new) = hunk_ranges(raw);
            at = (old.0, new.0);
            left = (old.1, new.1);
            file.hunks.push(Hunk {
                header: raw.to_string(),
                lines: Vec::new(),
            });
        }
    }
    files
}

/// One line inside a hunk, numbered, counted, and taken off what is left.
fn hunk_line(
    (added, removed): (&mut usize, &mut usize),
    hunk: &mut Hunk,
    raw: &str,
    left: &mut (u32, u32),
    at: &mut (u32, u32),
) {
    let (kind, text) = match raw.chars().next() {
        Some('+') => (LineKind::Added, &raw[1..]),
        Some('-') => (LineKind::Removed, &raw[1..]),
        Some('\\') => (LineKind::NoNewline, raw),
        Some(' ') => (LineKind::Context, &raw[1..]),
        // An empty context line, its space trimmed by some tools.
        _ => (LineKind::Context, raw),
    };
    let (old, new) = match kind {
        LineKind::Context => (Some(at.0), Some(at.1)),
        LineKind::Added => (None, Some(at.1)),
        LineKind::Removed => (Some(at.0), None),
        LineKind::NoNewline => (None, None),
    };
    if old.is_some() {
        at.0 += 1;
        left.0 = left.0.saturating_sub(1);
    }
    if new.is_some() {
        at.1 += 1;
        left.1 = left.1.saturating_sub(1);
    }
    match kind {
        LineKind::Added => *added += 1,
        LineKind::Removed => *removed += 1,
        _ => {}
    }
    hunk.lines.push(Line {
        kind,
        old,
        new,
        text: text.to_string(),
    });
}

/// `a/x b/y` from a `diff --git` line. A path with spaces is ambiguous
/// there; the `---`/`+++` and rename lines that follow settle it.
fn git_paths(rest: &str) -> (String, String) {
    let rest = rest.trim();
    if let Some(split) = rest.rfind(" b/") {
        let old = rest[..split].trim_start_matches("a/").to_string();
        let new = rest[split + 3..].to_string();
        return (old, new);
    }
    (rest.to_string(), rest.to_string())
}

/// A `---` or `+++` path without its `a/` or `b/`; `None` for `/dev/null`.
fn side_path(path: &str, prefix: &str) -> Option<String> {
    let path = path.trim_end();
    let path = path.split('\t').next().unwrap_or(path);
    (path != "/dev/null").then(|| path.strip_prefix(prefix).unwrap_or(path).to_string())
}

/// `(start, count)` of the old and new side of an `@@ -a,b +c,d @@` header.
fn hunk_ranges(header: &str) -> ((u32, u32), (u32, u32)) {
    let mut parts = header.split_whitespace().skip(1);
    let range = |part: Option<&str>| {
        let part = part.unwrap_or("").trim_start_matches(['-', '+']);
        let (start, count) = part.split_once(',').unwrap_or((part, "1"));
        (start.parse().unwrap_or(0), count.parse().unwrap_or(0))
    };
    let old = range(parts.next());
    let new = range(parts.next());
    (old, new)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHOW: &str = "\
abc1234\tFix the parser

diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,4 +1,4 @@ fn main() {
 use std::io;
-let a = 1;
+let a = 2;
+let b = 3;

--- not a header
@@ -10,2 +11,2 @@
 keep
-old
+new
diff --git a/docs/new.md b/docs/new.md
new file mode 100644
index 0000000..3333333
--- /dev/null
+++ b/docs/new.md
@@ -0,0 +1 @@
+# New
\\ No newline at end of file
diff --git a/gone.txt b/gone.txt
deleted file mode 100644
--- a/gone.txt
+++ /dev/null
@@ -1 +0,0 @@
-bye
diff --git a/logo.png b/logo.png
index 4444444..5555555 100644
Binary files a/logo.png and b/logo.png differ
diff --git a/old name.rs b/new name.rs
similarity index 90%
rename from old name.rs
rename to new name.rs
";

    #[test]
    fn a_commit_header_and_its_files_parse() {
        let diff = parse_show(SHOW);
        assert_eq!(diff.hash, "abc1234");
        assert_eq!(diff.subject, "Fix the parser");
        let paths: Vec<&str> = diff.files.iter().map(FileDiff::path).collect();
        assert_eq!(
            paths,
            [
                "src/lib.rs",
                "docs/new.md",
                "gone.txt",
                "logo.png",
                "new name.rs"
            ]
        );
        let statuses: Vec<FileStatus> = diff.files.iter().map(FileDiff::status).collect();
        assert_eq!(
            statuses,
            [
                FileStatus::Modified,
                FileStatus::Added,
                FileStatus::Deleted,
                FileStatus::Modified,
                FileStatus::Renamed,
            ]
        );
        assert!(diff.files[3].binary && diff.files[3].hunks.is_empty());
        assert_eq!(diff.files[4].old_path.as_deref(), Some("old name.rs"));
    }

    #[test]
    fn hunk_lines_are_numbered_and_counted() {
        let files = parse_show(SHOW).files;
        let lib = &files[0];
        assert_eq!((lib.added, lib.removed), (3, 3));
        assert_eq!(lib.hunks.len(), 2);
        let first = &lib.hunks[0];
        assert_eq!(first.header, "@@ -1,4 +1,4 @@ fn main() {");
        let numbers: Vec<(LineKind, Option<u32>, Option<u32>)> = first
            .lines
            .iter()
            .map(|line| (line.kind, line.old, line.new))
            .collect();
        assert_eq!(
            numbers,
            [
                (LineKind::Context, Some(1), Some(1)),
                (LineKind::Removed, Some(2), None),
                (LineKind::Added, None, Some(2)),
                (LineKind::Added, None, Some(3)),
                (LineKind::Context, Some(3), Some(4)),
                (LineKind::Removed, Some(4), None),
            ]
        );
        assert_eq!(
            first.lines[5].text, "-- not a header",
            "counts decide, not prefixes"
        );
        assert_eq!(first.lines[1].text, "let a = 1;");
        assert_eq!(lib.hunks[1].lines[0].old, Some(10));
        assert_eq!(lib.hunks[1].lines[0].new, Some(11));
    }

    #[test]
    fn a_missing_newline_note_stays_with_its_file() {
        let files = parse_show(SHOW).files;
        let new = &files[1];
        assert_eq!(new.hunks[0].lines.len(), 2);
        assert_eq!(new.hunks[0].lines[1].kind, LineKind::NoNewline);
        assert_eq!(files[2].hunks[0].lines[0].kind, LineKind::Removed);
    }

    #[test]
    fn extensions_name_the_syntax() {
        let file = |path: &str| FileDiff {
            new_path: Some(path.to_string()),
            ..FileDiff::default()
        };
        assert_eq!(file("src/app.rs").extension(), "rs");
        assert_eq!(file("web/index.test.ts").extension(), "ts");
        assert_eq!(file("Makefile").extension(), "Makefile");
        assert_eq!(file(".bashrc").extension(), "bashrc");
    }

    #[test]
    fn a_real_commit_is_shown() {
        let scratch = tempfile::tempdir().unwrap();
        let dir = scratch.path();
        let git = |args: &[&str]| {
            let status = Command::new("git")
                .arg("-C")
                .arg(dir)
                .args([
                    "-c",
                    "user.name=t",
                    "-c",
                    "user.email=t@t",
                    "-c",
                    "commit.gpgsign=false",
                ])
                .args(args)
                .output()
                .unwrap();
            assert!(status.status.success(), "{status:?}");
            String::from_utf8_lossy(&status.stdout).trim().to_string()
        };
        git(&["init", "-q"]);
        std::fs::write(dir.join("a.txt"), "one\ntwo\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "First"]);
        std::fs::write(dir.join("a.txt"), "one\nthree\n").unwrap();
        git(&["commit", "-q", "-am", "Second"]);
        let hash = git(&["rev-parse", "--short", "HEAD"]);

        let diffs = show(dir, std::slice::from_ref(&hash), &Cancel::default()).unwrap();
        assert_eq!(diffs[0].hash, hash);
        assert_eq!(diffs[0].subject, "Second");
        let file = &diffs[0].files[0];
        assert_eq!((file.path(), file.added, file.removed), ("a.txt", 1, 1));
    }
}
