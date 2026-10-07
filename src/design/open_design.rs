//! Import an Open Design design-system package into the `design/` shape.
//!
//! An Open Design package is a folder with `manifest.json`, a prose `DESIGN.md`,
//! a `tokens.css` contract and a `components.html` fixture (see the Open Design
//! project, credited in NOTICE). We map the parts that fit the canonical model —
//! the name, the brief and the tokens — and warn about the rest. Its components
//! are one HTML fixture rather than per-component folders, so they are not
//! imported; `design/` owns components once the package is written.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::{DesignSystem, Meta, Theme, Token, TokenValue, Tokens};

/// A package read from disk, before it is written out as `design/`.
pub struct Package {
    pub title: String,
    pub namespace: String,
    pub brief: Option<String>,
    pub tokens: Tokens,
    pub warnings: Vec<String>,
}

impl Package {
    /// Write the package as a `design/` directory: `design-system.json`,
    /// `tokens.json`, and `README.md` when the package carried a brief.
    pub fn write(self, design_dir: &Path) -> Result<DesignSystem, String> {
        let brief = self.brief.clone();
        let system = self.into_design_system(design_dir.to_path_buf());
        system.save(design_dir)?;
        if let Some(brief) = brief {
            let readme = design_dir.join("README.md");
            std::fs::write(&readme, brief)
                .map_err(|err| format!("Couldn't write {}: {err}", readme.display()))?;
        }
        Ok(system)
    }

    /// The same package as a `DesignSystem` rooted at `root`, without writing.
    pub fn into_design_system(self, root: PathBuf) -> DesignSystem {
        DesignSystem {
            root,
            meta: Meta {
                title: self.title,
                namespace: self.namespace,
                version: 1,
                readme: self.brief.as_ref().map(|_| "README.md".to_string()),
                last_change: None,
            },
            tokens: self.tokens,
            components: Vec::new(),
            asset_groups: std::collections::BTreeMap::new(),
            warnings: self.warnings,
        }
    }
}

#[derive(Debug, Deserialize, Default)]
struct RawManifest {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    files: RawFiles,
}

#[derive(Debug, Deserialize, Default)]
struct RawFiles {
    #[serde(default)]
    design: Option<String>,
    #[serde(default)]
    tokens: Option<String>,
}

/// Read an Open Design package from `dir`.
pub fn read(dir: &Path) -> Result<Package, String> {
    let manifest_path = dir.join("manifest.json");
    if !manifest_path.is_file() {
        return Err(format!(
            "No manifest.json in {} — not an Open Design package.",
            dir.display()
        ));
    }
    let manifest: RawManifest = super::read_json(&manifest_path)?;

    let title = if manifest.name.trim().is_empty() {
        dir.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Imported")
            .to_string()
    } else {
        manifest.name
    };
    let namespace = if manifest.id.trim().is_empty() {
        slug(&title)
    } else {
        manifest.id
    };

    let mut warnings = Vec::new();
    let brief = read_optional(dir, manifest.files.design.as_deref().unwrap_or("DESIGN.md"))?;
    if brief.is_none() {
        warnings.push("no DESIGN.md found; the brief will be empty".to_string());
    }
    let css = read_optional(
        dir,
        manifest.files.tokens.as_deref().unwrap_or("tokens.css"),
    )?
    .unwrap_or_default();
    let parsed = parse_tokens_css(&css);
    if parsed.is_empty() {
        warnings.push("no tokens found in tokens.css".to_string());
    }
    warnings.push(
        "Open Design components are an HTML fixture, not folders; none were imported".to_string(),
    );

    let mut tokens = Tokens {
        name: title.clone(),
        version: 1,
        themes: vec![Theme {
            id: "light".to_string(),
            name: "Light".to_string(),
        }],
        ..Default::default()
    };
    for token in parsed {
        route(&mut tokens, token);
    }

    Ok(Package {
        title,
        namespace,
        brief,
        tokens,
        warnings,
    })
}

/// Route a parsed custom property into the section its name suggests.
fn route(tokens: &mut Tokens, token: Token) {
    if let Some(key) = token.name.strip_prefix("font-") {
        tokens
            .families
            .insert(key.to_string(), value_string(&token.value));
    } else if token.name.starts_with("space-") {
        tokens.spacing.push(token);
    } else if token.name.starts_with("radius-") {
        tokens.radius.push(token);
    } else if token.name.starts_with("shadow-") {
        tokens.shadow.push(token);
    } else if token.name.starts_with("text-")
        || token.name.starts_with("leading-")
        || token.name.starts_with("tracking-")
    {
        tokens.size.push(token);
    } else {
        tokens.color.push(token);
    }
}

