//! Writing the system back to `design/`, keeping what the model does not carry.

use std::path::Path;

use super::load::read_json;
use super::model::{DesignSystem, Token, TokenValue, TypeGroup, TypeStyle};

impl DesignSystem {
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

    pub(super) fn save_meta(&self, dir: &Path) -> Result<(), String> {
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
