//! A new component folder, with a starter README and preview.

use std::path::PathBuf;

use super::model::{CardMeta, Component, DesignSystem, LastChange};

impl DesignSystem {
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
