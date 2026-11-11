//! A refinement before it is drawn: what the challenger and the author said,
//! the answer being written and the challenger's turn on its way. See
//! `design/components/RefinePanel`.

use std::path::PathBuf;
use std::time::Duration;

use crate::agent::{self, Challenge};
use crate::job::{self, Job};

/// One turn of a refinement: the challenger's, or the author's answer.
pub enum Turn {
    Challenger(Challenge),
    /// The full text of the choice picked, if any, and the author's own words.
    Author {
        choice: Option<String>,
        note: String,
    },
}

impl Turn {
    pub fn speaker(&self) -> &'static str {
        match self {
            Turn::Challenger(_) => "Challenger",
            Turn::Author { .. } => "Author",
        }
    }

    /// The turn as the agent reads it again.
    pub fn said(&self) -> String {
        match self {
            Turn::Challenger(challenge) => match &challenge.tldr {
                Some(tldr) => format!("TLDR: {tldr}\n\n{}", challenge.text),
                None => challenge.text.clone(),
            },
            Turn::Author { choice, note } => match (choice, note.as_str()) {
                (Some(choice), "") => choice.clone(),
                (Some(choice), note) => format!("{choice}\n\n{note}"),
                (None, note) => note.to_string(),
            },
        }
    }

    pub fn challenge(&self) -> Option<&Challenge> {
        match self {
            Turn::Challenger(challenge) => Some(challenge),
            Turn::Author { .. } => None,
        }
    }
}

/// The challenger's next turn: none asked, on its way, or failed.
pub enum NextTurn {
    Idle,
    Waiting(Job<Result<String, String>>),
    Failed(String),
}

/// The refinement of one issue: what was said, and the turn on its way.
pub struct Refinement {
    pub path: PathBuf,
    /// The configured tool (and model), shown in the panel header.
    pub agent: String,
    pub turns: Vec<Turn>,
    /// The author's own words: a note to the choice, or the whole answer.
    pub note: String,
    /// The choice picked among the challenger's last options.
    pub choice: Option<usize>,
    /// The choice the arrow keys are on.
    pub cursor: Option<usize>,
    /// On a narrow panel, the reasoning unfolded under the TL;DR.
    pub reasoning_open: bool,
    pub next: NextTurn,
    /// Tucked away to read the page; the conversation goes on behind.
    pub hidden: bool,
}

impl Refinement {
    pub fn new(path: PathBuf, agent: String) -> Self {
        Self {
            path,
            agent,
            turns: Vec::new(),
            note: String::new(),
            choice: None,
            cursor: None,
            reasoning_open: false,
            next: NextTurn::Idle,
            hidden: false,
        }
    }

    pub fn is_running(&self) -> bool {
        matches!(self.next, NextTurn::Waiting(_))
    }

    /// The round on screen: one more than the answers sent, counting the
    /// one the challenger is reading.
    pub fn round(&self) -> usize {
        let sent = self
            .turns
            .iter()
            .filter(|turn| matches!(turn, Turn::Author { .. }))
            .count();
        if self.is_running() {
            sent.max(1)
        } else {
            sent + 1
        }
    }

    /// The resolutions the challenger's last turn offers, while it waits.
    pub fn options(&self) -> &[String] {
        match self.turns.last().and_then(Turn::challenge) {
            Some(challenge) if !self.is_running() => &challenge.options,
            _ => &[],
        }
    }

    /// The latest refined issue the challenger proposed.
    pub fn proposal(&self) -> Option<&str> {
        self.turns
            .iter()
            .rev()
            .find_map(|turn| turn.challenge()?.proposal.as_deref())
    }

    /// What Send hands the challenger: the choice's full text, then the note.
    pub fn answer(&self) -> Option<Turn> {
        let choice = self.choice.and_then(|at| self.options().get(at)).cloned();
        let note = self.note.trim().to_string();
        (choice.is_some() || !note.is_empty()).then_some(Turn::Author { choice, note })
    }

    /// Send's label, naming what goes out.
    pub fn send_label(&self) -> String {
        let choice = self.choice.filter(|at| *at < self.options().len());
        match (choice, self.note.trim().is_empty()) {
            (Some(at), true) => format!("Send choice {}", at + 1),
            (Some(at), false) => format!("Send choice {} with note", at + 1),
            (None, _) => "Send answer".to_string(),
        }
    }

    /// Picks a choice, or unpicks it when it is the one picked.
    pub fn toggle_choice(&mut self, index: usize) {
        if index < self.options().len() {
            self.choice = (self.choice != Some(index)).then_some(index);
        }
    }

    /// Moves the cursor to the next choice, or onto the first.
    pub fn cursor_down(&mut self) {
        let Some(last) = self.options().len().checked_sub(1) else {
            return;
        };
        self.cursor = Some(self.cursor.map_or(0, |at| (at + 1).min(last)));
    }

    /// Moves the cursor to the previous choice, or onto the first.
    pub fn cursor_up(&mut self) {
        if !self.options().is_empty() {
            self.cursor = Some(self.cursor.map_or(0, |at| at.saturating_sub(1)));
        }
    }

    /// Picks or unpicks the choice under the cursor; false without one.
    pub fn toggle_at_cursor(&mut self) -> bool {
        let Some(last) = self.options().len().checked_sub(1) else {
            return false;
        };
        let Some(at) = self.cursor else {
            return false;
        };
        self.toggle_choice(at.min(last));
        true
    }

