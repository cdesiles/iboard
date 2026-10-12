//! Reading `design/`: locating it, its JSON files and its component folders.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::card::parse_card_meta;
use super::model::{
    CardMeta, Component, DesignSystem, LastChange, Meta, Theme, Token, TokenValue, Tokens,
    TypeGroup, TypeStyle,
};

impl DesignSystem {
    /// Load from a project root, a `design/` directory, or a `design-system.json`.
    pub fn load(path: &Path) -> Result<Self, String> {
        let root = locate_root(path)?;

        let meta_raw: RawMeta = read_json(&root.join("design-system.json"))?;
        let tokens_raw: RawTokens = read_json(&root.join("tokens.json"))?;

        let meta = Meta {
            title: meta_raw.title,
            namespace: meta_raw.namespace,
            version: meta_raw.v,
            readme: meta_raw.docs.readme,
            last_change: meta_raw.last_change.map(|lc| LastChange {
                by: lc.by,
                at: lc.at,
                via: lc.via,
                note: lc.note,
            }),
        };

        let mut warnings = Vec::new();
        let tokens = convert_tokens(tokens_raw);
        let components = load_components(&root, &mut warnings)?;
        let asset_groups = meta_raw
            .asset_groups
            .iter()
            .map(|(group, value)| (group.clone(), asset_names(value)))
            .collect();

        Ok(Self {
            root,
            meta,
            tokens,
            components,
            asset_groups,
            warnings,
        })
    }

    /// Find a design system from any starting folder, walking up to the root.
    /// The first folder that has one decides: a system that fails to load is
    /// reported, never skipped for one further up.
    pub fn discover(start: &Path) -> Result<Self, String> {
        let mut dir = std::fs::canonicalize(start).unwrap_or_else(|_| start.to_path_buf());
        loop {
            if locate_root(&dir).is_ok() {
                return Self::load(&dir);
            }
            match dir.parent() {
                Some(parent) => dir = parent.to_path_buf(),
                None => break,
            }
        }
        Err(format!(
            "No design system found from {} (looked for design/design-system.json).",
            start.display()
        ))
    }
}

/// Resolve the `design/` directory from a path that may be a project root, the
/// design directory itself, or the `design-system.json` file.
fn locate_root(path: &Path) -> Result<PathBuf, String> {
    if path.is_file() {
        if path.file_name().and_then(|name| name.to_str()) == Some("design-system.json") {
            return path
                .parent()
                .map(Path::to_path_buf)
                .ok_or_else(|| format!("{} has no parent directory", path.display()));
        }
        return Err(format!("{} is not a design-system.json", path.display()));
    }
    if path.join("design-system.json").is_file() {
        return Ok(path.to_path_buf());
    }
    let nested = path.join("design");
    if nested.join("design-system.json").is_file() {
        return Ok(nested);
    }
    Err(format!(
        "No design system at {} (looked for design-system.json and design/design-system.json).",
        path.display()
    ))
}

/// Scan `components/*/` into a sorted list, warning about what is missing.
fn load_components(root: &Path, warnings: &mut Vec<String>) -> Result<Vec<Component>, String> {
    let dir = root.join("components");
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut components = Vec::new();
    let entries =
        std::fs::read_dir(&dir).map_err(|err| format!("Couldn't read {}: {err}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|err| format!("Couldn't read {}: {err}", dir.display()))?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let readme_path = path.join("README.md");
        let readme = if readme_path.is_file() {
            Some(
                std::fs::read_to_string(&readme_path)
                    .map_err(|err| format!("Couldn't read {}: {err}", readme_path.display()))?,
            )
        } else {
            warnings.push(format!("components/{name}/ has no README.md"));
            None
        };
        let has_preview = path.join("preview.html").is_file();
        if !has_preview {
            warnings.push(format!("components/{name}/ has no preview.html"));
        }
        let card = if has_preview {
            std::fs::read_to_string(path.join("preview.html"))
                .map(|text| parse_card_meta(&text))
                .unwrap_or_default()
        } else {
            CardMeta::default()
        };
        components.push(Component {
            name,
            dir: path,
            readme,
            has_preview,
            card,
        });
    }
    components.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(components)
}

/// Every string leaf of an `assetGroups` value, whatever shape it takes.
fn asset_names(value: &serde_json::Value) -> Vec<String> {
    match value {
        serde_json::Value::Array(items) => items.iter().flat_map(asset_names).collect(),
        serde_json::Value::Object(map) => map.values().flat_map(asset_names).collect(),
        serde_json::Value::String(name) => vec![name.clone()],
        _ => Vec::new(),
    }
}

pub(super) fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|err| format!("Couldn't read {}: {err}", path.display()))?;
    serde_json::from_str(&text).map_err(|err| format!("Couldn't parse {}: {err}", path.display()))
}

