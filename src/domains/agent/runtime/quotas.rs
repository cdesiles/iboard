//! What a subscription reports it has used, when the provider itself says.
//!
//! Copilot's account endpoint counts premium interactions, not tokens, so its
//! share cannot come from the usage log. The token is the GitHub OAuth one
//! opencode already stores; the endpoint is internal and may refuse or change
//! at any time, in which case the caller falls back on the log.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use serde_json::Value;

use crate::platform::files;
use crate::platform::job::Cancel;
use crate::platform::process;

/// The unit GitHub counts a Copilot plan in.
pub const UNIT: &str = "AI credits";

/// GitHub's account endpoint, as the discovery found it.
const ACCOUNT: &str = "https://api.github.com";

/// The integration opencode stores the GitHub OAuth token under.
const INTEGRATION: &str = "github-copilot";

/// How long the account endpoint may take to answer.
const CALL: Duration = Duration::from_secs(15);

/// A provider's own count of its window: what it used out of what it includes,
/// and when it resets.
#[derive(Debug, Clone, PartialEq)]
pub struct Quota {
    /// The normalized provider ID the meters and `[usage]` share.
    pub provider: String,
    pub used: u64,
    pub cap: u64,
    /// What the cap counts, as the provider writes it.
    pub unit: &'static str,
    pub reset: DateTime<Utc>,
}

impl Quota {
    /// The share of the cap used, from 0 up; `None` when the cap is zero.
    pub fn share(&self) -> Option<f32> {
        (self.cap > 0).then(|| self.used as f32 / self.cap as f32)
    }
}

/// Copilot's quota, read from GitHub with opencode's GitHub token. `None` when
/// the token, the network or the endpoint fails.
pub fn copilot(cancel: &Cancel) -> Option<Quota> {
    let token = stored_token().ok()?;
    fetch(ACCOUNT, &token, cancel).ok()
}

/// The GitHub OAuth token opencode stores, when it is there and still valid.
fn stored_token() -> Result<String, String> {
    let path = auth_path().ok_or_else(|| "Couldn't find opencode's data folder".to_string())?;
    stored_token_at(&path)
}

fn auth_path() -> Option<std::path::PathBuf> {
    files::dir_from_env("XDG_DATA_HOME", ".local/share")
        .map(|dir| dir.join("opencode").join("auth.json"))
}

fn stored_token_at(path: &Path) -> Result<String, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|err| format!("Couldn't read {}: {err}", path.display()))?;
    token_from(&text)
}

/// The GitHub token in opencode's credential store, refusing one that expired.
fn token_from(text: &str) -> Result<String, String> {
    let store: Value =
        serde_json::from_str(text).map_err(|err| format!("opencode's store is not JSON: {err}"))?;
    let credential = store
        .get(INTEGRATION)
        .map(|credential| credential.get("value").unwrap_or(credential))
        .ok_or_else(|| "opencode stores no GitHub Copilot credential".to_string())?;
    let token = credential["access"]
        .as_str()
        .filter(|token| !token.is_empty())
        .ok_or_else(|| "opencode's GitHub Copilot credential has no token".to_string())?;
    let expires = credential["expires"].as_i64().unwrap_or(0);
    if expires > 0 && expires <= Utc::now().timestamp_millis() {
        return Err("opencode's GitHub Copilot token expired".to_string());
    }
    Ok(token.to_string())
}

/// `base`'s Copilot account endpoint, asked with `token`.
fn fetch(base: &str, token: &str, cancel: &Cancel) -> Result<Quota, String> {
    let url = format!("{}/copilot_internal/user", base.trim_end_matches('/'));
    let script = "exec curl -sS --fail --max-time 10 \
        --header \"Authorization: Bearer $IBOARD_GITHUB_TOKEN\" \
        --header 'Accept: application/json' \
        --header 'X-GitHub-Api-Version: 2022-11-28' \
        \"$1\"";
    let mut command = Command::new("sh");
    command
        .args(["-c", script, "iboard-copilot", &url])
        .env("IBOARD_GITHUB_TOKEN", token);
    let output = process::run(command, "the GitHub Copilot account", cancel, CALL)?;
    parse(&output.into_result("the GitHub Copilot account")?)
}

