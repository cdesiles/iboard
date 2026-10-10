//! A project's design system, read from the Claude Design shape in `design/`.
//!
//! The on-disk layout is the source of truth (see `design/README.md`):
//!
//! ```text
//! design/
//! ├── design-system.json          metadata (title, namespace, last change)
//! ├── tokens.json                 color, type, spacing, radius, size, shadow
//! └── components/<Name>/
//!     ├── README.md               what the component is for
//!     └── preview.html            a standalone rendering
//! ```
//!
//! `idesign` and iboard read it through here so they never disagree. This is
//! the canonical store: an Open Design package is an import source that maps
//! onto these types, never a second format on disk.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;

pub mod catalog;
pub mod css;
pub mod fonts;
pub mod open_design;
pub mod render;

/// Metadata from `design-system.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Meta {
    pub title: String,
    pub namespace: String,
    pub version: u64,
    pub readme: Option<String>,
    pub last_change: Option<LastChange>,
}

/// The `lastChange` block: who touched the system last, and what they said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastChange {
    pub by: Option<String>,
    pub at: Option<String>,
    pub via: Option<String>,
    pub note: Option<String>,
}

/// One design token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub name: String,
    pub value: TokenValue,
    pub usage: Option<String>,
}

/// A token value. Colors and shadows carry one value per theme; everything
/// else is a single value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenValue {
    /// One value, the same in every theme (`"2px"`).
    Plain(String),
    /// A value per theme, keyed by theme id (`light`, `dark`).
    Themed(BTreeMap<String, String>),
}

impl TokenValue {
    /// The value for `theme`, when one exists.
    pub fn theme(&self, theme: &str) -> Option<&str> {
        match self {
            TokenValue::Plain(value) => Some(value.as_str()),
            TokenValue::Themed(values) => values.get(theme).map(String::as_str),
        }
    }

    /// Every value as a string, for reference scanning and printing.
    pub fn values(&self) -> Vec<&str> {
        match self {
            TokenValue::Plain(value) => vec![value.as_str()],
            TokenValue::Themed(values) => values.values().map(String::as_str).collect(),
        }
    }
}

/// A theme the tokens are declared for (`light`, `dark`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    pub id: String,
    pub name: String,
}

/// One style in a type group (`ui-body`, `read-h1`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeStyle {
    pub name: String,
    pub font_size: Option<String>,
    pub line_height: Option<String>,
    pub weight: Option<u32>,
    pub letter_spacing: Option<String>,
    pub sample: Option<String>,
    pub usage: Option<String>,
}

/// A group of type styles sharing a family (`Interface`, `Reading`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeGroup {
    pub name: String,
    pub family: String,
    pub styles: Vec<TypeStyle>,
}

/// One component folder under `design/components/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Component {
    pub name: String,
    pub dir: PathBuf,
    pub readme: Option<String>,
    pub has_preview: bool,
    /// What the preview's `<!-- @dsCard … -->` line declares.
    pub card: CardMeta,
}

/// The card contract a preview fragment declares on its first line:
/// `<!-- @dsCard group="Controls" height=110 [subtitle="…"] [page] -->`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CardMeta {
    pub group: Option<String>,
    pub height: Option<u32>,
    pub subtitle: Option<String>,
    /// A whole screen rather than a single card; shown larger.
    pub page: bool,
}

/// One file that makes up the system on disk, for the browser's Files view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesignFile {
    /// Path relative to the design root, with `/` separators.
    pub rel: String,
    pub bytes: u64,
}

/// One asset the system ships or declares: a font, an image or a name from
/// `design-system.json`'s `assetGroups`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    /// The folder the asset was found in (`fonts`, a declared group name).
    pub group: String,
    /// The file on disk, when there is one.
    pub path: Option<PathBuf>,
    pub bytes: u64,
}

/// The token sections of `tokens.json`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tokens {
    pub name: String,
    pub version: u64,
    pub themes: Vec<Theme>,
    pub color: Vec<Token>,
    pub spacing: Vec<Token>,
    pub radius: Vec<Token>,
    pub size: Vec<Token>,
    pub size_note: Option<String>,
    pub shadow: Vec<Token>,
    pub families: BTreeMap<String, String>,
    pub type_groups: Vec<TypeGroup>,
}

