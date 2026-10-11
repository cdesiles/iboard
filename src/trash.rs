//! Deleting an issue to a Trash it can be restored from: the macOS Trash, the
//! freedesktop one on Linux and the BSDs, or a folder of its own for tests.

use std::path::{Path, PathBuf};

/// Where a deleted issue goes, as the confirmation dialog tells the user.
#[cfg(target_os = "macos")]
pub const CONSEQUENCE: &str =
    "The file is moved to the macOS Trash. You can restore it from there.";
#[cfg(not(target_os = "macos"))]
pub const CONSEQUENCE: &str = "The file is moved to the Trash. You can restore it from there.";

/// The Trash a deleted file goes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trash {
    /// The user's Trash.
    System,
    /// A freedesktop-style Trash in this folder, which never fills the user's.
    Folder(PathBuf),
}

impl Trash {
    /// Moves `path` to this Trash, and says where it went when the Trash can.
    pub fn delete(&self, path: &Path) -> std::io::Result<Option<PathBuf>> {
        match self {
            Trash::System => system(path),
            Trash::Folder(folder) => into_folder(path, folder).map(Some),
        }
    }
}

/// Moves a file [`Trash::delete`] put at `trashed` back to `path`, with the
/// `.trashinfo` a freedesktop Trash kept beside it.
pub fn restore(trashed: &Path, path: &Path) -> std::io::Result<()> {
    std::fs::rename(trashed, path)?;
    if let (Some(files), Some(name)) = (trashed.parent(), trashed.file_name())
        && let Some(trash) = files.parent().filter(|_| files.ends_with("files"))
    {
        let mut info = name.to_os_string();
        info.push(".trashinfo");
        let _ = std::fs::remove_file(trash.join("info").join(info));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn system(path: &Path) -> std::io::Result<Option<PathBuf>> {
    use objc2::rc::Retained;
    use objc2_foundation::{NSFileManager, NSString, NSURL};

    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    let mut resulting: Option<Retained<NSURL>> = None;
    NSFileManager::defaultManager()
        .trashItemAtURL_resultingItemURL_error(&url, Some(&mut resulting))
        .map_err(|err| std::io::Error::other(err.localizedDescription().to_string()))?;
    Ok(resulting
        .and_then(|url| url.path())
        .map(|path| PathBuf::from(path.to_string())))
}

/// The freedesktop Trash: `$XDG_DATA_HOME/Trash`, or `~/.local/share/Trash`.
#[cfg(not(target_os = "macos"))]
fn system(path: &Path) -> std::io::Result<Option<PathBuf>> {
    let data = crate::settings::dir_from_env("XDG_DATA_HOME", ".local/share")
        .ok_or_else(|| std::io::Error::other("no home folder for the Trash"))?;
    into_folder(path, &data.join("Trash")).map(Some)
}

/// Moves `path` into the freedesktop Trash at `trash`: the file under
/// `files/`, and a `.trashinfo` under `info/` so file managers can restore
/// it. Never overwrites: a second `issue.md` becomes `issue.2.md`.
fn into_folder(path: &Path, trash: &Path) -> std::io::Result<PathBuf> {
    let (files, info) = (trash.join("files"), trash.join("info"));
    std::fs::create_dir_all(&files)?;
    std::fs::create_dir_all(&info)?;
    let original = std::fs::canonicalize(path)?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "issue.md".to_string());
    let (stem, ext) = match name.rsplit_once('.') {
        Some((stem, ext)) => (stem.to_string(), format!(".{ext}")),
        None => (name.clone(), String::new()),
    };
    let mut candidate = name.clone();
    let mut n = 2;
    while files.join(&candidate).exists() || info.join(format!("{candidate}.trashinfo")).exists() {
        candidate = format!("{stem}.{n}{ext}");
        n += 1;
    }
    let encoded: String = original
        .to_string_lossy()
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect();
    let date = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S");
    std::fs::write(
        info.join(format!("{candidate}.trashinfo")),
        format!("[Trash Info]\nPath={encoded}\nDeletionDate={date}\n"),
    )?;
    let target = files.join(&candidate);
    // Another file system: copy, then remove.
    if std::fs::rename(path, &target).is_err() {
        std::fs::copy(path, &target)?;
        std::fs::remove_file(path)?;
    }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_trash_folder_keeps_a_restorable_copy() {
        let scratch = tempfile::tempdir().unwrap();
        let dir = scratch.path();
        let file = dir.join("my issue.md");
        std::fs::write(&file, "# Mine\n").unwrap();
        let trash = Trash::Folder(dir.join("Trash"));

        let first = trash.delete(&file).unwrap().unwrap();
        assert!(!file.exists());
        assert_eq!(first, dir.join("Trash/files/my issue.md"));
        let info_file = dir.join("Trash/info/my issue.md.trashinfo");
        let info = std::fs::read_to_string(&info_file).unwrap();
        assert!(info.starts_with("[Trash Info]\nPath=/"), "{info}");
        assert!(
            info.contains("my%20issue.md"),
            "the path is URL-encoded: {info}"
        );
        assert!(info.contains("DeletionDate="));

        std::fs::write(&file, "# Again\n").unwrap();
        let second = trash.delete(&file).unwrap().unwrap();
        assert_eq!(
            second,
            dir.join("Trash/files/my issue.2.md"),
            "never overwrites"
        );

        restore(&first, &file).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "# Mine\n");
        assert!(!info_file.exists(), "its .trashinfo goes with it");
    }
}
