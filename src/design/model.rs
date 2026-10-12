//! The design system's types, as loaded from `design/`, and its summaries.

use std::collections::BTreeMap;
use std::path::PathBuf;

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
    /// A token by name, across every section.
    pub fn token(&self, name: &str) -> Option<&Token> {
        self.tokens.all().find(|token| token.name == name)
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
}
