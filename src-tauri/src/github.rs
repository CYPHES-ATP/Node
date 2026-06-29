use std::{fs, path::PathBuf, time::Duration};

use chrono::{SecondsFormat, TimeZone, Utc};
use reqwest::{
    header::{ACCEPT, AUTHORIZATION, USER_AGENT},
    StatusCode,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

const MAX_CACHE_BODY_BYTES: usize = 12 * 1024 * 1024;
const SHORT_LIVED_CACHE_MS: i64 = 5 * 60 * 1000;
const REPOSITORY_METADATA_CACHE_MS: i64 = 60 * 60 * 1000;
const PINNED_SOURCE_CACHE_MS: i64 = 30 * 24 * 60 * 60 * 1000;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SourceCacheMetadata {
    url: String,
    cached_at_ms: i64,
    expires_at_ms: i64,
    body_sha256: String,
}

pub fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|error| error.to_string())
}

pub fn github_access_status() -> GitHubAccessStatus {
    let gateway = source_gateway_base_urls().into_iter().next();
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
                match gateway {
                    Some(gateway) => format!(
                        "Source Gateway enabled at {gateway}; local GitHub token is available as fallback."
                    ),
                    None => "GitHub reads are authenticated for higher quota.".to_string(),
                }
            } else {
                match gateway {
                    Some(gateway) => format!(
                        "Source Gateway enabled at {gateway}; direct GitHub fallback is unauthenticated."
                    ),
                    None => "GitHub reads are unauthenticated; public API quota is limited.".to_string(),
                }
            },
        },
    }
}

pub async fn get_json<T: DeserializeOwned>(
    client: &reqwest::Client,
    url: impl AsRef<str>,
) -> Result<T, String> {
    let url = url.as_ref();
    if let Some(body) = read_source_cache(url) {
        return serde_json::from_str(&body)
            .map_err(|error| format!("cached GitHub response was not valid JSON: {error}"));
    }
    if let Some(body) = get_via_source_gateway(client, url).await {
        write_source_cache(url, &body);
        return serde_json::from_str::<T>(&body)
            .map_err(|error| format!("Source Gateway response was not valid JSON: {error}"));
    }
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
    let body = response
        .text()
        .await
        .map_err(|error| format!("GitHub response body read failed: {error}"))?;
    write_source_cache(url, &body);
    serde_json::from_str::<T>(&body)
        .map_err(|error| format!("GitHub response was not valid JSON: {error}"))
}

