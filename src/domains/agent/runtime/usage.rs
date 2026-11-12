//! What a run spent, as its engine reports it: money, tokens, provider and
//! model, and whether the money was metered or came out of a subscription.
//!
//! OpenCode reports it on every session; Claude Code on the `result` event of
//! its stream.
//!
//! A provider's [`Plan`] says how its runs are read: metered, where the log's
//! dollars are money spent, or a subscription, where they are API-equivalent.
//! The `[usage]` section of the user configuration declares plans; the four
//! known providers have defaults, and anything else is metered with no cap.

use std::collections::BTreeMap;

use chrono::{DateTime, Datelike, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The five token counts an engine reports.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tokens {
    pub input: u64,
    pub output: u64,
    pub reasoning: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

impl Tokens {
    /// Every token the run used, cache included.
    pub fn total(self) -> u64 {
        self.input + self.output + self.reasoning + self.cache_read + self.cache_write
    }
}

/// Whether a run's dollars were taken, or are a subscription's API-equivalent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Billing {
    #[default]
    Metered,
    Subscription,
}

impl Billing {
    /// The word `[usage]` stores.
    pub fn key(self) -> &'static str {
        match self {
            Billing::Metered => "metered",
            Billing::Subscription => "subscription",
        }
    }

    /// The word Settings › AI shows.
    pub fn name(self) -> &'static str {
        match self {
            Billing::Metered => "Metered",
            Billing::Subscription => "Subscription",
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        match key.trim().to_ascii_lowercase().as_str() {
            "metered" => Some(Billing::Metered),
            "subscription" => Some(Billing::Subscription),
            _ => None,
        }
    }
}

/// What a plan's cap fills over, or a window a provider reports.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Window {
    #[default]
    Day,
    Week,
    Month,
    /// Claude.ai's rolling five-hour session, which only a provider report
    /// anchors: [`Window::start_before`] reads it from the reset.
    Session,
}

impl Window {
    /// The windows a plan may declare. A session is provider-anchored, so it
    /// is never a declared cap.
    pub const ALL: [Window; 3] = [Window::Day, Window::Week, Window::Month];

    /// The word `[usage]` stores.
    pub fn key(self) -> &'static str {
        match self {
            Window::Day => "day",
            Window::Week => "week",
            Window::Month => "month",
            Window::Session => "session",
        }
    }

    /// The word Settings › AI and the meters show.
    pub fn name(self) -> &'static str {
        match self {
            Window::Day => "Day",
            Window::Week => "Week",
            Window::Month => "Month",
            Window::Session => "Session",
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        match key.trim().to_ascii_lowercase().as_str() {
            "day" => Some(Window::Day),
            "week" => Some(Window::Week),
            "month" => Some(Window::Month),
            _ => None,
        }
    }

    /// When the window holding `at` began, in UTC. A session is only anchored
    /// by a provider's reset, so this reads it as one starting now.
    pub fn start(self, at: DateTime<Utc>) -> DateTime<Utc> {
        if self == Window::Session {
            return at;
        }
        let date = at.date_naive();
        let first = match self {
            Window::Day | Window::Session => date,
            Window::Week => {
                date - chrono::Duration::days(i64::from(date.weekday().num_days_from_monday()))
            }
            Window::Month => date.with_day(1).unwrap_or(date),
        };
        DateTime::from_naive_utc_and_offset(first.and_time(NaiveTime::MIN), Utc)
    }

    /// When the window holding `at` ends and its next one begins, in UTC.
    pub fn reset(self, at: DateTime<Utc>) -> DateTime<Utc> {
        if self == Window::Session {
            return at + chrono::Duration::hours(5);
        }
        let start = self.start(at);
        let date = start.date_naive();
        let next = match self {
            Window::Day => date + chrono::Duration::days(1),
            Window::Week => date + chrono::Duration::days(7),
            Window::Month => date
                .checked_add_months(chrono::Months::new(1))
                .unwrap_or(date),
            Window::Session => date,
        };
        DateTime::from_naive_utc_and_offset(next.and_time(NaiveTime::MIN), Utc)
    }

    /// When the window that ends at `reset` began: five hours for a session,
    /// seven days for a week, and so on. A provider's own reset need not fall
    /// on a calendar boundary.
    pub fn start_before(self, reset: DateTime<Utc>) -> DateTime<Utc> {
        match self {
            Window::Session => reset - chrono::Duration::hours(5),
            Window::Day => reset - chrono::Duration::days(1),
            Window::Week => reset - chrono::Duration::days(7),
            Window::Month => reset
                .checked_sub_months(chrono::Months::new(1))
                .unwrap_or(reset),
        }
    }
}

