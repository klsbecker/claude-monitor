//! Reads the plan usage from the Claude API with the OAuth sign-in that Claude Code saves.

use std::path::PathBuf;
use std::sync::RwLock;

use serde_json::Value;
use windows::core::w;

use crate::http;
use crate::i18n::tr;
use crate::time::now_unix;
use crate::util::{home, log};

pub const SESSION: &str = "five_hour";
pub const WEEK: &str = "seven_day";
/// Every limit the app knows, in display order. The model-specific weekly limits only appear on some plans.
pub const LIMIT_KEYS: [&str; 4] = [SESSION, WEEK, "seven_day_opus", "seven_day_sonnet"];
pub const SESSION_SECS: i64 = 5 * 3600;
pub const WEEK_SECS: i64 = 7 * 86400;

/// The public client id of Claude Code, which issued the saved tokens.
const OAUTH_CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";

/// The name of a limit in the current language.
pub fn label(key: &str) -> &str {
    match key {
        SESSION => tr().session_5h,
        WEEK => tr().week,
        "seven_day_opus" => tr().week_opus,
        "seven_day_sonnet" => tr().week_sonnet,
        other => other,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Limit {
    pub key: String,
    /// Utilization, 0 to 100.
    pub pct: f64,
    /// When the window resets, as ISO 8601.
    pub reset: Option<String>,
}

pub fn find_limit(items: &[Limit], key: &str) -> Option<Limit> {
    items.iter().find(|l| l.key == key).cloned()
}

/// Why a fetch failed: `Login` means only a (new) Claude Code sign-in can fix it.
#[derive(Debug)]
pub enum Fail {
    Login(String),
    Other(String),
}

impl Fail {
    pub fn text(self) -> String {
        match self {
            Fail::Login(e) | Fail::Other(e) => e,
        }
    }
}

/// The Claude Code folder chosen in the menu; `None` uses [`default_config_dir`].
static CONFIG_DIR: RwLock<Option<PathBuf>> = RwLock::new(None);

pub fn set_config_dir(dir: Option<PathBuf>) {
    *CONFIG_DIR.write().unwrap() = dir;
}

/// The folder Claude Code itself uses: `$CLAUDE_CONFIG_DIR`, else `~\.claude`.
pub fn default_config_dir() -> PathBuf {
    match std::env::var_os("CLAUDE_CONFIG_DIR") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => home().join(".claude"),
    }
}

/// The Claude Code folder whose sign-in is read.
pub fn config_dir() -> PathBuf {
    CONFIG_DIR.read().unwrap().clone().unwrap_or_else(default_config_dir)
}

/// Whether a folder other than Claude Code's own was chosen.
pub fn custom_config_dir() -> Option<PathBuf> {
    CONFIG_DIR.read().unwrap().clone()
}

/// Other Claude Code folders in the home folder (`~\.claude*`, e.g. one per account) that have a sign-in.
pub fn find_config_dirs() -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(home()) else { return Vec::new() };
    let mut dirs: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().to_lowercase().starts_with(".claude")))
        .filter(|p| p.join(".credentials.json").is_file())
        .collect();
    dirs.sort();
    dirs
}

/// The id of the Claude account signed in to the folder, from Claude Code's settings file (`.claude.json` in
/// the folder, or in the home folder for `~\.claude`). It holds no secret.
pub fn account_id() -> Option<String> {
    let dir = config_dir();
    let mut files = vec![dir.join(".claude.json")];
    if dir.to_string_lossy().trim_end_matches('\\').eq_ignore_ascii_case(&home().join(".claude").to_string_lossy()) {
        files.push(home().join(".claude.json"));
    }
    files.iter().find_map(|f| {
        let v: Value = serde_json::from_str(&std::fs::read_to_string(f).ok()?).ok()?;
        v["oauthAccount"]["accountUuid"].as_str().filter(|id| !id.is_empty()).map(str::to_owned)
    })
}

pub fn credentials_path() -> PathBuf {
    config_dir().join(".credentials.json")
}

pub fn credentials_mtime() -> Option<std::time::SystemTime> {
    std::fs::metadata(credentials_path()).and_then(|m| m.modified()).ok()
}

fn read_credentials() -> Result<Value, Fail> {
    let text = std::fs::read_to_string(credentials_path()).map_err(|_| Fail::Login(tr().not_signed_in.into()))?;
    let v: Value = serde_json::from_str(&text).map_err(|e| Fail::Other(format!("credentials: {e}")))?;
    if v["claudeAiOauth"]["accessToken"].as_str().is_none() {
        return Err(Fail::Login(tr().not_signed_in.into()));
    }
    Ok(v)
}

