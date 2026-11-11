//! The board's spend feed: what the sessions running in its projects have
//! cost so far, asked of the service every 30 seconds.
//!
//! This is not the runs' own poller: that one watches a project every 700 ms
//! only while an agent works. The board's figure is there with no agent
//! running, and a group's board sums its projects from one light ask each.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;

use super::poller::status_of;
use super::service::listed;
use super::{Billing, Usage};

/// How often the board asks the service about its running sessions.
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

    /// Asks each of `projects` for its sessions; one that does not answer
    /// adds nothing.
    pub fn read(projects: &[PathBuf]) -> Self {
        let mut live = Live::default();
        for project in projects {
            if let Ok(sessions) = listed(project) {
                live.add(project, &sessions);
            }
        }
        live
    }

    /// Adds the running sessions of `project` in one session list.
    fn add(&mut self, project: &Path, sessions: &[Value]) {
        let project = std::fs::canonicalize(project).unwrap_or_else(|_| project.to_path_buf());
        for session in sessions {
            let Some(directory) = session["location"]["directory"].as_str() else {
                continue;
            };
            if Path::new(directory) != project || !status_of(session).running {
                continue;
            }
            self.sessions += 1;
            match Usage::from_session(session) {
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
    use serde_json::json;

    fn running(id: &str, project: &Path, provider: &str, cost: f64) -> Value {
        json!({
            "id": id,
            "location": {"directory": project},
            "model": {"id": "some-model", "providerID": provider},
            "cost": cost,
            "tokens": {"input": 10, "output": 2},
            "time": {"created": 1_000, "busy": 2_000},
        })
    }

    fn live_of(sessions: &[Value], project: &Path) -> Live {
        let mut live = Live::default();
        live.add(project, sessions);
        live
    }

    #[test]
    fn a_running_metered_session_adds_what_it_cost() {
        let live = live_of(
            &[running("ses_1", Path::new("/code/app"), "deepseek", 0.12)],
            Path::new("/code/app"),
        );
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
        let finished = json!({
            "id": "ses_done",
            "location": {"directory": "/code/app"},
            "cost": 0.42,
            "time": {"created": 1_000, "busy": 2_000, "idle": 3_000},
        });
        assert_eq!(
            live_of(&[finished], Path::new("/code/app")),
            Live::default()
        );
    }

    #[test]
    fn a_subscription_run_counts_as_running_but_its_dollars_are_not_spend() {
        let live = live_of(
            &[running(
                "ses_copilot",
                Path::new("/code/app"),
                "github-copilot",
                3.0,
            )],
            Path::new("/code/app"),
        );
        assert_eq!(
            live,
            Live {
                sessions: 1,
                cost: 0.0
            }
        );
        assert!(live.running(), "an agent is at work");
    }

    #[test]
    fn another_projects_session_is_left_out() {
        let elsewhere = running("ses_there", Path::new("/code/other"), "deepseek", 2.0);
        assert_eq!(
            live_of(&[elsewhere], Path::new("/code/app")),
            Live::default()
        );
    }
}