/// A cap a plan declares: money, or tokens. A plan has at most one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cap {
    Money(f64),
    Tokens(u64),
}

/// What a provider is, and what its window holds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plan {
    pub billing: Billing,
    pub window: Window,
    /// What the plan costs over its window, when the user knows.
    pub price: Option<f64>,
    /// What its window may use, when a cap is known.
    pub cap: Option<Cap>,
}

impl Default for Plan {
    /// What an unknown provider is: metered, with no cap.
    fn default() -> Self {
        Plan {
            billing: Billing::Metered,
            window: Window::Day,
            price: None,
            cap: None,
        }
    }
}

impl Plan {
    /// The default plan of `provider`; metered with no cap when it is unknown.
    pub fn default_for(provider: &str) -> Self {
        match Provider::from_key(provider) {
            Some(provider) => provider.default_plan(),
            None => Plan::default(),
        }
    }
}

/// The providers iboard knows a default plan for, in Settings' order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Anthropic,
    DeepSeek,
    Claude,
    Copilot,
}

impl Provider {
    pub const ALL: [Provider; 4] = [
        Provider::Anthropic,
        Provider::DeepSeek,
        Provider::Claude,
        Provider::Copilot,
    ];

    /// The provider ID the usage log stores, normalized.
    pub fn key(self) -> &'static str {
        match self {
            Provider::Anthropic => "anthropic",
            Provider::DeepSeek => "deepseek",
            Provider::Claude => "claude",
            Provider::Copilot => "copilot",
        }
    }

    /// The name Settings and the meters show.
    pub fn name(self) -> &'static str {
        match self {
            Provider::Anthropic => "Anthropic API",
            Provider::DeepSeek => "DeepSeek",
            Provider::Claude => "Claude.ai",
            Provider::Copilot => "Copilot",
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        match provider_key(key).as_str() {
            "anthropic" => Some(Provider::Anthropic),
            "deepseek" => Some(Provider::DeepSeek),
            "claude" => Some(Provider::Claude),
            "copilot" => Some(Provider::Copilot),
            _ => None,
        }
    }

    fn default_plan(self) -> Plan {
        match self {
            Provider::Anthropic | Provider::DeepSeek => Plan::default(),
            Provider::Claude => Plan {
                billing: Billing::Subscription,
                window: Window::Week,
                price: None,
                cap: None,
            },
            Provider::Copilot => Plan {
                billing: Billing::Subscription,
                window: Window::Month,
                price: None,
                cap: None,
            },
        }
    }
}

/// A provider ID as plans and the usage log name it: lowercased, with the
/// aliases the engines use folded together.
pub fn provider_key(provider: &str) -> String {
    match provider.trim().to_ascii_lowercase().as_str() {
        "github-copilot" | "github_copilot" => "copilot".to_string(),
        "claude-ai" | "claude.ai" | "claudeai" => "claude".to_string(),
        key => key.to_string(),
    }
}

/// The name a provider is shown with: a known provider's, or its key.
pub fn provider_name(provider: &str) -> String {
    match Provider::from_key(provider) {
        Some(provider) => provider.name().to_string(),
        None => provider_key(provider),
    }
}

/// The plans the user configuration overrides, by normalized provider ID.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plans {
    overrides: BTreeMap<String, Plan>,
}