/// GitHub's reply: `quota_snapshots.premium_interactions` and the reset date.
fn parse(reply: &str) -> Result<Quota, String> {
    let value: Value =
        serde_json::from_str(reply).map_err(|err| format!("GitHub sent no JSON: {err}"))?;
    let snapshot = &value["quota_snapshots"]["premium_interactions"];
    if snapshot.is_null() {
        return Err("GitHub reported no Copilot premium-interaction quota".to_string());
    }
    if snapshot["unlimited"].as_bool() == Some(true) {
        return Err("GitHub reports unlimited Copilot premium interactions".to_string());
    }
    let cap = snapshot["entitlement"]
        .as_f64()
        .filter(|cap| *cap > 0.0)
        .ok_or_else(|| "GitHub reported no Copilot entitlement".to_string())?;
    let used = used_of(snapshot, cap)
        .ok_or_else(|| "GitHub reported no Copilot credits used".to_string())?;
    let reset =
        reset_of(&value).ok_or_else(|| "GitHub reported no Copilot reset date".to_string())?;
    Ok(Quota {
        provider: "copilot".to_string(),
        used,
        cap: cap.round() as u64,
        unit: UNIT,
        reset,
    })
}

/// What the snapshot says was used, or what the entitlement leaves.
fn used_of(snapshot: &Value, cap: f64) -> Option<u64> {
    if let Some(used) = snapshot["credits_used"].as_f64() {
        return Some(used.max(0.0).round() as u64);
    }
    let remaining = snapshot["remaining"].as_f64()?;
    Some((cap - remaining).max(0.0).round() as u64)
}