/// Trades the refresh token for a new access token and writes both back, as Claude Code itself does. Only
/// called once the saved access token no longer works: a Claude Code session in use would already have
/// refreshed it, so this mainly keeps the app working for people who rarely open Claude Code.
fn refresh_credentials(mut creds: Value) -> Result<String, Fail> {
    let refresh = creds["claudeAiOauth"]["refreshToken"].as_str().ok_or(Fail::Login(tr().not_signed_in.into()))?;
    let mut payload = serde_json::json!({
        "grant_type": "refresh_token",
        "refresh_token": refresh,
        "client_id": OAUTH_CLIENT_ID,
    });
    if let Some(scopes) = creds["claudeAiOauth"]["scopes"].as_array() {
        let joined: Vec<&str> = scopes.iter().filter_map(Value::as_str).collect();
        payload["scope"] = Value::from(joined.join(" "));
    }
    let payload = payload.to_string();
    let (status, body) = http::request(
        w!("POST"),
        w!("platform.claude.com"),
        w!("/v1/oauth/token"),
        "Content-Type: application/json\r\n",
        payload.as_bytes(),
    )
    .map_err(Fail::Other)?;
    if status == 400 || status == 401 {
        return Err(Fail::Login(tr().sign_in_expired.into()));
    }
    if status != 200 {
        return Err(Fail::Other(format!("token refresh: HTTP {status}")));
    }
    let v: Value = serde_json::from_slice(&body).map_err(|e| Fail::Other(format!("token refresh: {e}")))?;
    let access = v["access_token"].as_str().ok_or(Fail::Other("token refresh: no access_token".into()))?;
    let oauth = &mut creds["claudeAiOauth"];
    oauth["accessToken"] = Value::from(access);
    if let Some(r) = v["refresh_token"].as_str() {
        oauth["refreshToken"] = Value::from(r);
    }
    if let Some(secs) = v["expires_in"].as_i64() {
        oauth["expiresAt"] = Value::from((now_unix() + secs) * 1000);
    }
    let path = credentials_path();
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, creds.to_string())
        .and_then(|_| std::fs::rename(&tmp, &path))
        .map_err(|e| Fail::Other(format!("saving credentials: {e}")))?;
    log("Claude Code token refreshed");
    Ok(access.to_owned())
}

/// The limits in a usage response, in [`LIMIT_KEYS`] order (unknown ones last).
pub fn parse_limits(v: &Value) -> Vec<Limit> {
    let mut out = Vec::new();
    if let Some(map) = v.as_object() {
        for (key, val) in map {
            if let Some(pct) = val.get("utilization").and_then(Value::as_f64) {
                let reset = val.get("resets_at").and_then(Value::as_str).map(str::to_owned);
                out.push(Limit { key: key.clone(), pct, reset });
            }
        }
    }
    let rank = |k: &str| LIMIT_KEYS.iter().position(|key| *key == k).unwrap_or(LIMIT_KEYS.len());
    out.sort_by_key(|l| rank(&l.key));
    out
}

pub fn fetch_usage() -> Result<Vec<Limit>, Fail> {
    let creds = read_credentials()?;
    let expired = creds["claudeAiOauth"]["expiresAt"].as_i64().is_some_and(|ms| ms / 1000 < now_unix());
    let (mut token, mut refreshed) = if expired {
        (refresh_credentials(creds.clone())?, true)
    } else {
        (creds["claudeAiOauth"]["accessToken"].as_str().unwrap_or_default().to_owned(), false)
    };
    loop {
        let headers = format!("Authorization: Bearer {token}\r\nanthropic-beta: oauth-2025-04-20\r\n");
        let (status, body) = http::get(w!("api.anthropic.com"), w!("/api/oauth/usage"), &headers).map_err(Fail::Other)?;
        match status {
            200 => {
                let v: Value = serde_json::from_slice(&body).map_err(|e| Fail::Other(format!("JSON: {e}")))?;
                return Ok(parse_limits(&v));
            }
            401 if !refreshed => {
                token = refresh_credentials(read_credentials()?)?;
                refreshed = true;
            }
            401 => return Err(Fail::Login(tr().token_rejected.into())),
            _ => {
                let detail: String = String::from_utf8_lossy(&body).chars().take(300).collect();
                return Err(Fail::Other(format!("HTTP {status}: {detail}")));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_orders_limits() {
        let v = serde_json::json!({
            "seven_day": { "utilization": 18.0, "resets_at": "2026-11-28T18:00:00+00:00" },
            "five_hour": { "utilization": 42.5, "resets_at": null },
            "extra_usage": { "is_enabled": false },
        });
        let limits = parse_limits(&v);
        assert_eq!(limits.len(), 2);
        assert_eq!(limits[0], Limit { key: SESSION.into(), pct: 42.5, reset: None });
        assert_eq!(limits[1].key, WEEK);
        assert_eq!(limits[1].reset.as_deref(), Some("2026-11-28T18:00:00+00:00"));
    }
}
