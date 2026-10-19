//! Images pasted into an issue. They live in `issues/.assets/<slug>/NN.png`,
//! outside the columns, so moving or archiving an issue never breaks a link,
//! and the issue links them relatively: `![…](../.assets/<slug>/01.png)`.
//! The slug is the file name without its Ready number, as for the review
//! sidecars, so renumbering a column keeps it.

use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};

use pulldown_cmark::{Event, Options, Parser, Tag};

use crate::files;

/// The folder under the issues root that holds every issue's images.
pub const DIR: &str = ".assets";

/// The largest image accepted, before and after conversion to PNG.
pub const MAX_BYTES: usize = 10 * 1024 * 1024;

/// The folder of one issue's images.
pub fn folder(root: &Path, slug: &str) -> PathBuf {
    root.join(DIR).join(slug)
}

/// The link from an issue in any column to one of its images.
pub fn link(slug: &str, file: &str) -> String {
    format!("../{DIR}/{slug}/{file}")
}

/// The first `NN.png` after the highest number already in `dir`.
pub fn next_name(dir: &Path) -> String {
    let last = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.strip_suffix(".png")?.parse::<u32>().ok()
        })
        .max()
        .unwrap_or(0);
    format!("{:02}.png", last + 1)
}

/// Writes `png` as the issue's next image and returns the markdown that shows
/// it.
pub fn add(root: &Path, slug: &str, png: &[u8]) -> std::io::Result<String> {
    let dir = folder(root, slug);
    std::fs::create_dir_all(&dir)?;
    let name = next_name(&dir);
    files::atomic_write(&dir.join(&name), png)?;
    Ok(format!("![screenshot]({})", link(slug, &name)))
}

/// An inline image link in an issue's markdown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageLink {
    pub url: String,
    /// The whole `![alt](url)`.
    pub span: Range<usize>,
    /// The url alone.
    pub url_at: Range<usize>,
}

/// The inline image links of `text`, in order. Code fences and inline code
/// hold none: the parser reads them as code.
pub fn image_links(text: &str) -> Vec<ImageLink> {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES;
    Parser::new_ext(text, options)
        .into_offset_iter()
        .filter_map(|(event, span)| {
            let Event::Start(Tag::Image { dest_url, .. }) = event else {
                return None;
            };
            let at = span.start + text[span.clone()].rfind(dest_url.as_ref())?;
            Some(ImageLink {
                url: dest_url.to_string(),
                url_at: at..at + dest_url.len(),
                span,
            })
        })
        .collect()
}

/// The file a relative image link of `issue` points at; `None` for a remote
/// or absolute one.
pub fn local_file(issue: &Path, url: &str) -> Option<PathBuf> {
    if url.is_empty() || url.contains("://") || url.starts_with('/') || url.starts_with("data:") {
        return None;
    }
    Some(issue.parent()?.join(url))
}

/// Whether the images follow the issue or are copied for another one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Carry {
    Move,
    Copy,
}

/// Points the links of `text` into `from`'s images at `to`'s, moving or
/// copying the files. A move with no `to` folder yet takes the whole folder.
/// A link whose file is missing stays as it was.
pub fn relink(
    root: &Path,
    text: &str,
    from: &str,
    to: &str,
    carry: Carry,
) -> std::io::Result<String> {
    if from == to {
        return Ok(text.to_string());
    }
    let (source, target) = (folder(root, from), folder(root, to));
    let prefix = link(from, "");
    let links: Vec<(Range<usize>, String)> = image_links(text)
        .into_iter()
        .filter_map(|image| {
            let file = image.url.strip_prefix(&prefix)?;
            let plain = !file.is_empty() && !file.contains(['/', '\\']);
            (plain && source.join(file).is_file()).then(|| (image.url_at, file.to_string()))
        })
        .collect();
    let mut names: HashMap<String, String> = HashMap::new();
    if carry == Carry::Move && source.is_dir() && !target.exists() {
        std::fs::rename(&source, &target)?;
        names.extend(links.iter().map(|(_, file)| (file.clone(), file.clone())));
    } else {
        for (_, file) in &links {
            if names.contains_key(file) {
                continue;
            }
            std::fs::create_dir_all(&target)?;
            let name = if target.join(file).exists() {
                next_name(&target)
            } else {
                file.clone()
            };
            match carry {
                Carry::Move => std::fs::rename(source.join(file), target.join(&name))?,
                Carry::Copy => {
                    std::fs::copy(source.join(file), target.join(&name))?;
                }
            }
            names.insert(file.clone(), name);
        }
        if carry == Carry::Move {
            let _ = std::fs::remove_dir(&source);
        }
    }
    let mut out = text.to_string();
    for (range, file) in links.iter().rev() {
        out.replace_range(range.clone(), &link(to, &names[file]));
    }
    Ok(out)
}

/// Why an image of `size` bytes is refused.
pub fn too_big(size: u64) -> String {
    format!(
        "The image is {:.1} MB: images over 10 MB can't be added.",
        size as f64 / (1024.0 * 1024.0)
    )
}

