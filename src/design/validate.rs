//! The fatal inconsistencies `idesign validate` reports.

use std::collections::BTreeSet;

use super::model::{DesignSystem, Token, TokenValue};

impl DesignSystem {
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