fn convert_tokens(raw: RawTokens) -> Tokens {
    Tokens {
        name: raw.name,
        version: raw.version,
        themes: raw
            .color
            .themes
            .into_iter()
            .map(|theme| Theme {
                id: theme.id,
                name: theme.name,
            })
            .collect(),
        color: parse_tokens(raw.color.tokens),
        spacing: parse_tokens(raw.spacing.tokens),
        radius: parse_tokens(raw.radius.tokens),
        size: parse_tokens(raw.size.tokens),
        size_note: raw.size.note,
        shadow: parse_tokens(raw.shadow.tokens),
        families: raw.type_.families,
        type_groups: raw
            .type_
            .groups
            .into_iter()
            .map(|group| TypeGroup {
                name: group.name,
                family: group.family,
                styles: group
                    .styles
                    .into_iter()
                    .map(|style| TypeStyle {
                        name: style.name,
                        font_size: style.font_size,
                        line_height: style.line_height,
                        weight: style.weight,
                        letter_spacing: style.letter_spacing,
                        sample: style.sample,
                        usage: style.usage,
                    })
                    .collect(),
            })
            .collect(),
    }
}

fn parse_tokens(raw: Vec<RawToken>) -> Vec<Token> {
    raw.into_iter()
        .map(|token| Token {
            name: token.name,
            value: token_value(&token.value),
            usage: token.usage,
        })
        .collect()
}

fn token_value(raw: &serde_json::Value) -> TokenValue {
    match raw {
        serde_json::Value::String(value) => TokenValue::Plain(value.clone()),
        serde_json::Value::Object(map) => {
            let mut values = BTreeMap::new();
            for (key, value) in map {
                if let Some(text) = value.as_str() {
                    values.insert(key.clone(), text.to_string());
                }
            }
            if values.is_empty() {
                TokenValue::Plain(raw.to_string())
            } else {
                TokenValue::Themed(values)
            }
        }
        other => TokenValue::Plain(other.to_string()),
    }
}

#[derive(Debug, Deserialize, Default)]
struct RawMeta {
    #[serde(default)]
    v: u64,
    #[serde(default)]
    title: String,
    #[serde(default)]
    namespace: String,
    #[serde(default)]
    docs: RawDocs,
    #[serde(default, rename = "assetGroups")]
    asset_groups: BTreeMap<String, serde_json::Value>,
    #[serde(default, rename = "lastChange")]
    last_change: Option<RawLastChange>,
}

#[derive(Debug, Deserialize, Default)]
struct RawDocs {
    #[serde(default)]
    readme: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct RawLastChange {
    #[serde(default)]
    by: Option<String>,
    #[serde(default)]
    at: Option<String>,
    #[serde(default)]
    via: Option<String>,
    #[serde(default)]
    note: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct RawTokens {
    #[serde(default)]
    name: String,
    #[serde(default)]
    version: u64,
    #[serde(default)]
    color: RawColor,
    #[serde(default, rename = "type")]
    type_: RawType,
    #[serde(default)]
    spacing: RawSection,
    #[serde(default)]
    radius: RawSection,
    #[serde(default)]
    size: RawSection,
    #[serde(default)]
    shadow: RawSection,
}

#[derive(Debug, Deserialize, Default)]
struct RawColor {
    #[serde(default)]
    themes: Vec<RawTheme>,
    #[serde(default)]
    tokens: Vec<RawToken>,
}

#[derive(Debug, Deserialize, Default)]
struct RawTheme {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
}

#[derive(Debug, Deserialize, Default)]
struct RawSection {
    #[serde(default)]
    tokens: Vec<RawToken>,
    #[serde(default)]
    note: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct RawToken {
    #[serde(default)]
    name: String,
    #[serde(default)]
    value: serde_json::Value,
    #[serde(default)]
    usage: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct RawType {
    #[serde(default)]
    families: BTreeMap<String, String>,
    #[serde(default)]
    groups: Vec<RawTypeGroup>,
}

#[derive(Debug, Deserialize, Default)]
struct RawTypeGroup {
    #[serde(default)]
    name: String,
    #[serde(default)]
    family: String,
    #[serde(default)]
    styles: Vec<RawTypeStyle>,
}

#[derive(Debug, Deserialize, Default)]
struct RawTypeStyle {
    #[serde(default)]
    name: String,
    #[serde(default, rename = "fontSize")]
    font_size: Option<String>,
    #[serde(default, rename = "lineHeight")]
    line_height: Option<String>,
    #[serde(default, rename = "fontWeight")]
    weight: Option<u32>,
    #[serde(default, rename = "letterSpacing")]
    letter_spacing: Option<String>,
    #[serde(default)]
    sample: Option<String>,
    #[serde(default)]
    usage: Option<String>,
}