impl Tokens {
    /// Every token across the sections, in file order.
    pub fn all(&self) -> impl Iterator<Item = &Token> {
        self.color
            .iter()
            .chain(&self.spacing)
            .chain(&self.radius)
            .chain(&self.size)
            .chain(&self.shadow)
    }

    /// The number of type styles across every group.
    pub fn type_style_count(&self) -> usize {
        self.type_groups
            .iter()
            .map(|group| group.styles.len())
            .sum()
    }
}

/// A loaded design system.
#[derive(Debug, Clone)]
pub struct DesignSystem {
    /// The `design/` directory.
    pub root: PathBuf,
    pub meta: Meta,
    pub tokens: Tokens,
    pub components: Vec<Component>,
    /// Names grouped in `design-system.json`'s `assetGroups`; usually empty.
    pub asset_groups: BTreeMap<String, Vec<String>>,
    /// Non-fatal problems found while loading, such as a component with no
    /// preview. `errors()` reports the fatal ones.
    pub warnings: Vec<String>,
}

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
    pub fn discover(start: &Path) -> Result<Self, String> {
        let mut dir = std::fs::canonicalize(start).unwrap_or_else(|_| start.to_path_buf());
        loop {
            if let Ok(system) = Self::load(&dir) {
                return Ok(system);
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

    /// A token by name, across every section.
    pub fn token(&self, name: &str) -> Option<&Token> {
        self.tokens.all().find(|token| token.name == name)
    }

    /// Fatal inconsistencies: a missing title, an unthemed color, a reference
    /// to a token that does not exist.
    pub fn errors(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.meta.title.trim().is_empty() {
            out.push("design-system.json: title is empty".to_string());
        }

        let theme_ids: Vec<&str> = self.tokens.themes.iter().map(|t| t.id.as_str()).collect();
        if theme_ids.is_empty() {
            out.push("tokens.json: color.themes is empty".to_string());
        }
        if self.tokens.color.is_empty() {
            out.push("tokens.json: color.tokens is empty".to_string());
        }

        let names: BTreeSet<&str> = self.tokens.all().map(|t| t.name.as_str()).collect();

        let sections: [(&str, &[Token]); 5] = [
            ("color", &self.tokens.color),
            ("spacing", &self.tokens.spacing),
            ("radius", &self.tokens.radius),
            ("size", &self.tokens.size),
            ("shadow", &self.tokens.shadow),
        ];
        for (section, tokens) in sections {
            for token in tokens {
                if let TokenValue::Themed(values) = &token.value {
                    for id in &theme_ids {
                        if !values.contains_key(*id) {
                            out.push(format!(
                                "{section} token `{}` has no value for theme `{id}`",
                                token.name
                            ));
                        }
                    }
                }
                for reference in references(&token.value) {
                    if !names.contains(reference.as_str()) {
                        out.push(format!(
                            "{section} token `{}` references `{{{reference}}}`, which does not exist",
                            token.name
                        ));
                    }
                }
            }
        }
        out
    }

    /// A short, human summary for `idesign show`.
    pub fn describe(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("{} ({})\n", self.meta.title, self.meta.namespace));
        out.push_str(&format!("  root       {}\n", self.root.display()));
        out.push_str(&format!("  version    v{}\n", self.meta.version));
        if let Some(change) = &self.meta.last_change {
            let who = change.by.as_deref().unwrap_or("unknown");
            let at = change.at.as_deref().unwrap_or("");
            let via = change.via.as_deref().unwrap_or("");
            out.push_str(&format!("  last change {who}, {at} via {via}\n"));
        }
        let themes: Vec<&str> = self.tokens.themes.iter().map(|t| t.id.as_str()).collect();
        out.push_str(&format!("  themes     {}\n", themes.join(", ")));
        out.push_str(&format!(
            "  tokens     {} color, {} spacing, {} radius, {} size, {} shadow, {} type styles\n",
            self.tokens.color.len(),
            self.tokens.spacing.len(),
            self.tokens.radius.len(),
            self.tokens.size.len(),
            self.tokens.shadow.len(),
            self.tokens.type_style_count(),
        ));
        out.push_str(&format!("  components {}\n", self.components.len()));
        out
    }

    /// The system's brief: `design/README.md`, the prose the browser shows on
    /// its Overview page.
    pub fn brief(&self) -> Option<String> {
        std::fs::read_to_string(self.root.join("README.md")).ok()
    }

    /// Every file under `design/`, `rel`ative to the root, sorted by path.
    pub fn files(&self) -> Vec<DesignFile> {
        let mut out = Vec::new();
        collect_files(&self.root, &self.root, &mut out);
        out.sort_by(|a, b| a.rel.cmp(&b.rel));
        out
    }

    /// The assets the system ships or declares: files under the project's
    /// `assets/` and the design's own `assets/`, plus the names in
    /// `design-system.json`'s `assetGroups` that are not files.
    pub fn assets(&self) -> Vec<Asset> {
        let mut out = Vec::new();
        if let Some(parent) = self.root.parent() {
            collect_assets(&parent.join("assets"), "project", &mut out);
        }
        collect_assets(&self.root.join("assets"), "design", &mut out);
        for (group, names) in &self.asset_groups {
            for name in names {
                if !out.iter().any(|asset| &asset.name == name) {
                    out.push(Asset {
                        name: name.clone(),
                        group: group.clone(),
                        path: None,
                        bytes: 0,
                    });
                }
            }
        }
        out.sort_by(|a, b| {
            (a.group.as_str(), a.name.as_str()).cmp(&(b.group.as_str(), b.name.as_str()))
        });
        out
    }

    /// Components by their README `Group:`, in group then name order.
    pub fn component_groups(&self) -> Vec<(String, Vec<&Component>)> {
        let mut map: BTreeMap<String, Vec<&Component>> = BTreeMap::new();
        for component in &self.components {
            map.entry(component_group(component))
                .or_default()
                .push(component);
        }
        map.into_iter()
            .map(|(group, mut list)| {
                list.sort_by(|a, b| a.name.cmp(&b.name));
                (group, list)
            })
            .collect()
    }

    /// Write the system back to `design/`. Fields the model does not carry
    /// (a Claude Design file holds a few) are preserved by merging into the
    /// existing JSON; component folders are copied when they are missing.
    pub fn save(&self, dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(dir)
            .map_err(|err| format!("Couldn't create {}: {err}", dir.display()))?;
        self.save_meta(dir)?;
        self.save_tokens(dir)?;
        self.save_components(dir)
    }

    /// Create a new component folder with a README and an HTML preview, and
    /// record the change in `design-system.json`.
    pub fn add_component(
        &mut self,
        name: &str,
        group: &str,
        intent: Option<&str>,
    ) -> Result<PathBuf, String> {
        let name = name.trim();
        validate_component_name(name)?;
        let dir = self.root.join("components").join(name);
        if dir.exists() {
            return Err(format!("{} already exists", dir.display()));
        }
        std::fs::create_dir_all(&dir)
            .map_err(|err| format!("Couldn't create {}: {err}", dir.display()))?;

        let readme = component_readme(name, group, intent);
        let readme_path = dir.join("README.md");
        std::fs::write(&readme_path, &readme)
            .map_err(|err| format!("Couldn't write {}: {err}", readme_path.display()))?;
        let preview_path = dir.join("preview.html");
        std::fs::write(&preview_path, component_preview(name, group))
            .map_err(|err| format!("Couldn't write {}: {err}", preview_path.display()))?;

        self.meta.last_change = Some(LastChange {
            by: None,
            at: Some(now()),
            via: Some("idesign".to_string()),
            note: Some(format!("Added component {name}.")),
        });
        self.components.push(Component {
            name: name.to_string(),
            dir: dir.clone(),
            readme: Some(readme),
            has_preview: true,
            card: CardMeta {
                group: Some(group.to_string()),
                height: Some(120),
                subtitle: None,
                page: false,
            },
        });
        self.components.sort_by(|a, b| a.name.cmp(&b.name));
        self.save_meta(&self.root)?;
        Ok(dir)
    }

    fn save_meta(&self, dir: &Path) -> Result<(), String> {
        let path = dir.join("design-system.json");
        let mut value = existing_json(&path)?.unwrap_or_else(|| serde_json::json!({}));
        let obj = value
            .as_object_mut()
            .ok_or_else(|| format!("{} is not a JSON object", path.display()))?;
        obj.insert("v".to_string(), serde_json::json!(self.meta.version));
        obj.insert("title".to_string(), serde_json::json!(self.meta.title));
        obj.insert(
            "namespace".to_string(),
            serde_json::json!(self.meta.namespace),
        );
        if let Some(readme) = &self.meta.readme {
            let docs = obj
                .entry("docs".to_string())
                .or_insert_with(|| serde_json::json!({}));
            if let Some(map) = docs.as_object_mut() {
                map.insert("readme".to_string(), serde_json::json!(readme));
            }
        }
        if let Some(change) = &self.meta.last_change {
            let mut map = serde_json::Map::new();
            insert_str(&mut map, "by", change.by.as_deref());
            insert_str(&mut map, "at", change.at.as_deref());
            insert_str(&mut map, "via", change.via.as_deref());
            insert_str(&mut map, "note", change.note.as_deref());
            obj.insert("lastChange".to_string(), serde_json::Value::Object(map));
        }
        write_json(&path, &value)
    }

    fn save_tokens(&self, dir: &Path) -> Result<(), String> {
        let path = dir.join("tokens.json");
        let mut value = existing_json(&path)?.unwrap_or_else(|| serde_json::json!({}));
        let obj = value
            .as_object_mut()
            .ok_or_else(|| format!("{} is not a JSON object", path.display()))?;
        obj.insert("name".to_string(), serde_json::json!(self.tokens.name));
        obj.insert(
            "version".to_string(),
            serde_json::json!(self.tokens.version),
        );

        let color = obj
            .entry("color".to_string())
            .or_insert_with(|| serde_json::json!({}));
        if let Some(map) = color.as_object_mut() {
            map.insert(
                "themes".to_string(),
                serde_json::Value::Array(
                    self.tokens
                        .themes
                        .iter()
                        .map(|theme| serde_json::json!({ "id": theme.id, "name": theme.name }))
                        .collect(),
                ),
            );
            map.insert("tokens".to_string(), tokens_json(&self.tokens.color));
        }

        set_section(obj, "spacing", &self.tokens.spacing, None);
        set_section(obj, "radius", &self.tokens.radius, None);
        set_section(
            obj,
            "size",
            &self.tokens.size,
            self.tokens.size_note.as_deref(),
        );
        set_section(obj, "shadow", &self.tokens.shadow, None);

        let ty = obj
            .entry("type".to_string())
            .or_insert_with(|| serde_json::json!({}));
        if let Some(map) = ty.as_object_mut() {
            map.insert(
                "families".to_string(),
                serde_json::to_value(&self.tokens.families)
                    .map_err(|err| format!("Couldn't encode families: {err}"))?,
            );
            map.insert("groups".to_string(), groups_json(&self.tokens.type_groups));
        }

        write_json(&path, &value)
    }

    fn save_components(&self, dir: &Path) -> Result<(), String> {
        let components = dir.join("components");
        std::fs::create_dir_all(&components)
            .map_err(|err| format!("Couldn't create {}: {err}", components.display()))?;
        for component in &self.components {
            let target = components.join(&component.name);
            if target.exists() || component.dir == target || !component.dir.is_dir() {
                continue;
            }
            copy_dir(&component.dir, &target)?;
        }
        Ok(())
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

/// The `Group:` a component belongs to: the preview's `@dsCard` group first,
/// then the README's `Group:` line, defaulting to `Components`.
fn component_group(component: &Component) -> String {
    if let Some(group) = component.card.group.as_deref().filter(|g| !g.is_empty()) {
        return group.to_string();
    }
    component
        .readme
        .as_deref()
        .and_then(|readme| {
            readme.lines().find_map(|line| {
                let line = line.trim_start_matches(['-', '*', ' ']);
                let rest = line
                    .strip_prefix("Group:")
                    .or_else(|| line.strip_prefix("**Group**:"))?;
                let name = rest.trim().trim_matches('*').trim();
                (!name.is_empty()).then(|| name.to_string())
            })
        })
        .unwrap_or_else(|| "Components".to_string())
}

/// Parse the `<!-- @dsCard group="…" height=N [subtitle="…"] [page] -->` line
/// a preview fragment leads with. Unknown or malformed keys are ignored.
pub fn parse_card_meta(text: &str) -> CardMeta {
    let mut meta = CardMeta::default();
    let Some(at) = text.find("@dsCard") else {
        return meta;
    };
    let rest = &text[at + "@dsCard".len()..];
    let body = match rest.find("-->") {
        Some(end) => &rest[..end],
        None => rest,
    };
    let chars: Vec<char> = body.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        let start = i;
        while i < chars.len() && !chars[i].is_whitespace() && chars[i] != '=' {
            i += 1;
        }
        let key: String = chars[start..i].iter().collect();
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        let mut value = None;
        if i < chars.len() && chars[i] == '=' {
            i += 1;
            while i < chars.len() && chars[i].is_whitespace() {
                i += 1;
            }
            let quoted = i < chars.len() && chars[i] == '"';
            if quoted {
                i += 1;
            }
            let value_start = i;
            while i < chars.len()
                && if quoted {
                    chars[i] != '"'
                } else {
                    !chars[i].is_whitespace()
                }
            {
                i += 1;
            }
            value = Some(chars[value_start..i].iter().collect::<String>());
            if quoted && i < chars.len() {
                i += 1;
            }
        }
        match key.as_str() {
            "group" => meta.group = value.filter(|v| !v.is_empty()),
            "subtitle" => meta.subtitle = value.filter(|v| !v.is_empty()),
            "height" => meta.height = value.and_then(|v| v.parse::<u32>().ok()),
            "page" => meta.page = true,
            _ => {}
        }
    }
    meta
}

/// Recursively collect the files under `dir`, relative to `root`, skipping
/// hidden entries.
fn collect_files(root: &Path, dir: &Path, out: &mut Vec<DesignFile>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            collect_files(root, &path, out);
        } else if let Ok(rel) = path.strip_prefix(root) {
            out.push(DesignFile {
                rel: rel.to_string_lossy().replace('\\', "/"),
                bytes: entry.metadata().map(|meta| meta.len()).unwrap_or(0),
            });
        }
    }
}

