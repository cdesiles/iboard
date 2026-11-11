//! What a run spent, as its engine reports it: money, tokens, provider and
//! model, and whether the money was metered or came out of a subscription.
//!
//! OpenCode reports it on every session; Claude Code on the `result` event of
//! its stream.

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

/// Whether a run's dollars were taken, or are a subscription's API-equivalent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Billing {
    #[default]
    Metered,
    Subscription,
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
    /// The cost as the build log writes it: `$0.42`.
    pub fn cost_label(&self) -> Option<String> {
        self.cost.map(|cost| format!("${cost:.2}"))
    }

    /// A session from OpenCode's session list.
    pub(crate) fn from_session(session: &Value) -> Option<Self> {
        let cost = session["cost"].as_f64();
        let tokens = &session["tokens"];
        let provider = text(&session["model"]["providerID"]);
        let model = text(&session["model"]["id"]);
        (cost.is_some() || !tokens.is_null() || provider.is_some() || model.is_some()).then(|| {
            Usage {
                cost,
                tokens: tokens_of(tokens),
                billing: billing_of(provider.as_deref()),
                provider,
                model,
            }
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

fn text(value: &Value) -> Option<String> {
    value
        .as_str()
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn tokens_of(value: &Value) -> Tokens {
    Tokens {
        input: value["input"].as_u64().unwrap_or(0),
        output: value["output"].as_u64().unwrap_or(0),
        reasoning: value["reasoning"].as_u64().unwrap_or(0),
        cache_read: value["cache"]["read"].as_u64().unwrap_or(0),
        cache_write: value["cache"]["write"].as_u64().unwrap_or(0),
    }
}

fn billing_of(provider: Option<&str>) -> Billing {
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

    /// A session from `GET /api/session`, trimmed to what usage reads.
    const OPENCODE_SESSION: &str = r#"{
        "id": "ses_deepseek",
        "model": {"id": "deepseek-flash", "providerID": "deepseek", "variant": "default"},
        "cost": 0.42,
        "tokens": {"input": 1234, "output": 56, "reasoning": 7,
                   "cache": {"read": 890, "write": 12}},
        "location": {"directory": "/code/app"}
    }"#;

    /// A Copilot session: a subscription, which reports no money.
    const COPILOT_SESSION: &str = r#"{
        "id": "ses_copilot",
        "model": {"id": "claude-opus-4.6", "providerID": "github-copilot"},
        "cost": 0,
        "tokens": {"input": 159, "output": 14001, "reasoning": 0,
                   "cache": {"read": 1902598, "write": 383223}},
        "location": {"directory": "/code/app"}
    }"#;

    /// Claude Code's init and result events, trimmed to what usage reads.
    const CLAUDE_STREAM: &str = concat!(
        r#"{"type":"system","subtype":"init","session_id":"abc","model":"claude-sonnet-4-5"}"#,
        "\n",
        r#"{"type":"result","subtype":"success","is_error":false,"result":"Done.","session_id":"abc","total_cost_usd":0.42,"usage":{"input_tokens":1234,"output_tokens":56,"cache_read_input_tokens":890,"cache_creation_input_tokens":12}}"#,
    );

    #[test]
    fn an_opencode_session_reads_its_cost_and_tokens() {
        let session: Value = serde_json::from_str(OPENCODE_SESSION).unwrap();
        let usage = Usage::from_session(&session).expect("usage");
        assert_eq!(usage.cost, Some(0.42));
        assert_eq!(
            usage.tokens,
            Tokens {
                input: 1234,
                output: 56,
                reasoning: 7,
                cache_read: 890,
                cache_write: 12,
            }
        );
        assert_eq!(usage.provider.as_deref(), Some("deepseek"));
        assert_eq!(usage.model.as_deref(), Some("deepseek-flash"));
        assert_eq!(usage.billing, Billing::Metered);
        assert_eq!(usage.cost_label().as_deref(), Some("$0.42"));
    }

    #[test]
    fn a_copilot_session_at_cost_zero_is_a_subscription() {
        let session: Value = serde_json::from_str(COPILOT_SESSION).unwrap();
        let usage = Usage::from_session(&session).expect("usage");
        assert_eq!(usage.cost, Some(0.0));
        assert_eq!(usage.billing, Billing::Subscription);
        assert_eq!(usage.provider.as_deref(), Some("github-copilot"));
        assert_eq!(usage.tokens.input, 159);
        assert_eq!(usage.tokens.cache_read, 1_902_598);
        assert_eq!(usage.cost_label().as_deref(), Some("$0.00"));
    }

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
}