impl Plans {
    /// The plan `provider` is read with: its override, or its default.
    pub fn plan(&self, provider: &str) -> Plan {
        let key = provider_key(provider);
        self.overrides
            .get(&key)
            .copied()
            .unwrap_or_else(|| Plan::default_for(&key))
    }

    /// Declares `plan` for `provider`; a plan equal to its default is removed,
    /// so the file never repeats what iboard already knows.
    pub fn set(&mut self, provider: &str, plan: Plan) {
        let key = provider_key(provider);
        if plan == Plan::default_for(&key) {
            self.overrides.remove(&key);
        } else {
            self.overrides.insert(key, plan);
        }
    }

    /// Every declared plan, including providers iboard does not know.
    pub fn overrides(&self) -> &BTreeMap<String, Plan> {
        &self.overrides
    }

    /// Whether the file declares nothing, so every provider has its default.
    pub fn is_empty(&self) -> bool {
        self.overrides.is_empty()
    }

    /// The four known providers and the plan each is read with.
    pub fn known(&self) -> [(Provider, Plan); 4] {
        Provider::ALL.map(|provider| (provider, self.plan(provider.key())))
    }
}

/// The `[usage]` section as written on disk.
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(transparent)]
pub struct UsageSection(BTreeMap<String, RawPlan>);

#[derive(Debug, Default, Deserialize, Serialize)]
struct RawPlan {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    kind: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    window: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    price: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    money_cap: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    token_cap: Option<u64>,
}

impl UsageSection {
    /// Reads the plans the section declares. A value iboard cannot use keeps
    /// the provider's default and is reported.
    pub fn plans(self) -> (Plans, Vec<String>) {
        let mut plans = Plans::default();
        let mut problems = Vec::new();
        for (provider, raw) in self.0 {
            let (plan, found) = raw.plan(&provider);
            plans.set(&provider, plan);
            problems.extend(found);
        }
        (plans, problems)
    }
}

impl RawPlan {
    fn plan(self, provider: &str) -> (Plan, Vec<String>) {
        let mut plan = Plan::default_for(provider);
        let mut problems = Vec::new();
        let key = provider_key(provider);
        if !self.kind.trim().is_empty() {
            match Billing::from_key(&self.kind) {
                Some(billing) => plan.billing = billing,
                None => problems.push(format!(
                    "[usage.{key}]: kind \"{}\" is not metered or subscription",
                    self.kind
                )),
            }
        }
        if !self.window.trim().is_empty() {
            match Window::from_key(&self.window) {
                Some(window) => plan.window = window,
                None => problems.push(format!(
                    "[usage.{key}]: window \"{}\" is not day, week or month",
                    self.window
                )),
            }
        }
        if let Some(price) = self.price {
            if price < 0.0 {
                problems.push(format!("[usage.{key}]: price {price} is negative"));
            } else {
                plan.price = Some(price);
            }
        }
        match (self.money_cap, self.token_cap) {
            (Some(_), Some(_)) => {
                problems.push(format!(
                    "[usage.{key}]: money_cap and token_cap are both set"
                ));
            }
            (Some(cap), None) if cap < 0.0 => {
                problems.push(format!("[usage.{key}]: money_cap {cap} is negative"));
            }
            (Some(cap), None) => plan.cap = Some(Cap::Money(cap)),
            (None, Some(cap)) => plan.cap = Some(Cap::Tokens(cap)),
            (None, None) => {}
        }
        (plan, problems)
    }

    fn of(plan: &Plan) -> Self {
        Self {
            kind: plan.billing.key().to_string(),
            window: plan.window.key().to_string(),
            price: plan.price,
            money_cap: match plan.cap {
                Some(Cap::Money(cap)) => Some(cap),
                _ => None,
            },
            token_cap: match plan.cap {
                Some(Cap::Tokens(cap)) => Some(cap),
                _ => None,
            },
        }
    }
}