pub async fn get_text(client: &reqwest::Client, url: impl AsRef<str>) -> Result<String, String> {
    let url = url.as_ref();
    if let Some(body) = read_source_cache(url) {
        return Ok(body);
    }
    if let Some(body) = get_via_source_gateway(client, url).await {
        write_source_cache(url, &body);
        return Ok(body);
    }
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
    let body = response
        .text()
        .await
        .map_err(|error| format!("GitHub response body read failed: {error}"))?;
    write_source_cache(url, &body);
    Ok(body)
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

async fn get_via_source_gateway(client: &reqwest::Client, url: &str) -> Option<String> {
    for gateway_url in source_gateway_urls_for_github_url(url) {
        let Ok(response) = client
            .get(gateway_url)
            .header(USER_AGENT, format!("CYPHES/{}", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(6))
            .send()
            .await
        else {
            continue;
        };
        if response.status().is_success() {
            if let Ok(body) = response.text().await {
                return Some(body);
            }
        }
    }
    None
}

#[cfg(test)]
fn source_gateway_url_for_github_url(url: &str) -> Option<String> {
    source_gateway_urls_for_github_url(url).into_iter().next()
}

fn source_gateway_urls_for_github_url(url: &str) -> Vec<String> {
    source_gateway_base_urls()
        .into_iter()
        .filter_map(|base| source_gateway_url_for_github_url_with_base(&base, url))
        .collect()
}

fn source_gateway_url_for_github_url_with_base(base: &str, url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url).ok()?;
    let host = parsed.host_str()?;
    let segments = parsed
        .path_segments()
        .map(|segments| segments.collect::<Vec<_>>())
        .unwrap_or_default();
    if host == "api.github.com" && segments.len() >= 3 && segments[0] == "repos" {
        let repo = format!(
            "{}/{}",
            decode_segment(segments[1]),
            decode_segment(segments[2])
        );
        if segments.len() == 3 {
            return gateway_url(&base, "/v1/github/repository", &[("repo", repo)]);
        }
        if segments.get(3) == Some(&"commits") && segments.len() >= 5 {
            let reference = segments[4..]
                .iter()
                .map(|segment| decode_segment(segment))
                .collect::<Vec<_>>()
                .join("/");
            return gateway_url(
                &base,
                "/v1/github/resolve",
                &[("repo", repo), ("ref", reference)],
            );
        }
        if segments.len() >= 6
            && segments.get(3) == Some(&"git")
            && segments.get(4) == Some(&"trees")
            && segments.get(5).is_some_and(|value| is_sha(value))
        {
            return gateway_url(
                &base,
                "/v1/github/tree",
                &[
                    ("repo", repo),
                    ("commit", decode_segment(segments[5]).to_ascii_lowercase()),
                ],
            );
        }
    }
    if host == "raw.githubusercontent.com" && segments.len() >= 4 {
        let commit = decode_segment(segments[2]).to_ascii_lowercase();
        if !is_sha(&commit) {
            return None;
        }
        let repo = format!(
            "{}/{}",
            decode_segment(segments[0]),
            decode_segment(segments[1])
        );
        let path = segments[3..]
            .iter()
            .map(|segment| decode_segment(segment))
            .collect::<Vec<_>>()
            .join("/");
        return gateway_url(
            &base,
            "/v1/github/file",
            &[("repo", repo), ("commit", commit), ("path", path)],
        );
    }
    None
}

fn source_gateway_base_urls() -> Vec<String> {
    if std::env::var("CYPHES_DISABLE_SOURCE_GATEWAY")
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
    {
        return Vec::new();
    }
    if let Ok(value) = std::env::var("CYPHES_SOURCE_GATEWAY_URL") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return vec![trimmed.trim_end_matches('/').to_string()];
        }
    }
    vec![
        "https://source.cyphes.com".to_string(),
        "https://cyphes-source-gateway.fly.dev".to_string(),
    ]
}

fn gateway_url(base: &str, path: &str, pairs: &[(&str, String)]) -> Option<String> {
    let mut url = reqwest::Url::parse(base).ok()?;
    url.set_path(path);
    url.set_query(None);
    {
        let mut query = url.query_pairs_mut();
        for (key, value) in pairs {
            query.append_pair(key, value);
        }
    }
    Some(url.to_string())
}

fn decode_segment(value: &str) -> String {
    let mut output = String::new();
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = &value[index + 1..index + 3];
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                output.push(byte as char);
                index += 3;
                continue;
            }
        }
        output.push(bytes[index] as char);
        index += 1;
    }
    output
}

fn read_source_cache(url: &str) -> Option<String> {
    if source_cache_disabled() {
        return None;
    }
    let policy = source_cache_policy_ms(url)?;
    let (metadata_path, body_path) = source_cache_paths(url).ok()?;
    let metadata = fs::read_to_string(metadata_path).ok()?;
    let metadata = serde_json::from_str::<SourceCacheMetadata>(&metadata).ok()?;
    if metadata.url != url || metadata.expires_at_ms <= now_ms() {
        return None;
    }
    if metadata.cached_at_ms + policy < now_ms() {
        return None;
    }
    let body = fs::read_to_string(body_path).ok()?;
    (metadata.body_sha256 == sha256_hex(body.as_bytes())).then_some(body)
}

fn write_source_cache(url: &str, body: &str) {
    if source_cache_disabled()
        || body.len() > MAX_CACHE_BODY_BYTES
        || source_cache_policy_ms(url).is_none()
    {
        return;
    }
    let Some(policy_ms) = source_cache_policy_ms(url) else {
        return;
    };
    let Ok((metadata_path, body_path)) = source_cache_paths(url) else {
        return;
    };
    let now = now_ms();
    let metadata = SourceCacheMetadata {
        url: url.to_string(),
        cached_at_ms: now,
        expires_at_ms: now + policy_ms,
        body_sha256: sha256_hex(body.as_bytes()),
    };
    if let Some(parent) = metadata_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(metadata) = serde_json::to_string_pretty(&metadata) {
        let _ = fs::write(&body_path, body);
        let _ = fs::write(metadata_path, metadata);
    }
}

