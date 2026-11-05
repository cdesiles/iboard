//! Reading the agent's answers: a review's findings and proposed rewrite,
//! the tickets of a split, and a refinement's turn.

/// One ticket of a split: its `# ` title (labels included) and its body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    pub title: String,
    pub body: String,
}

/// The tickets of a split answer, each after a [`crate::prompts::SPLIT_MARK`]
/// line. `None` unless there are at least two, each with a title.
pub fn parse_split(text: &str) -> Option<Vec<Draft>> {
    let drafts = text
        .split(crate::prompts::SPLIT_MARK)
        .skip(1)
        .map(|chunk| {
            let lines: Vec<&str> = chunk.lines().collect();
            let at = crate::markdown::title_line(&lines)?;
            Some(Draft {
                title: lines[at].trim_start_matches('#').trim().to_string(),
                body: lines[at + 1..].join("\n").trim().to_string(),
            })
        })
        .collect::<Option<Vec<Draft>>>()?;
    (drafts.len() >= 2).then_some(drafts)
}

/// One axis of the review: what a finding judges the issue on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    Relevance,
    Size,
    Clarity,
}

impl Axis {
    const ALL: [Self; 3] = [Self::Relevance, Self::Size, Self::Clarity];

    /// Its key in the agent's answer, as in `SIZE: Pass`.
    fn key(self) -> &'static str {
        match self {
            Self::Relevance => "RELEVANCE",
            Self::Size => "SIZE",
            Self::Clarity => "CLARITY",
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|axis| axis.key() == key)
    }

    /// The name in the review panel.
    pub fn name(self) -> &'static str {
        match self {
            Self::Relevance => "Relevance",
            Self::Size => "Size",
            Self::Clarity => "Clarity",
        }
    }
}

/// The verdict on one axis of the review, ranked from the best to the worst.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Verdict {
    Pass,
    Challenge,
    Block,
}

impl Verdict {
    fn parse(word: &str) -> Option<Self> {
        match word.trim().to_ascii_lowercase().as_str() {
            "pass" => Some(Self::Pass),
            "challenge" => Some(Self::Challenge),
            "block" => Some(Self::Block),
            _ => None,
        }
    }
}

/// One finding: an axis, a verdict and the agent's explanation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub axis: Axis,
    pub verdict: Verdict,
    pub body: String,
}

/// A parsed review: the three findings and, when the issue needed it, a
/// proposed rewrite. `proposal` is `None` for the word `NONE`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub findings: Vec<Finding>,
    pub proposal: Option<String>,
}

/// Parses the agent's answer. Returns `None` when the answer does not carry the
/// three findings, so the UI can show it verbatim instead.
pub fn parse(text: &str) -> Option<Report> {
    let mut findings: Vec<Finding> = Vec::new();
    let mut current: Option<(Axis, Verdict)> = None;
    let mut body = String::new();
    let mut proposal_lines: Vec<&str> = Vec::new();
    let mut in_proposal = false;

    let flush =
        |findings: &mut Vec<Finding>, current: &mut Option<(Axis, Verdict)>, body: &mut String| {
            if let Some((axis, verdict)) = current.take() {
                findings.push(Finding {
                    axis,
                    verdict,
                    body: body.trim().to_string(),
                });
            }
            body.clear();
        };

    for line in text.lines() {
        let trimmed = line.trim();
        if in_proposal {
            proposal_lines.push(line);
            continue;
        }
        if let Some((key, rest)) = key_and_rest(trimmed) {
            if key == "PROPOSAL" {
                flush(&mut findings, &mut current, &mut body);
                in_proposal = true;
                let rest = rest.trim();
                if !rest.is_empty() {
                    proposal_lines.push(rest);
                }
                continue;
            }
            if let Some(axis) = Axis::from_key(&key)
                && let Some((verdict, note)) = verdict_and_note(rest)
            {
                flush(&mut findings, &mut current, &mut body);
                current = Some((axis, verdict));
                if !note.is_empty() {
                    body.push_str(note);
                    body.push('\n');
                }
                continue;
            }
        }
        if current.is_some() {
            body.push_str(line);
            body.push('\n');
        }
    }
    flush(&mut findings, &mut current, &mut body);

    let proposal = proposal_of(&proposal_lines.join("\n"));
    let complete = findings.len() == Axis::ALL.len();
    (complete || (opens_on_proposal(text) && proposal.is_some()))
        .then_some(Report { findings, proposal })
}

/// Whether the answer is a proposal alone, as a title-only issue gets: an
/// agent repeating the prompt, which opens on prose, is not one.
fn opens_on_proposal(text: &str) -> bool {
    text.lines()
        .find(|line| !line.trim().is_empty())
        .and_then(|line| key_and_rest(line.trim()))
        .is_some_and(|(key, _)| key == "PROPOSAL")
}

/// A proposed rewrite, unless it is empty or the word `NONE`.
pub(super) fn proposal_of(text: &str) -> Option<String> {
    let text = unfenced(text.trim());
    (!text.is_empty() && !text.eq_ignore_ascii_case("none")).then(|| text.to_string())
}

