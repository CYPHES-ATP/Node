use std::{fs, path::PathBuf, time::Duration};

use chrono::{SecondsFormat, TimeZone, Utc};
use reqwest::{
    header::{ACCEPT, AUTHORIZATION, USER_AGENT},
    StatusCode,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubAccessStatus {
    pub authenticated: bool,
    pub paused: bool,
    pub retry_at: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GitHubBackoff {
    retry_after_ms: i64,
    message: String,
}

pub fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|error| error.to_string())
}

pub fn github_access_status() -> GitHubAccessStatus {
    match read_backoff() {
        Some(backoff) if backoff.retry_after_ms > now_ms() => GitHubAccessStatus {
            authenticated: github_token().is_some(),
            paused: true,
            retry_at: Some(format_ms(backoff.retry_after_ms)),
            message: backoff.message,
        },
        _ => GitHubAccessStatus {
            authenticated: github_token().is_some(),
            paused: false,
            retry_at: None,
            message: if github_token().is_some() {
                "GitHub reads are authenticated for higher quota.".to_string()
            } else {
                "GitHub reads are unauthenticated; public API quota is limited.".to_string()
            },
        },
    }
}

pub async fn get_json<T: DeserializeOwned>(
    client: &reqwest::Client,
    url: impl AsRef<str>,
) -> Result<T, String> {
    let url = url.as_ref();
    ensure_not_paused()?;
    let mut request = client
        .get(url)
        .header(USER_AGENT, format!("CYPHES/{}", env!("CARGO_PKG_VERSION")))
        .header(ACCEPT, "application/vnd.github+json");
    if let Some(token) = github_token() {
        request = request.header(AUTHORIZATION, format!("Bearer {token}"));
    }
    let response = request
        .send()
        .await
        .map_err(|error| format!("GitHub request failed: {error}"))?;
    let status = response.status();
    let retry_after_ms = retry_after_ms(&response);
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(handle_failure(status, retry_after_ms, &body));
    }
    response
        .json::<T>()
        .await
        .map_err(|error| format!("GitHub response was not valid JSON: {error}"))
}

pub async fn get_text(client: &reqwest::Client, url: impl AsRef<str>) -> Result<String, String> {
    let url = url.as_ref();
    ensure_not_paused()?;
    let mut request = client
        .get(url)
        .header(USER_AGENT, format!("CYPHES/{}", env!("CARGO_PKG_VERSION")));
    if let Some(token) = github_token() {
        request = request.header(AUTHORIZATION, format!("Bearer {token}"));
    }
    let response = request
        .send()
        .await
        .map_err(|error| format!("GitHub request failed: {error}"))?;
    let status = response.status();
    let retry_after_ms = retry_after_ms(&response);
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(handle_failure(status, retry_after_ms, &body));
    }
    response
        .text()
        .await
        .map_err(|error| format!("GitHub response body read failed: {error}"))
}

fn ensure_not_paused() -> Result<(), String> {
    if let Some(backoff) = read_backoff() {
        if backoff.retry_after_ms > now_ms() {
            return Err(format!(
                "GitHub paused until {}. {}",
                format_ms(backoff.retry_after_ms),
                backoff.message
            ));
        }
    }
    Ok(())
}

fn handle_failure(status: StatusCode, retry_after_ms: Option<i64>, body: &str) -> String {
    let lowered = body.to_ascii_lowercase();
    let rate_limited = status == StatusCode::TOO_MANY_REQUESTS
        || (status == StatusCode::FORBIDDEN && lowered.contains("rate limit"))
        || retry_after_ms.is_some();
    if rate_limited {
        let retry_at = retry_after_ms.unwrap_or_else(|| now_ms() + 15 * 60 * 1000);
        let message = if github_token().is_some() {
            "GitHub rate limit reached for authenticated reads. CYPHES paused repository watching and will retry automatically.".to_string()
        } else {
            "GitHub rate limit reached for unauthenticated reads. CYPHES paused repository watching; add a local GitHub token for higher quota.".to_string()
        };
        write_backoff(&GitHubBackoff {
            retry_after_ms: retry_at,
            message: message.clone(),
        });
        return format!("{message} Retry at {}.", format_ms(retry_at));
    }
    if status == StatusCode::UNAUTHORIZED {
        return "GitHub authorization failed. Check the local token configured for this CYPHES node.".to_string();
    }
    let body_message = github_error_message(body).unwrap_or_else(|| status.to_string());
    format!("GitHub returned {status}: {body_message}")
}

fn retry_after_ms(response: &reqwest::Response) -> Option<i64> {
    let remaining = response
        .headers()
        .get("x-ratelimit-remaining")
        .and_then(|value| value.to_str().ok());
    let reset = response
        .headers()
        .get("x-ratelimit-reset")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<i64>().ok())
        .map(|seconds| seconds * 1000);
    if remaining == Some("0") {
        return reset;
    }
    if response.status() == StatusCode::TOO_MANY_REQUESTS {
        return reset.or_else(|| Some(now_ms() + 15 * 60 * 1000));
    }
    None
}

fn github_error_message(body: &str) -> Option<String> {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("message")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .filter(|message| !message.trim().is_empty())
}

fn github_token() -> Option<String> {
    ["CYPHES_GITHUB_TOKEN", "GITHUB_TOKEN"]
        .into_iter()
        .find_map(|key| std::env::var(key).ok())
        .or_else(|| read_token_file().ok())
        .or_else(|| read_token_from_settings().ok())
        .map(|token| token.trim().to_string())
        .filter(|token| !token.is_empty())
}

fn read_token_file() -> Result<String, String> {
    fs::read_to_string(data_dir()?.join("github.token")).map_err(|error| error.to_string())
}

fn read_token_from_settings() -> Result<String, String> {
    let value = fs::read_to_string(data_dir()?.join("settings.json"))
        .map_err(|error| error.to_string())
        .and_then(|text| serde_json::from_str::<Value>(&text).map_err(|error| error.to_string()))?;
    for key in ["githubToken", "github_token", "gitHubToken"] {
        if let Some(token) = value.get(key).and_then(Value::as_str) {
            return Ok(token.to_string());
        }
    }
    if let Some(token) = value
        .get("github")
        .and_then(|github| github.get("token"))
        .and_then(Value::as_str)
    {
        return Ok(token.to_string());
    }
    Err("settings.json does not contain a GitHub token".to_string())
}

fn data_dir() -> Result<PathBuf, String> {
    if let Ok(data_dir) = std::env::var("CYPHES_DATA_DIR") {
        return Ok(PathBuf::from(data_dir));
    }
    let home = dirs::home_dir().ok_or_else(|| "Could not resolve home directory".to_string())?;
    Ok(home.join(".cyphes"))
}

fn backoff_path() -> Result<PathBuf, String> {
    Ok(data_dir()?.join("github-backoff.json"))
}

fn read_backoff() -> Option<GitHubBackoff> {
    let path = backoff_path().ok()?;
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_backoff(backoff: &GitHubBackoff) {
    if let Ok(path) = backoff_path() {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(text) = serde_json::to_string_pretty(backoff) {
            let _ = fs::write(path, text);
        }
    }
}

fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

fn format_ms(ms: i64) -> String {
    Utc.timestamp_millis_opt(ms)
        .single()
        .unwrap_or_else(Utc::now)
        .to_rfc3339_opts(SecondsFormat::Secs, true)
}
