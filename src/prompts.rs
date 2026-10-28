//! Prompt composition for agent calls.
//!
//! One builder, used by every feature that asks an agent to write against a
//! project's design system (component drafts, UX enrichment, the ticket
//! pipeline). It keeps the split Open Design makes: the brief is prose, the
//! token block is a binding contract. The design system is read-only here; only
//! the agent's answer changes files.

use crate::agent::ReviewMode;
use crate::design::{DesignSystem, css};

/// The design context: the brief, the token contract and the component
/// inventory, ready to splice into a prompt or to show as-is.
fn design_context(system: &DesignSystem, theme: &str) -> String {
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
/// issue-only on purpose, unlike the design-aware prompts below. A Deep review
/// (Ticket + project context) may check the issue against the repository and
/// its design system; a Quick one (Ticket only) must not. `notes` are the
/// author's comments, each on the passage it quotes.
pub fn review_prompt(content: &str, mode: ReviewMode, notes: &[(String, String)]) -> String {
    let scope = match mode {
        ReviewMode::Quick => "Read only the issue: do not open files or run tools.",
        ReviewMode::Deep => {
            "You may read the repository and its design system (design/, when there is one) \
             to check what the issue claims (sizes, existing code, duplicates, the components \
             it touches); keep the answer short."
        }
    };
    let mut prompt = format!(
        "You review software issues before they are implemented. {scope} Judge the \
         issue below on three axes and answer with nothing but this exact format:\n\
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
    );
    push_notes(&mut prompt, notes);
    prompt
}

/// The instruction for Update with AI: the agent checks, in the issue's
/// project, where the issue stands on `today`, changes nothing, and answers in
/// the review's format so the review panel shows it.
pub fn update_prompt(content: &str, today: &str) -> String {
    format!(
        "You keep a project's issue up to date. Today is {today}. Check in this repository \
         (its files, its git history, read-only commands) where the issue below stands: done, \
         under way, blocked, or no longer worth doing. Change nothing. Answer with nothing but \
         this exact format:\n\
         \n\
         RELEVANCE: Pass|Challenge|Block\n\
         <Pass: still to do as written; Challenge: still to do, but out of date; Block: done, or \
         no longer worth doing. Say what you found, naming the commits or files that show it>\n\
         SIZE: Pass|Challenge|Block\n\
         <what is left to do, and whether one person or agent can finish it in one sitting>\n\
         CLARITY: Pass|Challenge|Block\n\
         <what the issue says that is no longer true, or lacks to be picked up today>\n\
         PROPOSAL:\n\
         <the whole issue rewritten as things stand, keeping its front matter and its structure, \
         its next step brought up to date; or the single word NONE when it is still accurate>\n\
         \n\
         ---\n\
         {content}"
    )
}

/// One turn of a refinement for the agent to read again: who spoke
/// (`Challenger` or `Author`) and what they said.
pub type Turn<'a> = (&'a str, &'a str);

/// The instruction for a refinement: the agent plays a product challenger
/// talking with the issue's author until the issue is a clear spec worth
/// building, or until it is clear it should be dropped. `conversation` is
/// what was said so far, oldest first; the issue is read again every turn.
pub fn refine_prompt(content: &str, conversation: &[Turn], notes: &[(String, String)]) -> String {
    let mut prompt = format!(
        "You are a product challenger refining a software issue with its author before it \
         is planned (backlog refinement). Your sole role is to challenge the intent until \
         the issue is a clear spec worth building. Is the feature really adding value, and \
         for whom? Is the bug worth fixing: how often, and how badly does it hurt? Is there \
         a smaller or simpler way to the same outcome? What is out of scope, and how will \
         we know it is done? Read only the issue and the conversation: do not open files or \
         run tools, and do not write code. Be direct and brief: each turn, raise at most \
         three questions or objections, the most important first, and say plainly when you \
         think the issue should be dropped, and why. Answer with nothing but this format:\n\
         \n\
         TLDR: <your overall advice, in at most two short lines, so the author gets it at a \
         glance>\n\
         \n\
         <your challenge, in markdown: short paragraphs separated by blank lines>\n\
         OPTIONS:\n\
         - <a way to resolve it the author could pick, in one plain-text line: a title \
         of at most six words starting with a verb, a colon, then one or two sentences of \
         detail; two to four of them, the one you advise first>\n\
         PROPOSAL:\n\
         <once the conversation has settled the need, the full refined issue in markdown: \
         the need, the spec, out of scope and acceptance criteria; until then, the single \
         word NONE>\n\
         \n\
         ---\n\
         {content}"
    );
    push_notes(&mut prompt, notes);
    if !conversation.is_empty() {
        prompt.push_str("\n\n---\nThe conversation so far:\n");
        for (speaker, text) in conversation {
            prompt.push_str(&format!("\n{speaker}: {}\n", text.trim()));
        }
    }
    prompt
}

/// The instruction sent to the AI build agent: implement the issue at
/// `issue` (relative to the project), whose text is `content`, and wrap it up
/// the way the project asks.
pub fn build_prompt(issue: &str, content: &str) -> String {
    format!(
        "You implement one issue in this repository, working in it directly. The issue is \
         `{issue}`; it was just moved to its in-progress column. Read the project's own \
         instructions first (AGENTS.md, CLAUDE.md, CONTRIBUTING.md, whichever exist) and follow \
         them: their checks, how a finished issue is closed and how it is committed. Other agents \
         or people may be changing files at the same time: touch and commit only what this issue \
         needs. If the issue cannot be done as written, change no code and explain why. End with \
         a two or three line summary of what you did.\n\
         \n\
         ---\n\
         {content}"
    )
}

/// The line each ticket of a split answer starts with.
pub const SPLIT_MARK: &str = "=== TICKET ===";

/// A prompt asking an agent to cut an issue too big for one sitting into
/// smaller ones. `reason` is the review's Size finding.
pub fn split_prompt(content: &str, reason: &str, notes: &[(String, String)]) -> String {
    let mut prompt = format!(
        "You split a software issue that is too big for one sitting into smaller issues. \
         A review said about its size: {reason}\n\
         \n\
         Write 2 to 6 issues, in the order they should be done. Each one must be \
         finishable and committable on its own, and together they must cover the whole \
         original. Keep the original's [labels] in each title. Answer with nothing but \
         the issues, each one starting with a line `{SPLIT_MARK}`, then a \
         `# [kind] Title` line, then its body in markdown, 3 to 15 lines.\n\
         \n\
         ---\n\
         {content}",
        reason = reason.trim(),
    );
    push_notes(&mut prompt, notes);
    prompt
}

/// The author's notes, each quoting the passage it is about.
fn push_notes(prompt: &mut String, notes: &[(String, String)]) {
    if notes.is_empty() {
        return;
    }
    prompt.push_str(
        "\n\n---\nThe author's notes on the issue, each under the passage it is about. \
         Take them into account:\n",
    );
    for (passage, note) in notes {
        prompt.push('\n');
        for line in passage.lines() {
            prompt.push_str(&format!("> {line}\n"));
        }
        prompt.push_str(note.trim());
        prompt.push('\n');
    }
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

    const ISSUE: &str = "# Taller header\n\nThe header should be 48pt.\n";

    #[test]
    fn the_review_prompt() {
        insta::assert_snapshot!(review_prompt(ISSUE, ReviewMode::Quick, &[]));
    }

    #[test]
    fn the_update_prompt() {
        insta::assert_snapshot!(update_prompt(ISSUE, "2026-10-28"));
    }

    #[test]
    fn the_review_prompt_with_project_context() {
        insta::assert_snapshot!(review_prompt(ISSUE, ReviewMode::Deep, &[]));
    }

    #[test]
    fn the_refine_prompt() {
        let conversation = [
            ("Challenger", "Who needs a taller header?"),
            ("Author", "Touch users miss the buttons."),
        ];
        insta::assert_snapshot!(refine_prompt(ISSUE, &conversation, &[]));
    }

    #[test]
    fn the_enrichment_prompt_without_a_design_system() {
        insta::assert_snapshot!(enrichment_prompt(ISSUE, None, None, "light"));
    }

    #[test]
    fn the_enrichment_prompt_refining_a_draft() {
        let previous = ("<!doctype html><h1>Header</h1>", "make it taller");
        insta::assert_snapshot!(enrichment_prompt(ISSUE, None, Some(previous), "light"));
    }

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
    fn review_prompt_quotes_the_authors_notes() {
        let notes = [(
            "Three files.\nOver 1,100 lines.".to_string(),
            "Only design.rs matters now.".to_string(),
        )];
        let prompt = review_prompt("# Split", ReviewMode::Quick, &notes);
        assert!(
            prompt.ends_with("> Three files.\n> Over 1,100 lines.\nOnly design.rs matters now.\n")
        );
        assert!(!review_prompt("# Split", ReviewMode::Quick, &[]).contains("notes"));
    }

    #[test]
    fn the_build_prompt() {
        insta::assert_snapshot!(build_prompt(
            "issues/doing/001-play.md",
            "# [feat] Play\n\nStart the ready issues."
        ));
    }

    #[test]
    fn the_split_prompt_names_the_reason_and_the_shape() {
        let prompt = split_prompt("# [refactor] Big", "Three files, cut per file.", &[]);
        assert!(prompt.contains("A review said about its size: Three files, cut per file."));
        assert!(prompt.contains(SPLIT_MARK));
        assert!(prompt.ends_with("# [refactor] Big"));
    }

    #[test]
    fn review_prompt_stays_issue_only() {
        let prompt = review_prompt(
            "# Fix the filter\n\nIt forgets the chips.",
            ReviewMode::Deep,
            &[],
        );
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
