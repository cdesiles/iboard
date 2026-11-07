//! File names and writes: naming new files so they stay readable and
//! unique, and writing them so a crash or a full disk never leaves one
//! half-written.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};

/// A fresh path in the temp folder, `iboard-<tag>-<pid>-<n>.<extension>`: the
/// counter keeps two files made at the same instant apart.
fn temp_path(tag: &str, extension: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "iboard-{tag}-{}-{n}.{extension}",
        std::process::id()
    ))
}

/// The temp files still there; `None` once the process is exiting.
static TEMP_FILES: Mutex<Option<Vec<PathBuf>>> = Mutex::new(Some(Vec::new()));

/// A file in the temp folder, removed when dropped, or when the process
/// exits while the thread holding it is still at work.
#[derive(Debug)]
pub struct TempFile(PathBuf);

impl TempFile {
    /// Writes `contents` to a fresh [`temp_path`].
    pub fn write(tag: &str, extension: &str, contents: &str) -> std::io::Result<Self> {
        #[cfg(unix)]
        {
            static AT_EXIT: std::sync::Once = std::sync::Once::new();
            // SAFETY: `remove_temp_files` is a plain function that never unwinds.
            AT_EXIT.call_once(|| unsafe {
                libc::atexit(remove_temp_files);
            });
        }
        let path = temp_path(tag, extension);
        let mut files = TEMP_FILES.lock().unwrap_or_else(PoisonError::into_inner);
        let files = files
            .as_mut()
            .ok_or_else(|| std::io::Error::other("the process is exiting"))?;
        std::fs::write(&path, contents)?;
        files.push(path.clone());
        Ok(Self(path))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let mut files = TEMP_FILES.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(files) = files.as_mut() {
            files.retain(|path| *path != self.0);
            let _ = std::fs::remove_file(&self.0);
        }
    }
}

#[cfg(unix)]
extern "C" fn remove_temp_files() {
    let mut files = TEMP_FILES.lock().unwrap_or_else(PoisonError::into_inner);
    for path in files.take().unwrap_or_default() {
        let _ = std::fs::remove_file(path);
    }
}

/// Longest file name the editor accepts, in bytes, including its extension.
/// 255 bytes is the limit of APFS, ext4 and NTFS: longer names fail to write.
pub const MAX_FILE_NAME_BYTES: usize = 255;

/// Longest slug the composer derives from a title.
const MAX_SLUG_LEN: usize = 60;

/// The file name part of a path, as UTF-8 (empty when there is none).
pub fn file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string()
}

/// Whether `path` ends in `.md` or `.markdown`, in any case: the files
/// discovery reads as issues.
pub fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("markdown"))
        .unwrap_or(false)
}

/// Makes sure a user-typed file name keeps a markdown extension so the issue
/// stays visible to discovery, and caps it at [`MAX_FILE_NAME_BYTES`] so the
/// filesystem accepts it. A missing or non-markdown extension is replaced with
/// `.md`.
pub fn normalize_file_name(input: &str) -> String {
    let trimmed = input.trim();
    let (stem, ext) = if is_markdown(Path::new(trimmed)) {
        let ext = Path::new(trimmed)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("md");
        let stem = trimmed.strip_suffix(&format!(".{ext}")).unwrap_or(trimmed);
        (stem.to_string(), ext.to_string())
    } else {
        (trimmed.to_string(), "md".to_string())
    };
    let budget = MAX_FILE_NAME_BYTES.saturating_sub(ext.len() + 1);
    format!("{}.{ext}", truncate_stem(&stem, budget))
}

/// Cuts `stem` to at most `max_bytes`, on a character boundary, without leaving
/// a trailing hyphen or space.
fn truncate_stem(stem: &str, max_bytes: usize) -> String {
    if stem.len() <= max_bytes {
        return stem.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !stem.is_char_boundary(end) {
        end -= 1;
    }
    stem[..end].trim_end_matches(['-', ' ']).to_string()
}

/// A file name in `dir` that does not exist yet: `name`, then `name-2`, ...
pub fn unique_name(dir: &Path, name: &str) -> String {
    if !dir.join(name).exists() {
        return name.to_string();
    }
    let (stem, ext) = match Path::new(name).extension().and_then(|e| e.to_str()) {
        Some(ext) => (
            name.strip_suffix(&format!(".{ext}")).unwrap_or(name),
            format!(".{ext}"),
        ),
        None => (name, String::new()),
    };
    let mut n = 2;
    loop {
        let candidate = format!("{stem}-{n}{ext}");
        if !dir.join(&candidate).exists() {
            return candidate;
        }
        n += 1;
    }
}

/// Turns a title into a file stem: lowercase ASCII words joined by hyphens, at
/// most 60 characters. A title with no ASCII letters or digits gives `issue`.
pub fn slugify(title: &str) -> String {
    let mut slug = String::new();
    for word in title
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
    {
        let word = word.to_ascii_lowercase();
        let extra = if slug.is_empty() {
            word.len()
        } else {
            word.len() + 1
        };
        if slug.len() + extra > MAX_SLUG_LEN {
            if slug.is_empty() {
                slug = word[..MAX_SLUG_LEN].to_string();
            }
            break;
        }
        if !slug.is_empty() {
            slug.push('-');
        }
        slug.push_str(&word);
    }
    if slug.is_empty() {
        "issue".to_string()
    } else {
        slug
    }
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

/// What [`write_creating`] could not do: create the folder, or write the file.
#[derive(Debug)]
pub enum WriteError {
    Folder(PathBuf, std::io::Error),
    File(std::io::Error),
}

impl From<WriteError> for std::io::Error {
    fn from(err: WriteError) -> Self {
        match err {
            WriteError::Folder(_, err) | WriteError::File(err) => err,
        }
    }
}

/// [`atomic_write`], creating the folder `path` goes in first. Issue saves
/// keep to `atomic_write`, so a column deleted meanwhile is not brought back.
pub fn write_creating(path: &Path, contents: impl AsRef<[u8]>) -> Result<(), WriteError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|err| WriteError::Folder(dir.to_path_buf(), err))?;
    }
    atomic_write(path, contents).map_err(WriteError::File)
}

