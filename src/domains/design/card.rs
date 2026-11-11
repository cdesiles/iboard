//! The `@dsCard` contract a preview declares, and the groups it puts components in.

use std::collections::BTreeMap;

use super::model::{CardMeta, Component, DesignSystem};

impl DesignSystem {
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

    /// The components whose name contains `query`, all of them when it is
    /// empty, in the alphabetical order Components has in Claude Design.
    pub fn components_by_name(&self, query: &str) -> Vec<&Component> {
        let query = query.trim().to_lowercase();
        let mut list: Vec<&Component> = self
            .components
            .iter()
            .filter(|c| query.is_empty() || c.name.to_lowercase().contains(&query))
            .collect();
        list.sort_by_key(|c| c.name.to_lowercase());
        list
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
pub(super) fn parse_card_meta(text: &str) -> CardMeta {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_meta_parses_keys_and_the_page_flag() {
        let meta = parse_card_meta(
            "<!-- @dsCard group=\"Screens\" height=640 page subtitle=\"Click a column\" -->",
        );
        assert_eq!(meta.group.as_deref(), Some("Screens"));
        assert_eq!(meta.height, Some(640));
        assert!(meta.page);
        assert_eq!(meta.subtitle.as_deref(), Some("Click a column"));

        let minimal = parse_card_meta("<!-- @dsCard height=56 -->");
        assert_eq!(minimal.height, Some(56));
        assert!(!minimal.page);
        assert!(parse_card_meta("no card here").group.is_none());
    }
}
