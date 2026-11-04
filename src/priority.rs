//! The Ready priority prefix of issue file names (`001-make-colour-green.md`):
//! the number an issue takes on entering Ready and drops on leaving it, and
//! the renumbering that keeps an ordered column's files in the board's order.

use std::fs;
use std::path::{Path, PathBuf};

use crate::discovery::is_ready_column;
use crate::files;

/// The width of the priority prefix in Ready: `001-make-colour-green.md`.
const PRIORITY_DIGITS: usize = 3;

/// The file name without its Ready priority prefix, if it has one.
pub fn strip_priority(name: &str) -> &str {
    let bytes = name.as_bytes();
    let numbered = bytes.len() > PRIORITY_DIGITS
        && bytes[..PRIORITY_DIGITS].iter().all(u8::is_ascii_digit)
        && bytes[PRIORITY_DIGITS] == b'-';
    if numbered {
        &name[PRIORITY_DIGITS + 1..]
    } else {
        name
    }
}

/// The name an issue takes when moved from one folder into another: entering
/// Ready numbers it after the last issue there, leaving Ready drops the
/// number. Other moves keep the name.
pub fn moved_name(name: &str, from_dir: &Path, to_dir: &Path) -> String {
    if is_ready_column(to_dir) {
        let last = fs::read_dir(to_dir)
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| priority(&entry.file_name().to_string_lossy()))
            .max()
            .unwrap_or(0);
        format!(
            "{:0width$}-{}",
            last + 1,
            strip_priority(name),
            width = PRIORITY_DIGITS
        )
    } else if is_ready_column(from_dir) {
        strip_priority(name).to_string()
    } else {
        name.to_string()
    }
}

/// The priority number of a Ready file name: `001-x.md` → 1.
pub(crate) fn priority(name: &str) -> Option<u32> {
    let rest = strip_priority(name);
    if rest.len() == name.len() {
        return None;
    }
    name[..PRIORITY_DIGITS].parse().ok()
}

/// The `001-x.md` name for `name` at a one-based position, dropping any prefix
/// it already has so renumbering is stable.
pub fn numbered_name(position: usize, name: &str) -> String {
    format!(
        "{:0width$}-{}",
        position,
        strip_priority(name),
        width = PRIORITY_DIGITS
    )
}

/// Renames the files of an ordered column so their `001-` prefixes match
/// `order`. Renames go through hidden temp names in two phases, so a swap
/// (`001-a` ↔ `002-b`) cannot clobber a file. Returns every `(old, new)` pair
/// that changed.
pub fn renumber(dir: &Path, order: &[PathBuf]) -> Result<Vec<(PathBuf, PathBuf)>, String> {
    let changes: Vec<(PathBuf, PathBuf)> = order
        .iter()
        .enumerate()
        .filter_map(|(index, path)| {
            let target = dir.join(numbered_name(index + 1, &files::file_name(path)));
            (target != *path).then(|| (path.clone(), target))
        })
        .collect();
    if changes.is_empty() {
        return Ok(Vec::new());
    }

    // Phase 1: every changing file moves to a unique hidden temp name.
    let mut staged: Vec<(PathBuf, PathBuf, PathBuf)> = Vec::new();
    for (index, (original, target)) in changes.iter().enumerate() {
        let temp = dir.join(format!(".renumber-{}-{index}.tmp", std::process::id()));
        if let Err(err) = std::fs::rename(original, &temp) {
            restore(&staged);
            return Err(format!(
                "Couldn't renumber {}: {err}",
                files::file_name(original)
            ));
        }
        staged.push((temp, original.clone(), target.clone()));
    }

    // Phase 2: temps take their final names. On failure, put back what moved.
    let mut done: Vec<(PathBuf, PathBuf)> = Vec::new();
    for (index, (temp, original, target)) in staged.iter().enumerate() {
        if let Err(err) = std::fs::rename(temp, target) {
            restore(&staged[index..]);
            for (final_name, original) in &done {
                let _ = std::fs::rename(final_name, original);
            }
            return Err(format!(
                "Couldn't renumber {}: {err}",
                files::file_name(original)
            ));
        }
        done.push((target.clone(), original.clone()));
    }
    Ok(changes)
}

/// Best-effort undo of the staging phase.
fn restore(staged: &[(PathBuf, PathBuf, PathBuf)]) {
    for (temp, original, _) in staged {
        let _ = std::fs::rename(temp, original);
    }
}

/// [`renumber`] for an order that may span several folders: each folder
/// keeps the relative order of its own issues.
pub fn renumber_each(order: &[PathBuf]) -> Result<Vec<(PathBuf, PathBuf)>, String> {
    let mut folders: Vec<&Path> = Vec::new();
    for parent in order.iter().filter_map(|path| path.parent()) {
        if !folders.contains(&parent) {
            folders.push(parent);
        }
    }
    let mut renames = Vec::new();
    for folder in folders {
        let own: Vec<PathBuf> = order
            .iter()
            .filter(|path| path.parent() == Some(folder))
            .cloned()
            .collect();
        renames.extend(renumber(folder, &own)?);
    }
    Ok(renames)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entering_ready_numbers_an_issue_and_leaving_drops_the_number() {
        let scratch = tempfile::tempdir().unwrap();
        let dir = scratch.path();
        let (todo, ready, doing) = (dir.join("todo"), dir.join("ready"), dir.join("doing"));
        for d in [&todo, &ready, &doing] {
            fs::create_dir_all(d).unwrap();
        }
        assert_eq!(
            moved_name("make-green.md", &todo, &ready),
            "001-make-green.md"
        );
        fs::write(ready.join("001-a.md"), "").unwrap();
        fs::write(ready.join("007-b.md"), "").unwrap();
        fs::write(ready.join("2026-05-29-dated.md"), "").unwrap();
        // After the last number, not in the first gap: priority is kept.
        assert_eq!(
            moved_name("make-green.md", &todo, &ready),
            "008-make-green.md"
        );
        // A renumbered issue loses its old priority, other numbers stay.
        assert_eq!(moved_name("003-x.md", &doing, &ready), "008-x.md");
        assert_eq!(moved_name("02-ai.md", &todo, &ready), "008-02-ai.md");

        assert_eq!(moved_name("008-02-ai.md", &ready, &doing), "02-ai.md");
        assert_eq!(moved_name("008-x.md", &ready, &todo), "x.md");
        assert_eq!(moved_name("unnumbered.md", &ready, &doing), "unnumbered.md");
        assert_eq!(moved_name("001-x.md", &todo, &doing), "001-x.md");
        assert_eq!(strip_priority("2026-05-29-dated.md"), "2026-05-29-dated.md");
    }

    #[test]
    fn numbered_names() {
        assert_eq!(numbered_name(1, "hello.md"), "001-hello.md");
        assert_eq!(numbered_name(2, "002-byebye.md"), "002-byebye.md");
        assert_eq!(numbered_name(10, "x.md"), "010-x.md");
    }
}