/// The `[usage]` section that stores every declared plan.
pub fn usage_section(plans: &Plans) -> UsageSection {
    UsageSection(
        plans
            .overrides()
            .iter()
            .map(|(provider, plan)| (provider.clone(), RawPlan::of(plan)))
            .collect(),
    )
}

/// What one run cost.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Usage {
    pub cost: Option<f64>,
    pub tokens: Tokens,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub billing: Billing,
}

impl Usage {
    /// The cost as the build log writes it: `$0.42`, or `$0.42 API-equivalent`
    /// when the run came out of a subscription.
    pub fn cost_label(&self) -> Option<String> {
        self.cost.map(|cost| match self.billing {
            Billing::Metered => format!("${cost:.2}"),
            Billing::Subscription => format!("${cost:.2} API-equivalent"),
        })
    }

    /// The last usage an agent reported in its stream, when it reported one.
    pub(super) fn from_stream(stdout: &str) -> Option<Self> {
        let mut model = None;
        let mut usage = None;
        for line in stdout.lines() {
            let Ok(event) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            match event["type"].as_str() {
                Some("system") if event["subtype"] == "init" => {
                    model = text(&event["model"]).or(model);
                }
                Some("result") => {
                    if let Some(mut found) = from_result(&event) {
                        found.model = found.model.or_else(|| model.clone());
                        usage = Some(found);
                    }
                }
                Some("step_finish") => {
                    if let Some(mut found) = from_step_finish(&event) {
                        found.model = found.model.or_else(|| model.clone());
                        usage = Some(found);
                    }
                }
                _ => {}
            }
        }
        usage
    }
}

