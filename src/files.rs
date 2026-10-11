//! File names and writes: naming new files so they stay readable and
//! unique, and writing them so a crash or a full disk never leaves one
//! half-written.

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

/// Makes sure a user-typed file name keeps a markdown extension so the issue
/// stays visible to discovery, and caps it at [`MAX_FILE_NAME_BYTES`] so the
/// filesystem accepts it. A missing or non-markdown extension is replaced with
/// `.md`.
pub fn normalize_file_name(input: &str) -> String {
    let trimmed = input.trim();
    let is_markdown = Path::new(trimmed)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("markdown"))
        .unwrap_or(false);
    let (stem, ext) = if is_markdown {
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
}