fn value_string(value: &TokenValue) -> String {
    match value {
        TokenValue::Plain(value) => value.clone(),
        TokenValue::Themed(values) => values
            .get("light")
            .or_else(|| values.values().next())
            .cloned()
            .unwrap_or_default(),
    }
}

fn read_optional(dir: &Path, name: &str) -> Result<Option<String>, String> {
    let path = dir.join(name);
    if !path.is_file() {
        return Ok(None);
    }
    std::fs::read_to_string(&path)
        .map(Some)
        .map_err(|err| format!("Couldn't read {}: {err}", path.display()))
}

/// `--name: value;` pairs from a stylesheet, ignoring comments. A themed block
/// (`[data-theme=dark]`) is not modeled; every value is taken as-is.
pub fn parse_tokens_css(css: &str) -> Vec<Token> {
    let chars: Vec<char> = strip_comments(css).chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '-' && i + 1 < chars.len() && chars[i + 1] == '-' {
            let start = i + 2;
            let mut j = start;
            while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '-') {
                j += 1;
            }
            let name: String = chars[start..j].iter().collect();
            let mut k = j;
            while k < chars.len() && chars[k].is_whitespace() {
                k += 1;
            }
            if !name.is_empty() && k < chars.len() && chars[k] == ':' {
                k += 1;
                let mut e = k;
                while e < chars.len() && chars[e] != ';' && chars[e] != '}' && chars[e] != '\n' {
                    e += 1;
                }
                let value: String = chars[k..e].iter().collect();
                let value = value.trim();
                if !value.is_empty() {
                    tokens.push(Token {
                        name,
                        value: TokenValue::Plain(value.to_string()),
                        usage: None,
                    });
                }
                i = e;
                continue;
            }
        }
        i += 1;
    }
    tokens
}

/// Replace `/* … */` comments with a space, char-safe.
fn strip_comments(css: &str) -> String {
    let chars: Vec<char> = css.chars().collect();
    let mut out = String::with_capacity(css.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i += 2;
            out.push(' ');
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

fn slug(text: &str) -> String {
    let mut out = String::new();
    let mut last_dash = true;
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "idesign-import-{tag}-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn parses_root_tokens_and_skips_comments() {
        let css = r#"
            /* header */
            :root {
              --bg: #ffffff;
              --accent: oklch(0.6 0.2 250);
              --accent-hover: color-mix(in oklab, var(--accent), black 8%);
            }
        "#;
        let tokens = parse_tokens_css(css);
        let names: Vec<&str> = tokens.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["bg", "accent", "accent-hover"]);
        assert_eq!(tokens[0].value.theme("light"), Some("#ffffff"));
    }

    #[test]
    fn reads_and_writes_a_package() {
        let package_dir = temp_dir("package");
        std::fs::write(
            package_dir.join("manifest.json"),
            r#"{ "schemaVersion": "x", "id": "acme", "name": "Acme", "files": { "design": "DESIGN.md", "tokens": "tokens.css" } }"#,
        )
        .unwrap();
        std::fs::write(package_dir.join("DESIGN.md"), "# Acme\n\nBold and red.\n").unwrap();
        std::fs::write(
            package_dir.join("tokens.css"),
            ":root {\n  --bg: #101010;\n  --accent: #e11d48;\n}\n",
        )
        .unwrap();

        let package = read(&package_dir).expect("read");
        assert_eq!(package.title, "Acme");
        assert_eq!(package.namespace, "acme");
        assert_eq!(package.brief.as_deref(), Some("# Acme\n\nBold and red.\n"));
        assert_eq!(package.tokens.color.len(), 2);

        let design = package_dir.join("design-out");
        let system = package.write(&design).expect("write");
        assert_eq!(system.meta.title, "Acme");
        assert!(design.join("README.md").is_file());

        let reloaded = DesignSystem::load(&design).expect("reload");
        assert_eq!(reloaded.meta.title, "Acme");
        assert_eq!(reloaded.tokens.color.len(), 2);
        assert!(reloaded.errors().is_empty());

        std::fs::remove_dir_all(&package_dir).ok();
    }

    #[test]
    fn rejects_a_folder_without_a_manifest() {
        let dir = temp_dir("nomanifest");
        assert!(read(&dir).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
