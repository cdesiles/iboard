//! The font files a design system ships, read for their family and weight and
//! embedded as `@font-face` so a page shows the real face with no network.

use std::path::PathBuf;

use super::DesignSystem;

/// One shipped font file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontFace {
    pub path: PathBuf,
    pub file: String,
    pub family: String,
    pub style: String,
    pub weight: u16,
    pub italic: bool,
}

/// Every font file among the system's assets, by family then weight.
pub fn faces(system: &DesignSystem) -> Vec<FontFace> {
    let mut out: Vec<FontFace> = system
        .assets()
        .into_iter()
        .filter_map(|asset| {
            let path = asset.path?;
            format(&asset.name)?;
            let bytes = std::fs::read(&path).ok()?;
            let (family, style, weight, italic) =
                sfnt_names(&bytes).unwrap_or_else(|| from_file_name(&asset.name));
            Some(FontFace {
                path,
                file: asset.name,
                family,
                style,
                weight,
                italic,
            })
        })
        .collect();
    out.sort_by(|a, b| (&a.family, a.weight, a.italic).cmp(&(&b.family, b.weight, b.italic)));
    out
}

/// The `@font-face` rules for `faces`, each file inlined as a data URI.
pub fn font_face_css(faces: &[FontFace]) -> String {
    let mut out = String::new();
    for face in faces {
        let (Some(format), Ok(bytes)) = (format(&face.file), std::fs::read(&face.path)) else {
            continue;
        };
        out.push_str(&format!(
            "@font-face{{font-family:\"{family}\";font-weight:{weight};font-style:{style};\
             src:url(data:font/{format};base64,{data}) format(\"{css}\")}}\n",
            family = face.family.replace('"', ""),
            weight = face.weight,
            style = if face.italic { "italic" } else { "normal" },
            data = base64(&bytes),
            css = if format == "ttf" {
                "truetype"
            } else if format == "otf" {
                "opentype"
            } else {
                format
            },
        ));
    }
    out
}

/// The first face of a CSS family stack, unquoted: `IBM Plex Sans`.
pub fn primary(stack: &str) -> String {
    stack
        .split(',')
        .next()
        .unwrap_or_default()
        .trim()
        .trim_matches(['"', '\''])
        .to_string()
}

fn format(name: &str) -> Option<&'static str> {
    let ext = name.rsplit_once('.')?.1.to_ascii_lowercase();
    match ext.as_str() {
        "ttf" => Some("ttf"),
        "otf" => Some("otf"),
        "woff" => Some("woff"),
        "woff2" => Some("woff2"),
        _ => None,
    }
}

/// Family, style, weight and italic from a TrueType/OpenType `name` and `OS/2`.
fn sfnt_names(bytes: &[u8]) -> Option<(String, String, u16, bool)> {
    let u16_at = |at: usize| Some(u16::from_be_bytes([*bytes.get(at)?, *bytes.get(at + 1)?]));
    let u32_at = |at: usize| Some(((u16_at(at)? as u32) << 16) | u16_at(at + 2)? as u32);
    let table = |tag: &[u8; 4]| {
        (0..u16_at(4)? as usize)
            .map(|i| 12 + i * 16)
            .find(|&at| bytes.get(at..at + 4) == Some(tag))
            .and_then(|at| u32_at(at + 8))
            .map(|offset| offset as usize)
    };

    let name = table(b"name")?;
    let strings = name + u16_at(name + 4)? as usize;
    let mut found: [Option<String>; 4] = Default::default();
    for i in 0..u16_at(name + 2)? as usize {
        let at = name + 6 + i * 12;
        let (platform, language, id) = (u16_at(at)?, u16_at(at + 4)?, u16_at(at + 6)?);
        let slot = match id {
            1 => 0,
            2 => 1,
            16 => 2,
            17 => 3,
            _ => continue,
        };
        let start = strings + u16_at(at + 10)? as usize;
        let raw = bytes.get(start..start + u16_at(at + 8)? as usize)?;
        let text = match (platform, language) {
            (3, 0x409) | (0, _) => String::from_utf16_lossy(
                &raw.as_chunks::<2>()
                    .0
                    .iter()
                    .map(|&pair| u16::from_be_bytes(pair))
                    .collect::<Vec<_>>(),
            ),
            (1, 0) => raw.iter().map(|&b| b as char).collect(),
            _ => continue,
        };
        found[slot].get_or_insert(text);
    }
    let [family, style, typo_family, typo_style] = found;
    let family = typo_family.or(family)?;
    let style = typo_style.or(style).unwrap_or_else(|| "Regular".into());

    let (weight, italic) = match table(b"OS/2") {
        Some(os2) => (u16_at(os2 + 4)?, u16_at(os2 + 62)? & 1 == 1),
        None => (weight_of(&style), style.to_lowercase().contains("italic")),
    };
    Some((family, style, weight, italic))
}

/// A best guess for WOFF files and unreadable tables: `Inter-SemiBoldItalic.woff2`.
fn from_file_name(name: &str) -> (String, String, u16, bool) {
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    let (family, style) = stem.split_once('-').unwrap_or((stem, "Regular"));
    (
        family.to_string(),
        style.to_string(),
        weight_of(style),
        style.to_lowercase().contains("italic"),
    )
}

fn weight_of(style: &str) -> u16 {
    let style = style.to_lowercase().replace([' ', '-', '_'], "");
    [
        ("thin", 100),
        ("extralight", 200),
        ("ultralight", 200),
        ("light", 300),
        ("medium", 500),
        ("semibold", 600),
        ("demibold", 600),
        ("extrabold", 800),
        ("ultrabold", 800),
        ("bold", 700),
        ("black", 900),
        ("heavy", 900),
    ]
    .into_iter()
    .find(|(key, _)| style.contains(key))
    .map_or(400, |(_, weight)| weight)
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &b)| n | (b as u32) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_pads_like_the_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn the_shipped_plex_files_name_their_family_and_weight() {
        let bytes = include_bytes!("../../assets/fonts/IBMPlexSans-SemiBold.ttf");
        let (family, style, weight, italic) = sfnt_names(bytes).expect("name table");
        assert_eq!(family, "IBM Plex Sans");
        assert_eq!(style, "SemiBold");
        assert_eq!(weight, 600);
        assert!(!italic);
    }

    #[test]
    fn a_woff_name_falls_back_to_its_file_name() {
        assert_eq!(
            from_file_name("Inter-SemiBoldItalic.woff2"),
            ("Inter".into(), "SemiBoldItalic".into(), 600, true)
        );
        assert_eq!(primary("\"IBM Plex Sans\", system-ui"), "IBM Plex Sans");
    }
}