fn source_cache_policy_ms(url: &str) -> Option<i64> {
    let parsed = reqwest::Url::parse(url).ok()?;
    let host = parsed.host_str()?;
    let segments = parsed
        .path_segments()
        .map(|segments| segments.collect::<Vec<_>>())
        .unwrap_or_default();
    if host == "raw.githubusercontent.com" && segments.get(2).is_some_and(|value| is_sha(value)) {
        return Some(PINNED_SOURCE_CACHE_MS);
    }
    if host == "api.github.com" && segments.len() >= 3 && segments[0] == "repos" {
        if segments.len() >= 6
            && segments.get(3) == Some(&"git")
            && segments.get(4) == Some(&"trees")
            && segments.get(5).is_some_and(|value| is_sha(value))
        {
            return Some(PINNED_SOURCE_CACHE_MS);
        }
        if segments.get(3) == Some(&"commits") {
            return if segments.get(4).is_some_and(|value| is_sha(value)) {
                Some(PINNED_SOURCE_CACHE_MS)
            } else {
                Some(SHORT_LIVED_CACHE_MS)
            };
        }
        if segments.len() == 3 {
            return Some(REPOSITORY_METADATA_CACHE_MS);
        }
    }
    None
}

fn source_cache_paths(url: &str) -> Result<(PathBuf, PathBuf), String> {
    let digest = sha256_hex(url.as_bytes());
    let root = data_dir()?.join("source-cache").join("github");
    Ok((
        root.join(format!("{digest}.json")),
        root.join(format!("{digest}.body")),
    ))
}

fn source_cache_disabled() -> bool {
    std::env::var("CYPHES_DISABLE_SOURCE_CACHE")
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

fn is_sha(value: &str) -> bool {
    value.len() == 40 && value.chars().all(|character| character.is_ascii_hexdigit())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
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

#[cfg(test)]
mod tests {
    use super::{
        source_cache_policy_ms, source_gateway_url_for_github_url, PINNED_SOURCE_CACHE_MS,
        REPOSITORY_METADATA_CACHE_MS, SHORT_LIVED_CACHE_MS,
    };

    #[test]
    fn cache_policy_prefers_pinned_source_context() {
        assert_eq!(
            source_cache_policy_ms("https://raw.githubusercontent.com/owner/repo/0123456789abcdef0123456789abcdef01234567/contracts/A.sol"),
            Some(PINNED_SOURCE_CACHE_MS)
        );
        assert_eq!(
            source_cache_policy_ms("https://api.github.com/repos/owner/repo/git/trees/0123456789abcdef0123456789abcdef01234567?recursive=1"),
            Some(PINNED_SOURCE_CACHE_MS)
        );
    }

    #[test]
    fn cache_policy_keeps_moving_refs_short_lived() {
        assert_eq!(
            source_cache_policy_ms("https://api.github.com/repos/owner/repo/commits/main"),
            Some(SHORT_LIVED_CACHE_MS)
        );
        assert_eq!(
            source_cache_policy_ms("https://api.github.com/repos/owner/repo"),
            Some(REPOSITORY_METADATA_CACHE_MS)
        );
    }

    #[test]
    fn maps_github_urls_to_source_gateway_urls() {
        assert_eq!(
            source_gateway_url_for_github_url("https://api.github.com/repos/aave/aave-v3-origin")
                .unwrap(),
            "https://source.cyphes.com/v1/github/repository?repo=aave%2Faave-v3-origin"
        );
        assert_eq!(
            source_gateway_url_for_github_url(
                "https://api.github.com/repos/aave/aave-v3-origin/git/trees/0123456789abcdef0123456789abcdef01234567?recursive=1"
            )
            .unwrap(),
            "https://source.cyphes.com/v1/github/tree?repo=aave%2Faave-v3-origin&commit=0123456789abcdef0123456789abcdef01234567"
        );
        assert_eq!(
            source_gateway_url_for_github_url(
                "https://raw.githubusercontent.com/aave/aave-v3-origin/0123456789abcdef0123456789abcdef01234567/contracts/protocol/pool/Pool.sol"
            )
            .unwrap(),
            "https://source.cyphes.com/v1/github/file?repo=aave%2Faave-v3-origin&commit=0123456789abcdef0123456789abcdef01234567&path=contracts%2Fprotocol%2Fpool%2FPool.sol"
        );
    }
}