/// Recursively collect the files under an `assets/` folder, grouped by the
/// first folder below it (`fonts`, `images`).
fn collect_assets(dir: &Path, source: &str, out: &mut Vec<Asset>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            let group = format!("{source}/{name}");
            collect_grouped(&path, &group, out);
        } else {
            out.push(Asset {
                name: name.to_string(),
                group: source.to_string(),
                path: Some(path.clone()),
                bytes: entry.metadata().map(|meta| meta.len()).unwrap_or(0),
            });
        }
    }
}

fn collect_grouped(dir: &Path, group: &str, out: &mut Vec<Asset>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            collect_grouped(&path, group, out);
        } else {
            out.push(Asset {
                name: name.to_string(),
                group: group.to_string(),
                path: Some(path.clone()),
                bytes: entry.metadata().map(|meta| meta.len()).unwrap_or(0),
            });
        }
    }
}

/// Every `{name}` reference inside a token's values.
fn references(value: &TokenValue) -> Vec<String> {
    let mut out = Vec::new();
    for text in value.values() {
        let mut rest = text;
        while let Some(start) = rest.find('{') {
            let after = &rest[start + 1..];
            let Some(end) = after.find('}') else { break };
            let name = after[..end].trim();
            if !name.is_empty() {
                out.push(name.to_string());
            }
            rest = &after[end + 1..];
        }
    }
    out
}

