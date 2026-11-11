//! The board's spend feed: what the sessions running in its projects have
//! cost so far, asked of every service every 30 seconds.
//!
//! This is not the runs' own poller: that one watches a project every 700 ms
//! only while an agent works. The board's figure is there with no agent
//! running, and a group's board sums its projects from one light ask each.

use std::path::PathBuf;
use std::time::Duration;

use super::service::Session;
use super::{Billing, services};

/// How often the board asks the services about their running sessions.
pub const EVERY: Duration = Duration::from_secs(30);

/// What the sessions running in a board's projects have cost so far.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Live {
    /// How many sessions are running.
    pub sessions: usize,
    /// What their metered runs cost so far; a subscription's notional
    /// dollars are never spend.
    pub cost: f64,
}

impl Live {
    /// Whether an agent is at work in one of the projects.
    pub fn running(self) -> bool {
        self.sessions > 0
    }

    /// Asks each service for each of `projects`; one that does not answer
    /// adds nothing.
    pub fn read(projects: &[PathBuf]) -> Self {
        let mut live = Live::default();
        for service in services() {
            for project in projects {
                if let Ok(sessions) = service.sessions(project) {
                    live.add(&sessions);
                }
            }
        }
        live
    }

    /// Adds the running sessions of one project.
    fn add(&mut self, sessions: &[Session]) {
        for session in sessions.iter().filter(|session| session.status.running) {
            self.sessions += 1;
            match &session.usage {
                Some(usage) if usage.billing == Billing::Metered => {
                    self.cost += usage.cost.unwrap_or(0.0);
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::agent::runtime::service::Status;
    use crate::domains::agent::runtime::{Tokens, Usage};

    fn session(id: &str, provider: &str, cost: f64) -> Session {
        Session {
            id: id.to_string(),
            slug: None,
            status: Status {
                running: true,
                created: 1_000,
                updated: 2_000,
            },
            ended: 2_000,
            usage: Some(Usage {
                cost: Some(cost),
                tokens: Tokens::default(),
                provider: Some(provider.to_string()),
                model: Some("some-model".to_string()),
                billing: super::super::usage::billing_of(Some(provider)),
            }),
        }
    }

    fn live_of(sessions: &[Session]) -> Live {
        let mut live = Live::default();
        live.add(sessions);
        live
    }

    #[test]
    fn a_running_metered_session_adds_what_it_cost() {
        let live = live_of(&[session("ses_1", "deepseek", 0.12)]);
        assert_eq!(
            live,
            Live {
                sessions: 1,
                cost: 0.12
            }
        );
        assert!(live.running());
    }

    #[test]
    fn a_finished_session_adds_nothing() {
        let mut finished = session("ses_done", "deepseek", 0.42);
        finished.status.running = false;
        assert_eq!(live_of(&[finished]), Live::default());
    }

    #[test]
    fn a_subscription_run_counts_as_running_but_its_dollars_are_not_spend() {
        let live = live_of(&[session("ses_copilot", "github-copilot", 3.0)]);
        assert_eq!(
            live,
            Live {
                sessions: 1,
                cost: 0.0
            }
        );
        assert!(live.running(), "an agent is at work");
    }
}