/// Turns pasted or dropped image bytes into the PNG kept on disk: PNG as is,
/// JPEG and TIFF converted, anything else or anything over [`MAX_BYTES`]
/// refused with the reason.
#[cfg(feature = "gui")]
pub fn to_png(bytes: &[u8]) -> Result<Vec<u8>, String> {
    use image::ImageFormat;
    if bytes.len() > MAX_BYTES {
        return Err(too_big(bytes.len() as u64));
    }
    let png = match image::guess_format(bytes) {
        Ok(ImageFormat::Png) => bytes.to_vec(),
        Ok(format @ (ImageFormat::Jpeg | ImageFormat::Tiff)) => {
            let decoded = image::load_from_memory_with_format(bytes, format)
                .map_err(|err| format!("Couldn't read the image: {err}"))?;
            let mut out = std::io::Cursor::new(Vec::new());
            decoded
                .write_to(&mut out, ImageFormat::Png)
                .map_err(|err| format!("Couldn't convert the image to PNG: {err}"))?;
            out.into_inner()
        }
        _ => return Err("Only PNG, JPEG and TIFF images can be added to an issue.".to_string()),
    };
    if png.len() > MAX_BYTES {
        return Err(too_big(png.len() as u64));
    }
    Ok(png)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn images_are_numbered_per_issue() {
        let scratch = tempfile::tempdir().unwrap();
        let root = scratch.path();
        assert_eq!(
            add(root, "crash", b"one").unwrap(),
            "![screenshot](../.assets/crash/01.png)"
        );
        assert_eq!(
            add(root, "crash", b"two").unwrap(),
            "![screenshot](../.assets/crash/02.png)"
        );
        assert_eq!(
            add(root, "other", b"three").unwrap(),
            "![screenshot](../.assets/other/01.png)"
        );
        assert_eq!(
            std::fs::read(root.join(".assets/crash/02.png")).unwrap(),
            b"two"
        );
    }

    #[test]
    fn image_links_skip_code() {
        let text =
            "![a](x.png)\n\n```\n![b](y.png)\n```\n\nSee `![c](z.png)` and ![d](w.png \"t\").\n";
        let links = image_links(text);
        let urls: Vec<&str> = links.iter().map(|l| l.url.as_str()).collect();
        assert_eq!(urls, ["x.png", "w.png"]);
        assert_eq!(&text[links[1].url_at.clone()], "w.png");
        assert_eq!(&text[links[0].span.clone()], "![a](x.png)");
    }

    #[test]
    fn only_relative_links_are_local() {
        let issue = Path::new("/p/issues/todo/a.md");
        assert_eq!(
            local_file(issue, "../.assets/a/01.png"),
            Some(PathBuf::from("/p/issues/todo/../.assets/a/01.png"))
        );
        assert_eq!(local_file(issue, "https://x.org/a.png"), None);
        assert_eq!(local_file(issue, "/tmp/a.png"), None);
    }

    #[test]
    fn a_move_takes_the_folder_and_leaves_code_and_broken_links() {
        let scratch = tempfile::tempdir().unwrap();
        let root = scratch.path();
        add(root, "old", b"png").unwrap();
        let text = "![s](../.assets/old/01.png)\n\n```\n![s](../.assets/old/01.png)\n```\n\n![gone](../.assets/old/07.png)\n";
        let moved = relink(root, text, "old", "new", Carry::Move).unwrap();
        assert_eq!(
            moved,
            "![s](../.assets/new/01.png)\n\n```\n![s](../.assets/old/01.png)\n```\n\n![gone](../.assets/old/07.png)\n"
        );
        assert!(root.join(".assets/new/01.png").is_file());
        assert!(!root.join(".assets/old").exists());
    }

    #[test]
    fn a_copy_never_overwrites_the_target() {
        let scratch = tempfile::tempdir().unwrap();
        let root = scratch.path();
        add(root, "a", b"first").unwrap();
        add(root, "b", b"theirs").unwrap();
        let copied = relink(
            root,
            "![x](../.assets/a/01.png) ![y](../.assets/a/01.png)",
            "a",
            "b",
            Carry::Copy,
        )
        .unwrap();
        assert_eq!(
            copied,
            "![x](../.assets/b/02.png) ![y](../.assets/b/02.png)"
        );
        assert_eq!(
            std::fs::read(root.join(".assets/b/01.png")).unwrap(),
            b"theirs"
        );
        assert_eq!(
            std::fs::read(root.join(".assets/b/02.png")).unwrap(),
            b"first"
        );
        assert!(
            root.join(".assets/a/01.png").is_file(),
            "a copy keeps the source"
        );
    }

    #[cfg(feature = "gui")]
    #[test]
    fn jpeg_turns_into_png_and_other_formats_are_refused() {
        let pixels = image::RgbImage::from_pixel(3, 3, image::Rgb([10, 20, 30]));
        let mut jpeg = std::io::Cursor::new(Vec::new());
        pixels
            .write_to(&mut jpeg, image::ImageFormat::Jpeg)
            .unwrap();
        let png = to_png(jpeg.get_ref()).unwrap();
        assert_eq!(image::guess_format(&png).unwrap(), image::ImageFormat::Png);
        assert!(to_png(b"GIF89a....").is_err());
        assert!(
            to_png(&vec![0; MAX_BYTES + 1])
                .unwrap_err()
                .contains("10 MB")
        );
    }
}
