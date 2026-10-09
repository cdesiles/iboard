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

/// The instruction sent to the agent for a review. It carries only the issue,
/// by design: if that is not enough context, the issue is the problem. It stays
/// issue-only on purpose, unlike the design-aware prompts below.
pub fn review_prompt(content: &str) -> String {
    format!(
        "You review software issues before they are implemented. Judge the issue \
         below on three axes and answer with nothing but this exact format:\n\
         \n\
         RELEVANCE: Pass|Challenge|Block\n\
         <one or two sentences: is it still worth doing, does it duplicate another issue, \
         and is it tied to a clear user-facing outcome (its product impact)>\n\
         SIZE: Pass|Challenge|Block\n\
         <one or two sentences: can one person or agent finish it in one sitting, and if not, \
         the split; an issue over about 80 lines should be cut into smaller ones>\n\
         CLARITY: Pass|Challenge|Block\n\
         <one or two sentences: could a human or an agent implement it without a follow-up \
         question; is there enough guidance on the user experience; list what is missing, \
         contradictory or assumed>\n\
         PROPOSAL:\n\
         <a full improved issue in markdown, or the single word NONE>\n\
         \n\
         ---\n\
         {content}"
    )
}

/// A prompt asking an agent to draft a static HTML mockup of a ticket.
///
/// With a design system it binds to the system's tokens (read-only); without
/// one it asks for restrained defaults. `previous` carries the last artifact and
/// the user's refinement, so a follow-up edits instead of restarting.
pub fn enrichment_prompt(
    issue: &str,
    system: Option<&DesignSystem>,
    previous: Option<(&str, &str)>,
    theme: &str,
) -> String {
    let mut out = String::new();
    out.push_str(
        "You are a product designer. Produce a single static HTML mockup of the \
         change described by the ticket below — a visual answer, not prose. Return \
         only the HTML of one self-contained page. No <script>. No Markdown fence.\n\n",
    );

    out.push_str("## Ticket\n\n");
    out.push_str(issue.trim());
    out.push_str("\n\n");

    match system {
        Some(system) => {
            out.push_str(
                "Use the active design system below. Paste its `:root` block verbatim \
                 into the first <style>, reference every colour and size through \
                 var(--name), and never write raw hex outside that block. The system \
                 is read-only: do not change it.\n\n",
            );
            out.push_str(&design_context(system, theme));
        }
        None => {
            out.push_str(
                "This project has no design system; choose a clean, restrained set of \
                 colours, type and spacing and keep it consistent.\n\n",
            );
        }
    }

    if let Some((previous, instruction)) = previous {
        out.push_str("## Previous artifact\n\n```html\n");
        out.push_str(previous.trim());
        out.push_str("\n```\n\n## Refinement\n\n");
        out.push_str(instruction.trim());
        out.push_str("\n\nReturn the full updated HTML, not a diff.\n");
    }

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

/// Keep the markup from an agent's answer: it is asked for an HTML fragment but
/// often wraps it in a fenced block or a sentence. Prefer a fenced block, else
/// everything from the first tag.
pub fn clean_html_reply(reply: &str) -> String {
    let text = reply.trim();
    if let Some(start) = text.find("```") {
        let after = &text[start + 3..];
        let after = after
            .strip_prefix("html")
            .or_else(|| after.strip_prefix("HTML"))
            .unwrap_or(after);
        let after = after.trim_start_matches(['\n', '\r']);
        if let Some(end) = after.find("```") {
            let body = after[..end].trim();
            if body.contains('<') {
                return body.to_string();
            }
        }
    }
    if let Some(at) = text.find('<') {
        return text[at..].trim().to_string();
    }
    text.to_string()
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

    #[test]
    fn clean_html_reply_keeps_the_markup() {
        let fenced = "Here you go:\n```html\n<button class=\"ib-btn\">Hi</button>\n```\n";
        assert_eq!(
            clean_html_reply(fenced),
            "<button class=\"ib-btn\">Hi</button>"
        );

        let plain = "  <div class=\"ib-pill\">Saved</div>  ";
        assert_eq!(
            clean_html_reply(plain),
            "<div class=\"ib-pill\">Saved</div>"
        );

        let prose = "Sure, try this:\n<span class=\"ib-kbd\">esc</span>";
        assert_eq!(clean_html_reply(prose), "<span class=\"ib-kbd\">esc</span>");

        assert_eq!(clean_html_reply("no markup here"), "no markup here");
    }

    #[test]
    fn review_prompt_stays_issue_only() {
        let prompt = review_prompt("# Fix the filter\n\nIt forgets the chips.");
        assert!(prompt.contains("RELEVANCE: Pass|Challenge|Block"));
        assert!(prompt.contains("PROPOSAL:"));
        assert!(prompt.contains("# Fix the filter"));
        // The review deliberately carries no design system.
        assert!(!prompt.contains("Active design system"));
    }

    #[test]
    fn enrichment_prompt_binds_to_a_system_when_present() {
        let issue = "# Column focus\n\nThe board expands one column.";
        let with = enrichment_prompt(issue, Some(&repo()), None, "light");
        assert!(with.contains("## Ticket"));
        assert!(with.contains("Column focus"));
        assert!(with.contains("Active design system"));
        assert!(with.contains("--bg: #f4f5f7;"));
        assert!(with.contains("read-only"));

        let without = enrichment_prompt(issue, None, None, "light");
        assert!(without.contains("no design system"));
        assert!(!without.contains("Active design system"));
    }

    #[test]
    fn enrichment_prompt_carries_a_refinement() {
        let prompt = enrichment_prompt(
            "# Toast",
            None,
            Some(("<div>old</div>", "Make it bigger.")),
            "light",
        );
        assert!(prompt.contains("## Previous artifact"));
        assert!(prompt.contains("<div>old</div>"));
        assert!(prompt.contains("## Refinement"));
        assert!(prompt.contains("Make it bigger."));
    }
}