/// Now, as an RFC 3339 UTC stamp (`2026-10-06T12:00:00Z`).
fn now() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Reject anything that is not a plain component folder name: no separators,
/// no leading dot, no dot-dot.
fn validate_component_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("a component name is required".to_string());
    }
    if name.len() > 40 {
        return Err("a component name is at most 40 characters".to_string());
    }
    if name != name.trim() {
        return Err("a component name cannot start or end with a space".to_string());
    }
    if !name
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == ' ')
    {
        return Err("use letters, digits, spaces or hyphens in a component name".to_string());
    }
    Ok(())
}

fn component_readme(name: &str, group: &str, intent: Option<&str>) -> String {
    let body = intent
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .unwrap_or("Describe what this component is for, when to use it and when not.");
    format!(
        "# {name}\n\n\
         {body}\n\n\
         - Group: {group}\n\
         - Tokens: `surface`, `line-strong`, `ink` — replace with the tokens this component uses.\n"
    )
}

fn component_preview(name: &str, group: &str) -> String {
    format!(
        "<!-- @dsCard group=\"{group}\" height=120 -->\n\
         <!doctype html>\n\
         <html lang=\"en\">\n\
         <head><meta charset=\"utf-8\"><title>{name}</title></head>\n\
         <body>\n\
         <div class=\"ib-pad ib-stack\">\n\
         <div class=\"ib-row\"><span class=\"ib-cap\">{name} — replace this with the real states.</span></div>\n\
         </div>\n\
         </body>\n\
         </html>\n"
    )
}

