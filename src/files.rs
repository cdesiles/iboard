//! Writing files so that a crash or a full disk never leaves one half-written.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// A fresh path in the temp folder, `iboard-<tag>-<pid>-<n>.<extension>`: the
/// counter keeps two files made at the same instant apart.
pub fn temp_path(tag: &str, extension: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "iboard-{tag}-{}-{n}.{extension}",
        std::process::id()
    ))
}

/// Writes `contents` to `path` through a hidden temp file in the same folder,
/// synced to disk, then renamed over `path`. The temp name starts with `.`, so
/// board discovery never reads it, and carries the process id, so two iboard
/// windows never share one.
pub fn atomic_write(path: &Path, contents: impl AsRef<[u8]>) -> std::io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    let temp = parent.join(format!(".{name}.{}.tmp", std::process::id()));
    let written = std::fs::File::create(&temp).and_then(|mut file| {
        file.write_all(contents.as_ref())?;
        file.sync_all()
    });
    let result = written.and_then(|()| std::fs::rename(&temp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_replaces_content_without_leaving_temp_files() {
        let scratch = tempfile::tempdir().unwrap();
        let dir = scratch.path();
        let file = dir.join("issue.md");
        std::fs::write(&file, "old").unwrap();

        atomic_write(&file, "new content").unwrap();

        assert_eq!(std::fs::read_to_string(&file).unwrap(), "new content");
        let names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["issue.md"]);
    }

    #[test]
    fn a_failed_write_leaves_no_temp_file() {
        let scratch = tempfile::tempdir().unwrap();
        let dir = scratch.path();
        std::fs::create_dir_all(dir.join("taken")).unwrap();

        assert!(atomic_write(&dir.join("taken"), "over a folder").is_err());

        let names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["taken"]);
    }
}