/// Creates `dir` in its parent, never the parent itself, so work still
/// running in a folder deleted meanwhile does not bring it back.
pub fn create_last_dir(dir: &Path) -> std::io::Result<()> {
    match std::fs::create_dir(dir) {
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        result => result,
    }
}

/// [`write_creating`], creating only the folder `path` goes in: see
/// [`create_last_dir`].
pub fn write_creating_last(path: &Path, contents: impl AsRef<[u8]>) -> Result<(), WriteError> {
    if let Some(dir) = path.parent() {
        create_last_dir(dir).map_err(|err| WriteError::Folder(dir.to_path_buf(), err))?;
    }
    atomic_write(path, contents).map_err(WriteError::File)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_keep_a_markdown_extension() {
        assert_eq!(normalize_file_name("notes.md"), "notes.md");
        assert_eq!(normalize_file_name("notes.markdown"), "notes.markdown");
        assert_eq!(normalize_file_name("  notes.MD  "), "notes.MD");
        assert_eq!(normalize_file_name("notes"), "notes.md");
    }

    #[test]
    fn slugs_are_lowercase_ascii_words_joined_by_hyphens() {
        assert_eq!(slugify("Fix login redirect"), "fix-login-redirect");
        assert_eq!(
            slugify("  AI assist: refine an issue!  "),
            "ai-assist-refine-an-issue"
        );
        assert_eq!(slugify("Café au lait"), "caf-au-lait");
        assert_eq!(slugify("¿¡!?"), "issue");
        let long = slugify(&"word ".repeat(30));
        assert!(long.len() <= MAX_SLUG_LEN, "{long} is too long");
        assert!(!long.ends_with('-'));
    }

    #[test]
    fn long_issue_names_are_capped_and_keep_their_extension() {
        let long = "word ".repeat(80);
        let name = normalize_file_name(&long);
        assert!(
            name.len() <= MAX_FILE_NAME_BYTES,
            "{name} is too long ({} bytes)",
            name.len()
        );
        assert!(name.ends_with(".md"));

        let long_markdown = format!("{}.markdown", "b".repeat(400));
        let name = normalize_file_name(&long_markdown);
        assert!(name.len() <= MAX_FILE_NAME_BYTES);
        assert!(name.ends_with(".markdown"));

        // Multi-byte characters are cut on a character boundary.
        let accented = format!("{}.md", "é".repeat(400));
        let name = normalize_file_name(&accented);
        assert!(name.len() <= MAX_FILE_NAME_BYTES);
        assert!(name.ends_with(".md"));
    }

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

    #[test]
    fn only_write_creating_makes_a_missing_folder() {
        let scratch = tempfile::tempdir().unwrap();
        let dir = scratch.path();
        let file = dir.join("gone/issue.md");
        assert!(atomic_write(&file, "text").is_err());
        assert!(!dir.join("gone").exists());

        write_creating(&file, "text").unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "text");

        std::fs::write(dir.join("file"), "").unwrap();
        assert!(matches!(
            write_creating(&dir.join("file/sub/issue.md"), "text"),
            Err(WriteError::Folder(folder, _)) if folder == dir.join("file/sub")
        ));
    }

    #[test]
    fn write_creating_last_never_brings_a_deleted_folder_back() {
        let scratch = tempfile::tempdir().unwrap();
        let issues = scratch.path().join("issues");
        let log = issues.join(".builds/a.log");
        assert!(write_creating_last(&log, "text").is_err());
        assert!(!issues.exists());

        std::fs::create_dir(&issues).unwrap();
        write_creating_last(&log, "text").unwrap();
        write_creating_last(&log, "again").unwrap();
        assert_eq!(std::fs::read_to_string(&log).unwrap(), "again");
    }

    #[test]
    fn a_temp_file_goes_when_dropped() {
        let file = TempFile::write("test", "md", "prompt").unwrap();
        let path = file.path().to_path_buf();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "prompt");
        drop(file);
        assert!(!path.exists());
    }
}