/// `text` without the one fence around all of it (```` ``` ````,
/// ```` ```markdown ```` or ```` ```md ````): agents sometimes wrap the whole
/// rewritten issue in one.
fn unfenced(text: &str) -> &str {
    let Some((first, rest)) = text.split_once('\n') else {
        return text;
    };
    let opening = first.trim().strip_prefix("```").map(str::trim);
    match (opening, rest.trim_end().strip_suffix("```")) {
        (Some("" | "markdown" | "md"), Some(inside)) => inside.trim(),
        _ => text,
    }
}

/// One turn of a refinement, as the challenger answered it.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Challenge {
    /// The global advice in a line or two, after `TLDR:`.
    pub tldr: Option<String>,
    pub text: String,
    /// Answers the author may pick, listed after `OPTIONS:`.
    pub options: Vec<String>,
    /// The refined issue once it proposes one, after `PROPOSAL:`.
    pub proposal: Option<String>,
}

/// The challenger's turn: `TLDR:` up to the first blank line, the challenge,
/// `OPTIONS:` one per line, then `PROPOSAL:` to the end. Each part may be
/// missing.
pub fn parse_refinement(text: &str) -> Challenge {
    let lines: Vec<&str> = text.lines().collect();
    let mut challenge = Challenge::default();
    let mut end = lines.len();
    if let Some((at, rest)) = key_line(&lines, &["PROPOSAL"]) {
        challenge.proposal = proposal_of(&[&[rest][..], &lines[at + 1..]].concat().join("\n"));
        end = at;
    }
    if let Some((at, rest)) = key_line(&lines[..end], &["OPTIONS"]) {
        challenge.options = [&[rest][..], &lines[at + 1..end]]
            .concat()
            .iter()
            .map(|line| list_item(line))
            .filter(|option| !option.is_empty() && !option.eq_ignore_ascii_case("none"))
            .map(str::to_string)
            .collect();
        end = at;
    }
    let mut body = lines[..end].to_vec();
    if let Some((at, rest)) = key_line(&body, &["TLDR", "TL;DR"]) {
        let after = body[at + 1..]
            .iter()
            .position(|line| line.trim().is_empty())
            .map_or(body.len(), |blank| at + 1 + blank);
        let tldr = [&[rest][..], &body[at + 1..after]].concat().join(" ");
        challenge.tldr = Some(tldr.trim().to_string()).filter(|tldr| !tldr.is_empty());
        body.drain(at..after);
    }
    challenge.text = body.join("\n").trim().to_string();
    challenge
}

/// A suggested answer as a title and its detail: split at the first `:` or
/// ` — `, the title's emphasis dropped. Without either, it is all title.
pub fn choice_parts(option: &str) -> (String, Option<&str>) {
    let split = [": ", " — "]
        .iter()
        .filter_map(|sep| option.find(sep).map(|at| (at, sep.len())))
        .min();
    let (title, detail) = match split {
        Some((at, len)) => (&option[..at], Some(option[at + len..].trim())),
        None => (option, None),
    };
    let title = title.replace("**", "").replace('`', "");
    (title.trim().to_string(), detail.filter(|d| !d.is_empty()))
}

/// A list line without its bullet or number: `- Yes` and `2. Yes` give `Yes`.
fn list_item(line: &str) -> &str {
    let line = line.trim();
    if let Some(rest) = ["- ", "* ", "+ "]
        .iter()
        .find_map(|bullet| line.strip_prefix(bullet))
    {
        return rest.trim_start();
    }
    let digits = line.len() - line.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    match line[digits..].strip_prefix(['.', ')']) {
        Some(rest) if digits > 0 => rest.trim_start(),
        _ => line,
    }
}

/// The first line of `lines` keyed by one of `keys`, and what follows its key.
fn key_line<'a>(lines: &[&'a str], keys: &[&str]) -> Option<(usize, &'a str)> {
    lines.iter().enumerate().find_map(|(at, line)| {
        key_and_rest(line.trim())
            .filter(|(key, _)| keys.contains(&key.as_str()))
            .map(|(_, rest)| (at, rest))
    })
}

/// A line's `KEY: rest`, the markdown around the key dropped: `**Size:** Pass`
/// and `## Size: Pass` both give `SIZE`. Keys are single words.
fn key_and_rest(line: &str) -> Option<(String, &str)> {
    let (head, rest) = line.split_once(':')?;
    let key = head.trim_matches(|c: char| c.is_whitespace() || "*_#-`".contains(c));
    if key.is_empty() || key.contains(char::is_whitespace) {
        return None;
    }
    let rest = rest.trim_start_matches(|c: char| c.is_whitespace() || "*_`".contains(c));
    Some((key.to_ascii_uppercase(), rest))
}

/// The verdict a finding's line starts with, and what follows it:
/// `**Challenge** — too big` gives Challenge and `too big`. The prompt's own
/// `Pass|Challenge|Block` is a list of choices, not a verdict.
fn verdict_and_note(rest: &str) -> Option<(Verdict, &str)> {
    let end = rest
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(rest.len());
    if rest[end..].trim_start().starts_with(['|', '/']) {
        return None;
    }
    let verdict = Verdict::parse(&rest[..end])?;
    let note = rest[end..]
        .trim_start_matches(|c: char| c.is_whitespace() || "*_`.,;:—–-".contains(c))
        .trim_end();
    Some((verdict, note))
}