/// Recursively copy `from` into `to`, creating folders as needed.
fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to)
        .map_err(|err| format!("Couldn't create {}: {err}", to.display()))?;
    let entries = std::fs::read_dir(from)
        .map_err(|err| format!("Couldn't read {}: {err}", from.display()))?;
    for entry in entries {
        let entry = entry.map_err(|err| format!("Couldn't read {}: {err}", from.display()))?;
        let path = entry.path();
        let target = to.join(entry.file_name());
        if path.is_dir() {
            copy_dir(&path, &target)?;
        } else {
            std::fs::copy(&path, &target)
                .map_err(|err| format!("Couldn't copy {}: {err}", path.display()))?;
        }
    }
    Ok(())
}

fn existing_json(path: &Path) -> Result<Option<serde_json::Value>, String> {
    if !path.exists() {
        return Ok(None);
    }
    read_json(path).map(Some)
}

/// Write JSON in the one-space style the design system already uses.
fn write_json(path: &Path, value: &serde_json::Value) -> Result<(), String> {
    use serde::Serialize;
    let mut out = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b" ");
    let mut serializer = serde_json::Serializer::with_formatter(&mut out, formatter);
    value
        .serialize(&mut serializer)
        .map_err(|err| format!("Couldn't encode {}: {err}", path.display()))?;
    out.push(b'\n');
    std::fs::write(path, out).map_err(|err| format!("Couldn't write {}: {err}", path.display()))
}

