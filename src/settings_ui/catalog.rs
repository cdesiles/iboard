//! The installed tools and their models, probed in the background once a session.

use std::collections::HashMap;

use crate::agent::{self, Detected, Models, Tool};
use crate::job::{Job, Poll};

/// A value computed on another thread, kept once it lands.
enum Pending<T> {
    Loading(Job<T>),
    Ready(T),
}

impl<T: Send + 'static> Pending<T> {
    fn spawn(work: impl FnOnce() -> T + Send + 'static) -> Self {
        Pending::Loading(Job::spawn(move |_| work()))
    }

    /// Adopts the result once it lands; `lost` stands in for a thread that
    /// died without answering.
    fn poll(&mut self, lost: impl FnOnce() -> T) {
        if let Pending::Loading(job) = self {
            match job.poll() {
                Poll::Ready(value) => *self = Pending::Ready(value),
                Poll::Lost => *self = Pending::Ready(lost()),
                Poll::Waiting => {}
            }
        }
    }

    fn ready(&self) -> Option<&T> {
        match self {
            Pending::Ready(value) => Some(value),
            Pending::Loading(_) => None,
        }
    }
}

/// The installed tools and their models, gathered once per session.
#[derive(Default)]
pub struct Catalog {
    /// Run the tools to learn about them. Off in tests, which then see every
    /// tool as missing with no models.
    probe: bool,
    tools: HashMap<Tool, Pending<Detected>>,
    models: HashMap<Tool, Pending<Models>>,
}

impl Catalog {
    /// A catalog that runs the installed tools.
    pub fn probing() -> Self {
        Self {
            probe: true,
            ..Self::default()
        }
    }

    /// Starts looking for every tool, and listing the models of `tool`, unless
    /// that is already known or under way.
    pub fn load(&mut self, tool: Tool) {
        let probe = self.probe;
        for known in Tool::ALL.into_iter().filter(|t| t.binary().is_some()) {
            self.tools.entry(known).or_insert_with(|| {
                if probe {
                    Pending::spawn(move || agent::detect(known))
                } else {
                    Pending::Ready(Detected::default())
                }
            });
        }
        if tool.binary().is_some() {
            self.models.entry(tool).or_insert_with(|| {
                if probe {
                    Pending::spawn(move || agent::list_models(tool))
                } else {
                    Pending::Ready(Models {
                        list: Ok(Vec::new()),
                        default: None,
                    })
                }
            });
        }
    }

    pub(super) fn retry_models(&mut self, tool: Tool) {
        self.models.remove(&tool);
        self.load(tool);
    }

    /// Adopts finished work; true while some is still running.
    pub fn poll(&mut self) -> bool {
        for pending in self.tools.values_mut() {
            pending.poll(Detected::default);
        }
        for pending in self.models.values_mut() {
            pending.poll(|| Models {
                list: Err("the model list could not be read".to_string()),
                default: None,
            });
        }
        self.tools
            .values()
            .any(|pending| matches!(pending, Pending::Loading(_)))
            || self
                .models
                .values()
                .any(|pending| matches!(pending, Pending::Loading(_)))
    }

    pub(super) fn detected(&self, tool: Tool) -> Option<&Detected> {
        self.tools.get(&tool).and_then(Pending::ready)
    }

    pub(super) fn models(&self, tool: Tool) -> Option<&Models> {
        self.models.get(&tool).and_then(Pending::ready)
    }
}