/// The day the quota resets, from a date or a timestamp.
fn reset_of(value: &Value) -> Option<DateTime<Utc>> {
    let reset = value["quota_reset_date"].as_str()?;
    if let Ok(date) = NaiveDate::parse_from_str(reset, "%Y-%m-%d") {
        return Some(DateTime::from_naive_utc_and_offset(
            date.and_time(NaiveTime::MIN),
            Utc,
        ));
    }
    DateTime::parse_from_rfc3339(reset)
        .ok()
        .map(|at| at.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    use super::*;

    /// The account endpoint's reply, as the discovery sampled it.
    const SAMPLE: &str = r#"{
        "copilot_plan": "business",
        "access_type_sku": "copilot_for_business_seat_quota",
        "quota_reset_date": "2026-12-01",
        "quota_snapshots": {
            "premium_interactions": {
                "percent_remaining": 69.3,
                "entitlement": 30000,
                "remaining": 20807.1,
                "credits_used": 9193,
                "unlimited": false,
                "overage_permitted": true
            },
            "chat": {"unlimited": true},
            "completions": {"unlimited": true}
        }
    }"#;

    #[test]
    fn the_account_reply_reads_the_premium_interaction_quota() {
        let quota = parse(SAMPLE).expect("quota");
        assert_eq!(quota.provider, "copilot");
        assert_eq!(quota.used, 9_193);
        assert_eq!(quota.cap, 30_000);
        assert_eq!(quota.unit, "AI credits");
        assert_eq!(
            quota.reset,
            DateTime::parse_from_rfc3339("2026-12-01T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc)
        );
        let share = quota.share().unwrap();
        assert!((share - 0.306_43).abs() < 0.000_1, "{share}");
    }

    #[test]
    fn a_reply_without_a_counted_used_or_reset_is_an_error() {
        let missing_used = r#"{"quota_reset_date":"2026-12-01",
            "quota_snapshots":{"premium_interactions":{"entitlement":30000}}}"#;
        assert!(parse(missing_used).is_err());
        let missing_reset = r#"{"quota_snapshots":{"premium_interactions":
            {"entitlement":30000,"credits_used":9193}}}"#;
        assert!(parse(missing_reset).is_err());
        assert!(parse("").is_err());
    }

    #[test]
    fn a_remaining_count_stands_in_for_the_used_one() {
        let reply = r#"{"quota_reset_date":"2026-12-01T00:00:00Z",
            "quota_snapshots":{"premium_interactions":
            {"entitlement":30000,"remaining":20807.1}}}"#;
        assert_eq!(parse(reply).unwrap().used, 9_193);
    }

    #[test]
    fn an_unlimited_or_absent_snapshot_is_not_a_quota() {
        let unlimited = r#"{"quota_reset_date":"2026-12-01",
            "quota_snapshots":{"premium_interactions":
            {"entitlement":30000,"credits_used":9193,"unlimited":true}}}"#;
        assert!(parse(unlimited).is_err());
        assert!(parse(r#"{"quota_snapshots":{"chat":{"unlimited":true}}}"#).is_err());
    }

    #[test]
    fn the_token_comes_from_opencodes_store() {
        let store = r#"{"github-copilot":{"type":"oauth","access":"secret","expires":0}}"#;
        assert_eq!(token_from(store).unwrap(), "secret");
        let nested =
            r#"{"github-copilot":{"value":{"type":"oauth","access":"secret","expires":0}}}"#;
        assert_eq!(token_from(nested).unwrap(), "secret");
        assert!(token_from(r#"{"deepseek":{"type":"api","key":"secret"}}"#).is_err());
        assert!(token_from("not json").is_err());
    }

    #[test]
    fn an_expired_token_is_not_used() {
        let expired = r#"{"github-copilot":{"type":"oauth","access":"secret","expires":1}}"#;
        assert!(token_from(expired).is_err());
    }

    #[test]
    fn a_token_is_read_from_a_store_at_a_known_path() {
        let scratch = tempfile::tempdir().unwrap();
        let path = scratch.path().join("auth.json");
        std::fs::write(
            &path,
            r#"{"github-copilot":{"type":"oauth","access":"secret","expires":0}}"#,
        )
        .unwrap();
        assert_eq!(stored_token_at(&path).unwrap(), "secret");
        assert!(stored_token_at(&scratch.path().join("missing.json")).is_err());
    }

    /// A one-request server that answers `status` with `body`.
    fn serve(status: &str, body: &str) -> (String, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let body = body.to_string();
        let status = status.to_string();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut chunk = [0; 1024];
            while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                let read = stream.read(&mut chunk).unwrap();
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&chunk[..read]);
            }
            let reply = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n\
                 Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(reply.as_bytes()).unwrap();
            String::from_utf8_lossy(&request).into_owned()
        });
        (format!("http://{address}"), server)
    }

    fn curl_available() -> bool {
        Command::new("curl")
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success())
    }

    #[test]
    fn the_endpoint_answers_with_the_token_and_the_path_it_takes() {
        if !curl_available() {
            return;
        }
        let (base, server) = serve("200 OK", SAMPLE);
        let quota = fetch(&base, "secret", &Cancel::default()).expect("quota");
        let request = server.join().unwrap();
        assert!(
            request.starts_with("GET /copilot_internal/user "),
            "{request}"
        );
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer secret"),
            "{request}"
        );
        assert_eq!(quota.used, 9_193);
    }

    #[test]
    fn a_refused_or_unreadable_reply_is_an_error() {
        if !curl_available() {
            return;
        }
        let (base, server) = serve("401 Unauthorized", r#"{"message":"Bad credentials"}"#);
        assert!(fetch(&base, "secret", &Cancel::default()).is_err());
        let _ = server.join();

        let (base, server) = serve("200 OK", "not json");
        assert!(fetch(&base, "secret", &Cancel::default()).is_err());
        let _ = server.join();
    }
}