fn insert_str(
    map: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: Option<&str>,
) {
    if let Some(value) = value {
        map.insert(key.to_string(), serde_json::json!(value));
    }
}

fn tokens_json(tokens: &[Token]) -> serde_json::Value {
    serde_json::Value::Array(tokens.iter().map(token_json).collect())
}

fn token_json(token: &Token) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    map.insert("name".to_string(), serde_json::json!(token.name));
    map.insert("value".to_string(), value_json(&token.value));
    insert_str(&mut map, "usage", token.usage.as_deref());
    serde_json::Value::Object(map)
}

fn value_json(value: &TokenValue) -> serde_json::Value {
    match value {
        TokenValue::Plain(value) => serde_json::json!(value),
        TokenValue::Themed(values) => serde_json::Value::Object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), serde_json::json!(value)))
                .collect(),
        ),
    }
}

fn set_section(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    tokens: &[Token],
    note: Option<&str>,
) {
    let section = obj
        .entry(key.to_string())
        .or_insert_with(|| serde_json::json!({}));
    if let Some(map) = section.as_object_mut() {
        map.insert("tokens".to_string(), tokens_json(tokens));
        insert_str(map, "note", note);
    }
}

fn groups_json(groups: &[TypeGroup]) -> serde_json::Value {
    serde_json::Value::Array(
        groups
            .iter()
            .map(|group| {
                serde_json::json!({
                    "name": group.name,
                    "family": group.family,
                    "styles": group.styles.iter().map(style_json).collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
}

fn style_json(style: &TypeStyle) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    insert_str(&mut map, "name", Some(&style.name));
    insert_str(&mut map, "fontSize", style.font_size.as_deref());
    insert_str(&mut map, "lineHeight", style.line_height.as_deref());
    if let Some(weight) = style.weight {
        map.insert("fontWeight".to_string(), serde_json::json!(weight));
    }
    insert_str(&mut map, "letterSpacing", style.letter_spacing.as_deref());
    insert_str(&mut map, "sample", style.sample.as_deref());
    insert_str(&mut map, "usage", style.usage.as_deref());
    serde_json::Value::Object(map)
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> DesignSystem {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("design");
        DesignSystem::load(&dir).expect("the repo's design/ loads")
    }

    #[test]
    fn loads_the_repo_design_system() {
        let system = repo();
        assert_eq!(system.meta.title, "iboard");
        assert_eq!(system.meta.namespace, "iboard");
        assert!(system.token("bg").is_some());
        assert!(system.token("accent").is_some());
        assert!(system.components.iter().any(|c| c.name == "Button"));
        assert_eq!(system.tokens.themes.len(), 2);
    }

    #[test]
    fn color_tokens_are_themed_both_ways() {
        let system = repo();
        let bg = system.token("bg").expect("bg exists");
        assert_eq!(bg.value.theme("light"), Some("#f4f5f7"));
        assert_eq!(bg.value.theme("dark"), Some("#15171a"));
    }

    #[test]
    fn references_and_themes_resolve() {
        let system = repo();
        assert_eq!(system.errors(), Vec::<String>::new());
    }

    #[test]
    fn discover_walks_up_from_the_manifest_dir() {
        let system = DesignSystem::discover(Path::new(env!("CARGO_MANIFEST_DIR")))
            .expect("discover finds design/");
        assert_eq!(system.meta.title, "iboard");
    }

    #[test]
    fn the_browser_can_read_files_assets_the_brief_and_groups() {
        let system = repo();
        let files = system.files();
        assert!(files.iter().any(|file| file.rel == "tokens.json"));
        assert!(files.iter().any(|file| file.rel == "design-system.json"));
        assert!(
            files
                .iter()
                .any(|file| file.rel == "components/Button/preview.html")
        );
        assert!(files.iter().all(|file| !file.rel.contains('\\')));
        assert!(system.brief().is_some(), "design/README.md is the brief");

        // The project's bundled fonts are exposed as assets.
        assert!(
            system
                .assets()
                .iter()
                .any(|asset| asset.name.starts_with("IBMPlexSans"))
        );
    }

    #[test]
    fn components_group_by_their_readme_group() {
        let system = repo();
        // Button ships no `Group:` line, so it lands in the default group.
        let groups = system.component_groups();
        assert!(
            groups
                .iter()
                .any(|(group, list)| list.iter().any(|c| c.name == "Button") && !group.is_empty())
        );
    }

    #[test]
    fn missing_preview_and_readme_are_warnings_not_errors() {
        let system = repo();
        // BoardScreen and Cover ship a preview without a README today.
        assert!(system.warnings.iter().any(|w| w.contains("BoardScreen")));
        assert!(system.errors().is_empty());
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or(0);
        let dir =
            std::env::temp_dir().join(format!("idesign-{tag}-{}-{unique}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn save_round_trips_the_repo_system() {
        let source = repo();
        let base = temp_dir("roundtrip");
        let target = base.join("design");

        source.save(&target).expect("save");
        let reloaded = DesignSystem::load(&target).expect("reload");

        assert_eq!(reloaded.meta, source.meta);
        assert_eq!(reloaded.tokens, source.tokens);
        assert_eq!(reloaded.components.len(), source.components.len());
        let before: Vec<&str> = source.components.iter().map(|c| c.name.as_str()).collect();
        let after: Vec<&str> = reloaded
            .components
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        assert_eq!(after, before);
        assert_eq!(reloaded.errors(), Vec::<String>::new());

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn add_component_creates_and_reloads() {
        let base = temp_dir("add");
        let target = base.join("design");
        repo().save(&target).expect("seed");
        let mut system = DesignSystem::load(&target).expect("load copy");

        let dir = system
            .add_component("Toast", "Controls", Some("Transient confirmation."))
            .expect("add");
        assert!(dir.join("README.md").is_file());
        assert!(dir.join("preview.html").is_file());

        let reloaded = DesignSystem::load(&target).expect("reload");
        assert!(reloaded.components.iter().any(|c| c.name == "Toast"));
        assert_eq!(reloaded.components.len(), system.components.len());
        assert!(reloaded.errors().is_empty());
        assert_eq!(
            reloaded
                .meta
                .last_change
                .as_ref()
                .and_then(|c| c.via.as_deref()),
            Some("idesign")
        );

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn add_component_refuses_duplicates_and_bad_names() {
        let base = temp_dir("bad");
        let target = base.join("design");
        repo().save(&target).expect("seed");
        let mut system = DesignSystem::load(&target).expect("load copy");

        assert!(system.add_component("Button", "Controls", None).is_err());
        assert!(system.add_component("", "Controls", None).is_err());
        assert!(system.add_component("../Evil", "Controls", None).is_err());
        assert!(system.add_component("Lead/ing", "Controls", None).is_err());

        std::fs::remove_dir_all(&base).ok();
    }
}
