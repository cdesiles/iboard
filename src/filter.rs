//! The board filter: label tokens picked from suggestions, then free text.
//!
//! The suggestions are the board's three most used labels, unless the user
//! configuration (`~/.config/iboard/iboard.toml`, next to the `[ai]` section)
//! pins them:
//!
//! ```toml
//! [filter]
//! suggestions = ["bug", "feature"]
//! ```

use serde::Deserialize;

use crate::issue::Issue;
use crate::labels;
use crate::settings::UserConfig;

/// How many labels the field suggests when nothing is pinned.
const TOP_LABELS: usize = 3;

/// The board's filter. Every token and the text must all match.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    /// Labels picked from the suggestions (or a card), in pick order.
    pub tokens: Vec<String>,
    /// What the user typed after the tokens.
    pub text: String,
}

impl Filter {
    /// No token and no text: the board shows everything.
    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty() && self.text.trim().is_empty()
    }

    pub fn clear(&mut self) {
        self.tokens.clear();
        self.text.clear();
    }

    /// Adds a label as a token, once.
    pub fn pick(&mut self, word: &str) {
        if let Some(word) = labels::normalize(word)
            && !self.tokens.contains(&word)
        {
            self.tokens.push(word);
        }
    }

    /// Every token is one of the issue's labels, exactly, and the text
    /// matches.
    pub fn matches(&self, issue: &Issue) -> bool {
        self.tokens.iter().all(|token| issue.labels.contains(token))
            && issue.matches(self.text.trim())
    }

    /// What an empty column says while filtering: "No issues labelled bug"
    /// for labels alone, else "No issues match bug login".
    pub fn empty_message(&self) -> String {
        if self.text.trim().is_empty() && !self.tokens.is_empty() {
            format!("No issues labelled {}", self.tokens.join(" and "))
        } else {
            format!("No issues match {}", self.describe())
        }
    }

    /// The filter in words, for "No issues match bug login".
    pub fn describe(&self) -> String {
        let mut words: Vec<&str> = self.tokens.iter().map(String::as_str).collect();
        let text = self.text.trim();
        if !text.is_empty() {
            words.push(text);
        }
        words.join(" ")
    }
}

/// The `count` labels used most across `issues`, most used first, ties by
/// name.
pub fn top_labels<'a>(issues: impl IntoIterator<Item = &'a Issue>, count: usize) -> Vec<String> {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for issue in issues {
        for label in &issue.labels {
            match counts.iter_mut().find(|(name, _)| name == label) {
                Some((_, n)) => *n += 1,
                None => counts.push((label.clone(), 1)),
            }
        }
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    counts
        .into_iter()
        .take(count)
        .map(|(name, _)| name)
        .collect()
}

/// The suggestions for a board: the pinned ones, else its three most used
/// labels.
pub fn suggestions<'a>(
    pinned: Option<&[String]>,
    issues: impl IntoIterator<Item = &'a Issue>,
) -> Vec<String> {
    match pinned {
        Some(pinned) => pinned.to_vec(),
        None => top_labels(issues, TOP_LABELS),
    }
}

/// The `[filter]` section of the user configuration.
#[derive(Debug, Default, Deserialize)]
struct RawFilter {
    suggestions: Option<Vec<String>>,
}

/// The suggestions pinned in `[filter]`; `None` when the section or the key
/// is missing, so the board's labels are offered.
pub fn load_suggestions(config: &mut UserConfig) -> Option<Vec<String>> {
    let raw: RawFilter = config.section("filter");
    raw.suggestions.map(|words| {
        let mut picked: Vec<String> = Vec::new();
        for word in words {
            if let Some(word) = labels::normalize(&word)
                && !picked.contains(&word)
            {
                picked.push(word);
            }
        }
        picked
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn issue(file: &str, markdown: &str) -> Issue {
        let dir = std::env::temp_dir().join(format!("iboard-filter-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path: PathBuf = dir.join(file);
        std::fs::write(&path, markdown).unwrap();
        Issue::load(&path).unwrap()
    }

    #[test]
    fn tokens_and_text_must_all_match() {
        let login = issue(
            "fix-login.md",
            "# [Bug] Fix login redirect\n\nA bug on sign in.\n",
        );
        let export = issue("export.md", "---\ntype: feature\n---\n# Export board\n");
        // A label token is exact: the word in the body or file name is not it.
        let debug = issue("debug-log.md", "# Debug log\n\nA bug list.\n");
        let mut filter = Filter::default();
        filter.pick("bug");
        assert!(filter.matches(&login));
        assert!(!filter.matches(&export));
        assert!(!filter.matches(&debug));
        assert_eq!(filter.empty_message(), "No issues labelled bug");
        filter.text = "login".into();
        assert!(filter.matches(&login));
        filter.text = "export".into();
        assert!(!filter.matches(&login));
    }

    #[test]
    fn a_token_matches_front_matter() {
        let export = issue(
            "export-2.md",
            "---\nlabels: [feature]\n---\n# Export board\n",
        );
        let mut filter = Filter::default();
        filter.pick("Feature");
        assert_eq!(filter.tokens, vec!["feature"]);
        assert!(filter.matches(&export));
    }

    #[test]
    fn picking_twice_keeps_one_token() {
        let mut filter = Filter::default();
        filter.pick("bug");
        filter.pick("bug");
        assert_eq!(filter.tokens.len(), 1);
        assert_eq!(filter.describe(), "bug");
        filter.text = " login ".into();
        assert_eq!(filter.describe(), "bug login");
        filter.clear();
        assert!(filter.is_empty());
    }

    #[test]
    fn suggestions_are_the_most_used_labels_unless_pinned() {
        let a = issue("a.md", "# [Bug][UI] A\n");
        let b = issue("b.md", "# [bug] B\n");
        let c = issue("c.md", "---\nlabels: [ops]\n---\n# [ui] C\n");
        let d = issue("d.md", "# [zeta] D\n");
        let board = [&a, &b, &c, &d];
        assert_eq!(suggestions(None, board), vec!["bug", "ui", "ops"]);
        let pinned = vec!["chore".to_string()];
        assert_eq!(suggestions(Some(&pinned), board), vec!["chore"]);
    }

    #[test]
    fn suggestions_are_pinned_only_by_the_config() {
        let pinned = |text: &str| {
            load_suggestions(&mut UserConfig::parse(text, std::path::Path::new("c.toml")))
        };
        assert_eq!(load_suggestions(&mut UserConfig::default()), None);
        assert_eq!(pinned("[ai]\ntool = \"claude\"\n"), None);
        assert_eq!(
            pinned("[filter]\nsuggestions = [\"Chore\", \"bug\", \"\"]\n"),
            Some(vec!["chore".to_string(), "bug".to_string()])
        );
    }
}