pub(super) fn text(value: &Value) -> Option<String> {
    value
        .as_str()
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

pub(super) fn tokens_of(value: &Value) -> Tokens {
    Tokens {
        input: value["input"].as_u64().unwrap_or(0),
        output: value["output"].as_u64().unwrap_or(0),
        reasoning: value["reasoning"].as_u64().unwrap_or(0),
        cache_read: value["cache"]["read"].as_u64().unwrap_or(0),
        cache_write: value["cache"]["write"].as_u64().unwrap_or(0),
    }
}

pub(super) fn billing_of(provider: Option<&str>) -> Billing {
    match provider {
        Some("github-copilot" | "copilot") => Billing::Subscription,
        _ => Billing::Metered,
    }
}

/// Claude Code's `result` event: `total_cost_usd` and its `usage`.
fn from_result(event: &Value) -> Option<Usage> {
    let usage = &event["usage"];
    let cost = event["total_cost_usd"].as_f64();
    (cost.is_some() || !usage.is_null()).then(|| {
        let model = text(&event["model"])
            .or_else(|| event["modelUsage"].as_object()?.keys().next().cloned());
        let provider = text(&event["provider"]).unwrap_or_else(|| "anthropic".to_string());
        Usage {
            cost,
            tokens: Tokens {
                input: usage["input_tokens"].as_u64().unwrap_or(0),
                output: usage["output_tokens"].as_u64().unwrap_or(0),
                reasoning: usage["reasoning_output_tokens"]
                    .as_u64()
                    .or_else(|| usage["reasoning_tokens"].as_u64())
                    .unwrap_or(0),
                cache_read: usage["cache_read_input_tokens"].as_u64().unwrap_or(0),
                cache_write: usage["cache_creation_input_tokens"].as_u64().unwrap_or(0),
            },
            billing: billing_of(Some(&provider)),
            provider: Some(provider),
            model,
        }
    })
}

/// OpenCode's `step_finish` event, for a one-shot `run --format json`.
fn from_step_finish(event: &Value) -> Option<Usage> {
    let part = &event["part"];
    let cost = part["cost"].as_f64().or_else(|| event["cost"].as_f64());
    let tokens = if part["tokens"].is_null() {
        &event["tokens"]
    } else {
        &part["tokens"]
    };
    (cost.is_some() || !tokens.is_null()).then(|| {
        let provider =
            text(&part["model"]["providerID"]).or_else(|| text(&event["model"]["providerID"]));
        let model = text(&part["model"]["id"]).or_else(|| text(&event["model"]["id"]));
        Usage {
            cost,
            tokens: tokens_of(tokens),
            billing: billing_of(provider.as_deref()),
            provider,
            model,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Claude Code's init and result events, trimmed to what usage reads.
    const CLAUDE_STREAM: &str = concat!(
        r#"{"type":"system","subtype":"init","session_id":"abc","model":"claude-sonnet-4-5"}"#,
        "\n",
        r#"{"type":"result","subtype":"success","is_error":false,"result":"Done.","session_id":"abc","total_cost_usd":0.42,"usage":{"input_tokens":1234,"output_tokens":56,"cache_read_input_tokens":890,"cache_creation_input_tokens":12}}"#,
    );

    #[test]
    fn a_claude_result_reads_its_cost_and_usage() {
        let usage = Usage::from_stream(CLAUDE_STREAM).expect("usage");
        assert_eq!(usage.cost, Some(0.42));
        assert_eq!(
            usage.tokens,
            Tokens {
                input: 1234,
                output: 56,
                reasoning: 0,
                cache_read: 890,
                cache_write: 12,
            }
        );
        assert_eq!(usage.provider.as_deref(), Some("anthropic"));
        assert_eq!(usage.model.as_deref(), Some("claude-sonnet-4-5"));
        assert_eq!(usage.billing, Billing::Metered);
    }

    #[test]
    fn an_opencode_step_finish_reads_its_cost_and_usage() {
        let stream = r#"{"type":"step_finish","part":{"type":"step-finish","cost":0.01,"tokens":{"input":10,"output":2,"reasoning":1,"cache":{"read":3,"write":4}}}}"#;
        let usage = Usage::from_stream(stream).expect("usage");
        assert_eq!(usage.cost, Some(0.01));
        assert_eq!(
            usage.tokens,
            Tokens {
                input: 10,
                output: 2,
                reasoning: 1,
                cache_read: 3,
                cache_write: 4,
            }
        );
    }

    #[test]
    fn plain_text_has_no_usage() {
        assert_eq!(Usage::from_stream("all done\n"), None);
        assert_eq!(Usage::from_stream(""), None);
    }

    #[test]
    fn the_known_providers_have_the_default_plans() {
        let plans = Plans::default();
        assert_eq!(plans.plan("anthropic"), Plan::default());
        assert_eq!(plans.plan("deepseek"), Plan::default());
        assert_eq!(
            plans.plan("claude"),
            Plan {
                billing: Billing::Subscription,
                window: Window::Week,
                price: None,
                cap: None,
            }
        );
        assert_eq!(
            plans.plan("github-copilot"),
            Plan {
                billing: Billing::Subscription,
                window: Window::Month,
                price: None,
                cap: None,
            },
            "the engine's ID and the plan's agree"
        );
    }

    #[test]
    fn an_unknown_provider_is_metered_with_no_cap() {
        let plan = Plans::default().plan("some-new-provider");
        assert_eq!(plan, Plan::default());
        assert_eq!((plan.billing, plan.cap), (Billing::Metered, None));
    }

    #[test]
    fn a_section_reads_the_plan_and_its_cap() {
        let text = "[claude]\nkind = \"subscription\"\nwindow = \"month\"\n\
                    price = 20.0\ntoken_cap = 1000000\n\
                    [deepseek]\nkind = \"metered\"\nmoney_cap = 5.0\n";
        let (plans, problems) = toml::from_str::<UsageSection>(text).unwrap().plans();
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(
            plans.plan("claude"),
            Plan {
                billing: Billing::Subscription,
                window: Window::Month,
                price: Some(20.0),
                cap: Some(Cap::Tokens(1_000_000)),
            }
        );
        assert_eq!(
            plans.plan("deepseek").cap,
            Some(Cap::Money(5.0)),
            "an unknown field keeps its default"
        );
        assert_eq!(plans.plan("deepseek").billing, Billing::Metered);
    }

    #[test]
    fn a_plan_iboard_cannot_use_keeps_its_default_and_is_reported() {
        let text = "[claude]\nkind = \"trial\"\nwindow = \"year\"\nprice = -1.0\n\
                    money_cap = 1.0\ntoken_cap = 2\n";
        let (plans, problems) = toml::from_str::<UsageSection>(text).unwrap().plans();
        assert_eq!(plans.plan("claude"), Plan::default_for("claude"));
        assert_eq!(problems.len(), 4, "{problems:?}");
        assert!(
            problems[0].starts_with("[usage.claude]: kind"),
            "{problems:?}"
        );
        assert!(
            problems[1].starts_with("[usage.claude]: window"),
            "{problems:?}"
        );
        assert!(
            problems[2].starts_with("[usage.claude]: price"),
            "{problems:?}"
        );
        assert!(
            problems[3].contains("both set"),
            "a plan has either a money or a token cap: {problems:?}"
        );
    }

    #[test]
    fn a_plan_writes_back_as_it_was_read() {
        let text = "[claude]\nkind = \"subscription\"\nwindow = \"week\"\nprice = 20.0\n\
                    token_cap = 1000000\n";
        let (plans, problems) = toml::from_str::<UsageSection>(text).unwrap().plans();
        assert!(problems.is_empty(), "{problems:?}");
        let written = toml::to_string(&usage_section(&plans)).unwrap();
        let (again, problems) = toml::from_str::<UsageSection>(&written).unwrap().plans();
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(again, plans);
    }

    #[test]
    fn a_window_resets_when_its_next_one_begins() {
        let at = DateTime::parse_from_rfc3339("2026-11-11T15:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let expected = |stamp: &str| {
            DateTime::parse_from_rfc3339(stamp)
                .unwrap()
                .with_timezone(&Utc)
        };
        assert_eq!(Window::Day.reset(at), expected("2026-11-12T00:00:00Z"));
        assert_eq!(
            Window::Week.reset(at),
            expected("2026-11-16T00:00:00Z"),
            "Monday follows"
        );
        assert_eq!(Window::Month.reset(at), expected("2026-12-01T00:00:00Z"));
        assert_eq!(
            Window::Month.reset(expected("2026-12-31T23:00:00Z")),
            expected("2027-01-01T00:00:00Z"),
            "the year turns"
        );
    }

    #[test]
    fn a_provider_reset_anchors_a_window_that_does_not_start_on_a_boundary() {
        let reset = DateTime::parse_from_rfc3339("2026-11-17T19:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(
            Window::Session.start_before(reset),
            reset - chrono::Duration::hours(5)
        );
        assert_eq!(
            Window::Week.start_before(reset),
            DateTime::parse_from_rfc3339("2026-11-10T19:00:00Z")
                .unwrap()
                .with_timezone(&Utc)
        );
        assert_eq!(
            Window::Month.start_before(reset),
            DateTime::parse_from_rfc3339("2026-10-17T19:00:00Z")
                .unwrap()
                .with_timezone(&Utc)
        );
    }

    #[test]
    fn a_session_is_a_provider_window_not_a_declared_one() {
        assert!(!Window::ALL.contains(&Window::Session));
        assert_eq!(Window::from_key("session"), None);
        assert_eq!(Window::Session.name(), "Session");
    }

    #[test]
    fn a_window_starts_at_its_first_day_or_month() {
        let at = DateTime::parse_from_rfc3339("2026-11-11T15:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(
            Window::Day.start(at),
            DateTime::parse_from_rfc3339("2026-11-11T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc)
        );
        assert_eq!(
            Window::Week.start(at),
            DateTime::parse_from_rfc3339("2026-11-09T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc)
        );
        assert_eq!(
            Window::Month.start(at),
            DateTime::parse_from_rfc3339("2026-11-01T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc)
        );
    }
}