    /// Sends the answer, once the challenger is done speaking: it joins the
    /// turns and the choice and the note clear. False with nothing to send.
    pub fn take_answer(&mut self) -> bool {
        let Some(answer) = self.answer().filter(|_| !self.is_running()) else {
            return false;
        };
        self.note.clear();
        self.choice = None;
        self.cursor = None;
        self.turns.push(answer);
        true
    }

    /// Adopts the challenger's turn once it is in. While it is on its way,
    /// says when to look again.
    pub fn adopt(&mut self) -> Option<Duration> {
        job::land(
            self,
            |refine| match &refine.next {
                NextTurn::Waiting(answer) => Some(answer),
                _ => None,
            },
            |refine, result| {
                refine.next = match result {
                    Ok(raw) => {
                        refine.reasoning_open = false;
                        refine
                            .turns
                            .push(Turn::Challenger(agent::parse_refinement(&raw)));
                        NextTurn::Idle
                    }
                    Err(error) => NextTurn::Failed(error),
                };
            },
        )
    }

    /// Stops the challenger's turn (dropping the job stops the agent): the
    /// author's last answer comes back, its choice picked again. True when
    /// nothing was said yet, so the refinement closes.
    pub fn stop(&mut self) -> bool {
        self.next = NextTurn::Idle;
        if let Some(Turn::Author { choice, note }) = self
            .turns
            .pop_if(|turn| matches!(turn, Turn::Author { .. }))
        {
            self.choice = choice
                .and_then(|choice| self.options().iter().position(|option| *option == choice));
            self.note = note;
        }
        self.turns.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refinement(options: &[&str]) -> Refinement {
        let mut refine = Refinement::new(PathBuf::from("todo/x.md"), "claude".to_string());
        refine.turns.push(Turn::Challenger(Challenge {
            text: "Why?".to_string(),
            options: options.iter().map(ToString::to_string).collect(),
            ..Challenge::default()
        }));
        refine
    }

    #[test]
    fn send_names_what_goes_out() {
        let mut refine = refinement(&["Split it", "Drop it"]);
        assert_eq!(refine.send_label(), "Send answer");
        refine.toggle_choice(1);
        assert_eq!(refine.send_label(), "Send choice 2");
        refine.note = "  but soon ".to_string();
        assert_eq!(refine.send_label(), "Send choice 2 with note");
        refine.toggle_choice(1);
        assert_eq!(refine.send_label(), "Send answer");
        refine.toggle_choice(5);
        assert_eq!(refine.choice, None, "no such choice");
    }

    #[test]
    fn the_round_counts_the_answers_sent() {
        let mut refine = refinement(&["Split it"]);
        assert_eq!(refine.round(), 1);
        refine.toggle_choice(0);
        assert!(refine.take_answer());
        assert_eq!((refine.choice, refine.note.as_str()), (None, ""));
        assert_eq!(refine.round(), 2);
        refine.next = NextTurn::Waiting(Job::done(Ok("TLDR: Go.\n\nFine.".to_string())));
        assert_eq!(refine.round(), 1, "the challenger reads the first answer");
        assert!(refine.options().is_empty());
        assert_eq!(refine.adopt(), None);
        assert!(matches!(refine.next, NextTurn::Idle));
        assert_eq!(refine.round(), 2);
    }

    #[test]
    fn nothing_to_send_sends_nothing() {
        let mut refine = refinement(&["Split it"]);
        refine.note = "   ".to_string();
        assert!(!refine.take_answer());
        assert_eq!(refine.turns.len(), 1);
    }

    #[test]
    fn a_failed_turn_waits_for_a_retry() {
        let mut refine = refinement(&[]);
        refine.next = NextTurn::Waiting(Job::done(Err("offline".to_string())));
        assert_eq!(refine.adopt(), None);
        assert!(matches!(&refine.next, NextTurn::Failed(error) if error == "offline"));
    }

    #[test]
    fn stop_brings_the_last_answer_back() {
        let mut refine = refinement(&["Split it", "Drop it"]);
        refine.toggle_choice(1);
        refine.note = "with care".to_string();
        assert!(refine.take_answer());
        refine.next = NextTurn::Waiting(Job::spawn(|_| Ok(String::new())));
        assert!(!refine.stop());
        assert!(matches!(refine.next, NextTurn::Idle));
        assert_eq!(refine.turns.len(), 1);
        assert_eq!(refine.choice, Some(1));
        assert_eq!(refine.note, "with care");
    }

    #[test]
    fn stop_before_the_first_word_closes() {
        let mut refine = Refinement::new(PathBuf::from("todo/x.md"), "claude".to_string());
        refine.next = NextTurn::Waiting(Job::spawn(|_| Ok(String::new())));
        assert!(refine.stop());
    }

    #[test]
    fn the_cursor_stays_on_the_choices() {
        let mut refine = refinement(&["Split it", "Drop it"]);
        assert!(!refine.toggle_at_cursor());
        refine.cursor_up();
        assert_eq!(refine.cursor, Some(0));
        refine.cursor_down();
        refine.cursor_down();
        assert_eq!(refine.cursor, Some(1));
        assert!(refine.toggle_at_cursor());
        assert_eq!(refine.choice, Some(1));
        let mut empty = refinement(&[]);
        empty.cursor_down();
        assert_eq!(empty.cursor, None);
    }
}
