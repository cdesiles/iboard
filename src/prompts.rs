//! Prompt composition for agent calls.
//!
//! One builder, used by every feature that asks an agent to write against a
//! project's design system (component drafts, UX enrichment, the ticket
//! pipeline). It keeps the split Open Design makes: the brief is prose, the
//! token block is a binding contract. The design system is read-only here; only
//! the agent's answer changes files.

use crate::design::{DesignSystem, css};

/// The design context: the brief, the token contract and the component
/// inventory, ready to splice into a prompt or to show as-is.
pub fn design_context(system: &DesignSystem, theme: &str) -> String {
    let mut out = format!("## Active design system — {}\n\n", system.meta.title);
    out.push_str(
        "Treat this as authoritative for colour, typography, spacing and components. \
         Do not invent tokens. The brief is prose; the token block is the binding contract.\n\n",
    );

    if let Some(brief) = brief(system) {
        out.push_str("### Brief\n\n");
        out.push_str(brief.trim());
        out.push_str("\n\n");
    }

    out.push_str("### Token contract — paste the `:root` block verbatim\n\n```css\n");
    out.push_str(&css::tokens_css(system, theme));
    out.push_str("```\n\n");

    if !system.components.is_empty() {
        out.push_str("### Components\n\n");
        for component in &system.components {
            out.push_str(&format!("- `{}`", component.name));
            if let Some(summary) = css::component_summary(component) {
                out.push_str(&format!(" — {summary}"));
            }
            out.push('\n');
        }
        out.push('\n');
    }
    out
}

/// A prompt asking an agent to draft one new component for the system.
pub fn component_prompt(
    system: &DesignSystem,
    name: &str,
    group: &str,
    intent: &str,
    theme: &str,
) -> String {
    let mut out = String::new();
    out.push_str(
        "You write one component for a design system. Produce a single self-contained \
         HTML fragment for its preview, using only the tokens and the classes of the \
         bundle below. No <script>. No raw hex outside the token block. Return only the \
         fragment.\n\n",
    );
    out.push_str(&format!(
        "Component: {name}\nGroup: {group}\nIntent: {intent}\n\n"
    ));
    out.push_str(&design_context(system, theme));
    out
}

/// The readable brief: the file `design-system.json` points at, else README.md.
fn brief(system: &DesignSystem) -> Option<String> {
    let mut candidates = Vec::new();
    if let Some(path) = &system.meta.readme {
        candidates.push(system.root.join(path));
    }
    candidates.push(system.root.join("README.md"));
    candidates
        .into_iter()
        .find_map(|path| std::fs::read_to_string(path).ok())
        .filter(|text| !text.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn repo() -> DesignSystem {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("design");
        DesignSystem::load(&dir).expect("load")
    }

    #[test]
    fn design_context_carries_tokens_and_components() {
        let context = design_context(&repo(), "light");
        assert!(context.contains("Active design system"));
        assert!(context.contains("--bg: #f4f5f7;"));
        assert!(context.contains("`Button`"));
    }

    #[test]
    fn component_prompt_names_the_component_and_the_contract() {
        let prompt = component_prompt(
            &repo(),
            "Toast",
            "Controls",
            "Transient confirmation.",
            "light",
        );
        assert!(prompt.contains("Component: Toast"));
        assert!(prompt.contains(":root"));
        assert!(prompt.contains("binding contract"));
    }
}
