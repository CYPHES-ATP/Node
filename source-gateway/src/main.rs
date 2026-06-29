use std::{
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, SecondsFormat, Utc};
use ed25519_dalek::{Signer, SigningKey};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use rand_core::OsRng;
use reqwest::header::{
    ACCEPT, AUTHORIZATION, ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED, USER_AGENT,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use tower_http::trace::TraceLayer;

const CACHE_VERSION: &str = "cyphes.source-cache/0.1";
const MANIFEST_PROFILE: &str = "cyphes.source-manifest";
const MANIFEST_VERSION: &str = "0.1";
const USER_AGENT_VALUE: &str = "CYPHES-Source-Gateway/0.6.1";
const MAX_CACHE_BODY_BYTES: usize = 16 * 1024 * 1024;
const MOVING_REF_TTL_SECONDS: i64 = 300;
const REPOSITORY_TTL_SECONDS: i64 = 3600;
const PINNED_SOURCE_TTL_SECONDS: i64 = 30 * 24 * 60 * 60;

#[derive(Clone)]
struct AppState {
    client: reqwest::Client,
    cache_dir: PathBuf,
    signer: Arc<SigningKey>,
    gateway_id: String,
    token_provider: Arc<Mutex<TokenProvider>>,
}

#[derive(Debug)]
enum GatewayError {
    BadRequest(String),
    Upstream(String),
    Internal(String),
}

impl IntoResponse for GatewayError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            GatewayError::BadRequest(message) => (StatusCode::BAD_REQUEST, message),
            GatewayError::Upstream(message) => (StatusCode::BAD_GATEWAY, message),
            GatewayError::Internal(message) => (StatusCode::INTERNAL_SERVER_ERROR, message),
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}

#[derive(Debug, Deserialize)]
struct RepoQuery {
    repo: String,
}

#[derive(Debug, Deserialize)]
struct ResolveQuery {
    repo: String,
    #[serde(default = "default_ref")]
    r#ref: String,
}

#[derive(Debug, Deserialize)]
struct TreeQuery {
    repo: String,
    commit: String,
}

#[derive(Debug, Deserialize)]
struct FileQuery {
    repo: String,
    commit: String,
    path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CacheMetadata {
    profile: String,
    cache_key: String,
    upstream_url: String,
    cached_at: String,
    expires_at: String,
    etag: Option<String>,
    last_modified: Option<String>,
    content_type: Option<String>,
    body_sha256: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceManifest {
    profile: String,
    version: String,
    gateway_id: String,
    provider: String,
    repo: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reference: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    commit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    upstream_url: String,
    cache_key: String,
    body_sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    etag: Option<String>,
    fetched_at: String,
    served_at: String,
    signature: SourceManifestSignature,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceManifestSignature {
    algorithm: String,
    public_key_base64_url: String,
    signature_base64_url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UnsignedSourceManifest<'a> {
    profile: &'a str,
    version: &'a str,
    gateway_id: &'a str,
    provider: &'a str,
    repo: &'a str,
    reference: Option<&'a str>,
    commit: Option<&'a str>,
    path: Option<&'a str>,
    upstream_url: &'a str,
    cache_key: &'a str,
    body_sha256: &'a str,
    etag: Option<&'a str>,
    fetched_at: &'a str,
    served_at: &'a str,
}

#[derive(Debug, Deserialize, Serialize)]
struct GitHubAppClaims {
    iat: i64,
    exp: i64,
    iss: String,
}

#[derive(Debug, Deserialize)]
struct InstallationTokenResponse {
    token: String,
    expires_at: DateTime<Utc>,
}

#[derive(Debug)]
enum TokenProvider {
    Static(Option<String>),
    GitHubApp {
        app_id: String,
        installation_id: String,
        private_key_pem: String,
        cached: Option<CachedToken>,
    },
}

#[derive(Debug, Clone)]
struct CachedToken {
    token: String,
    expires_at: DateTime<Utc>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let port = std::env::var("CYPHES_SOURCE_GATEWAY_PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse::<u16>()?;
    let bind = std::env::var("CYPHES_SOURCE_GATEWAY_BIND")
        .unwrap_or_else(|_| format!("0.0.0.0:{port}"))
        .parse::<SocketAddr>()?;
    let cache_dir = cache_dir();
    fs::create_dir_all(&cache_dir)?;

    let signer = Arc::new(load_or_create_signing_key(&cache_dir)?);
    let gateway_id = std::env::var("CYPHES_SOURCE_GATEWAY_ID")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("cyphes-source:{}", public_key_base64(&signer)));
    let token_provider = TokenProvider::from_env().map_err(|error| format!("{error:?}"))?;
    let state = AppState {
        client: reqwest::Client::builder()
            .timeout(Duration::from_secs(45))
            .build()?,
        cache_dir,
        signer,
        gateway_id,
        token_provider: Arc::new(Mutex::new(token_provider)),
    };

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/github/repository", get(github_repository))
        .route("/v1/github/resolve", get(github_resolve))
        .route("/v1/github/tree", get(github_tree))
        .route("/v1/github/file", get(github_file))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    println!("CYPHES Source Gateway listening on {bind}");
    let listener = tokio::net::TcpListener::bind(bind).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

async fn healthz(State(state): State<AppState>) -> impl IntoResponse {
    Json(json!({
        "ok": true,
        "service": "cyphes-source-gateway",
        "version": env!("CARGO_PKG_VERSION"),
        "gatewayId": state.gateway_id,
        "cacheVersion": CACHE_VERSION
    }))
}

async fn github_repository(
    State(state): State<AppState>,
    Query(query): Query<RepoQuery>,
) -> Result<impl IntoResponse, GatewayError> {
    let repo = normalize_repo(&query.repo)?;
    let upstream_url = format!("https://api.github.com/repos/{repo}");
    let fetch = fetch_cached(
        &state,
        SourceRequest {
            repo,
            reference: None,
            commit: None,
            path: None,
            upstream_url,
            cache_ttl_seconds: REPOSITORY_TTL_SECONDS,
            accept: "application/vnd.github+json",
        },
    )
    .await?;
    Ok(json_response_with_manifest(fetch))
}

async fn github_resolve(
    State(state): State<AppState>,
    Query(query): Query<ResolveQuery>,
) -> Result<impl IntoResponse, GatewayError> {
    let repo = normalize_repo(&query.repo)?;
    let reference = normalize_ref(&query.r#ref)?;
    let upstream_url = format!(
        "https://api.github.com/repos/{repo}/commits/{}",
        encode_path_segment(&reference)
    );
    let ttl = if is_git_sha(&reference) {
        PINNED_SOURCE_TTL_SECONDS
    } else {
        MOVING_REF_TTL_SECONDS
    };
    let fetch = fetch_cached(
        &state,
        SourceRequest {
            repo,
            reference: Some(reference),
            commit: None,
            path: None,
            upstream_url,
            cache_ttl_seconds: ttl,
            accept: "application/vnd.github+json",
        },
    )
    .await?;
    Ok(json_response_with_manifest(fetch))
}

async fn github_tree(
    State(state): State<AppState>,
    Query(query): Query<TreeQuery>,
) -> Result<impl IntoResponse, GatewayError> {
    let repo = normalize_repo(&query.repo)?;
    let commit = normalize_commit(&query.commit)?;
    let upstream_url =
        format!("https://api.github.com/repos/{repo}/git/trees/{commit}?recursive=1");
    let fetch = fetch_cached(
        &state,
        SourceRequest {
            repo,
            reference: None,
            commit: Some(commit),
            path: None,
            upstream_url,
            cache_ttl_seconds: PINNED_SOURCE_TTL_SECONDS,
            accept: "application/vnd.github+json",
        },
    )
    .await?;
    Ok(json_response_with_manifest(fetch))
}

async fn github_file(
    State(state): State<AppState>,
    Query(query): Query<FileQuery>,
) -> Result<impl IntoResponse, GatewayError> {
    let repo = normalize_repo(&query.repo)?;
    let commit = normalize_commit(&query.commit)?;
    let path = normalize_file_path(&query.path)?;
    let upstream_url = format!(
        "https://raw.githubusercontent.com/{repo}/{commit}/{}",
        encode_raw_path(&path)
    );
    let fetch = fetch_cached(
        &state,
        SourceRequest {
            repo,
            reference: None,
            commit: Some(commit),
            path: Some(path),
            upstream_url,
            cache_ttl_seconds: PINNED_SOURCE_TTL_SECONDS,
            accept: "text/plain,application/octet-stream;q=0.9,*/*;q=0.8",
        },
    )
    .await?;
    Ok(text_response_with_manifest(fetch))
}

struct SourceRequest {
    repo: String,
    reference: Option<String>,
    commit: Option<String>,
    path: Option<String>,
    upstream_url: String,
    cache_ttl_seconds: i64,
    accept: &'static str,
}

struct FetchedSource {
    body: Vec<u8>,
    content_type: Option<String>,
    manifest: SourceManifest,
}

async fn fetch_cached(
    state: &AppState,
    request: SourceRequest,
) -> Result<FetchedSource, GatewayError> {
    let cache_key = cache_key(&request.upstream_url);
    let metadata_path = state.cache_dir.join(format!("{cache_key}.json"));
    let body_path = state.cache_dir.join(format!("{cache_key}.body"));
    let cached = read_cached(&metadata_path, &body_path);
    if let Some((metadata, body)) = cached.as_ref() {
        if !is_expired(metadata) {
            return Ok(with_manifest(
                state,
                request,
                cache_key,
                metadata.clone(),
                body.clone(),
            ));
        }
    }

    let token = {
        let mut provider = state.token_provider.lock().await;
        provider.token(&state.client).await?
    };
    let mut upstream = state
        .client
        .get(&request.upstream_url)
        .header(USER_AGENT, USER_AGENT_VALUE)
        .header(ACCEPT, request.accept);
    if let Some(token) = token {
        upstream = upstream.header(AUTHORIZATION, format!("Bearer {token}"));
    }
    if let Some((metadata, _)) = cached.as_ref() {
        if let Some(etag) = metadata.etag.as_deref() {
            upstream = upstream.header(IF_NONE_MATCH, etag);
        }
        if let Some(last_modified) = metadata.last_modified.as_deref() {
            upstream = upstream.header(IF_MODIFIED_SINCE, last_modified);
        }
    }

    let response = upstream
        .send()
        .await
        .map_err(|error| GatewayError::Upstream(format!("GitHub request failed: {error}")))?;
    if response.status() == StatusCode::NOT_MODIFIED {
        if let Some((mut metadata, body)) = cached {
            let now = Utc::now();
            metadata.cached_at = format_time(now);
            metadata.expires_at =
                format_time(now + chrono::Duration::seconds(request.cache_ttl_seconds));
            write_cached(&metadata_path, &body_path, &metadata, &body)?;
            return Ok(with_manifest(state, request, cache_key, metadata, body));
        }
    }
    let status = response.status();
    let headers = response.headers().clone();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(GatewayError::Upstream(format!(
            "GitHub returned {status}: {}",
            github_error_message(&body).unwrap_or(body)
        )));
    }
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let etag = headers
        .get(ETAG)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let last_modified = headers
        .get(LAST_MODIFIED)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let body = response
        .bytes()
        .await
        .map_err(|error| GatewayError::Upstream(format!("GitHub body read failed: {error}")))?
        .to_vec();
    if body.len() > MAX_CACHE_BODY_BYTES {
        return Err(GatewayError::Upstream(format!(
            "source response exceeds cache limit of {MAX_CACHE_BODY_BYTES} bytes"
        )));
    }
    let now = Utc::now();
    let metadata = CacheMetadata {
        profile: CACHE_VERSION.to_string(),
        cache_key: cache_key.clone(),
        upstream_url: request.upstream_url.clone(),
        cached_at: format_time(now),
        expires_at: format_time(now + chrono::Duration::seconds(request.cache_ttl_seconds)),
        etag,
        last_modified,
        content_type,
        body_sha256: sha256_ref(&body),
    };
    write_cached(&metadata_path, &body_path, &metadata, &body)?;
    Ok(with_manifest(state, request, cache_key, metadata, body))
}

fn with_manifest(
    state: &AppState,
    request: SourceRequest,
    cache_key: String,
    metadata: CacheMetadata,
    body: Vec<u8>,
) -> FetchedSource {
    let served_at = format_time(Utc::now());
    let manifest = sign_manifest(
        &state.signer,
        &state.gateway_id,
        ManifestInput {
            repo: &request.repo,
            reference: request.reference.as_deref(),
            commit: request.commit.as_deref(),
            path: request.path.as_deref(),
            upstream_url: &request.upstream_url,
            cache_key: &cache_key,
            body_sha256: &metadata.body_sha256,
            etag: metadata.etag.as_deref(),
            fetched_at: &metadata.cached_at,
            served_at: &served_at,
        },
    );
    FetchedSource {
        body,
        content_type: metadata.content_type,
        manifest,
    }
}

struct ManifestInput<'a> {
    repo: &'a str,
    reference: Option<&'a str>,
    commit: Option<&'a str>,
    path: Option<&'a str>,
    upstream_url: &'a str,
    cache_key: &'a str,
    body_sha256: &'a str,
    etag: Option<&'a str>,
    fetched_at: &'a str,
    served_at: &'a str,
}

fn sign_manifest(
    signer: &SigningKey,
    gateway_id: &str,
    input: ManifestInput<'_>,
) -> SourceManifest {
    let unsigned = UnsignedSourceManifest {
        profile: MANIFEST_PROFILE,
        version: MANIFEST_VERSION,
        gateway_id,
        provider: "github",
        repo: input.repo,
        reference: input.reference,
        commit: input.commit,
        path: input.path,
        upstream_url: input.upstream_url,
        cache_key: input.cache_key,
        body_sha256: input.body_sha256,
        etag: input.etag,
        fetched_at: input.fetched_at,
        served_at: input.served_at,
    };
    let bytes = serde_json::to_vec(&unsigned).expect("source manifest serializes");
    let signature = signer.sign(&bytes);
    SourceManifest {
        profile: unsigned.profile.to_string(),
        version: unsigned.version.to_string(),
        gateway_id: unsigned.gateway_id.to_string(),
        provider: unsigned.provider.to_string(),
        repo: unsigned.repo.to_string(),
        reference: unsigned.reference.map(str::to_string),
        commit: unsigned.commit.map(str::to_string),
        path: unsigned.path.map(str::to_string),
        upstream_url: unsigned.upstream_url.to_string(),
        cache_key: unsigned.cache_key.to_string(),
        body_sha256: unsigned.body_sha256.to_string(),
        etag: unsigned.etag.map(str::to_string),
        fetched_at: unsigned.fetched_at.to_string(),
        served_at: unsigned.served_at.to_string(),
        signature: SourceManifestSignature {
            algorithm: "ed25519".to_string(),
            public_key_base64_url: public_key_base64(signer),
            signature_base64_url: URL_SAFE_NO_PAD.encode(signature.to_bytes()),
        },
    }
}

fn json_response_with_manifest(fetch: FetchedSource) -> Result<Response, GatewayError> {
    let mut value = serde_json::from_slice::<Value>(&fetch.body)
        .map_err(|error| GatewayError::Upstream(format!("GitHub JSON was invalid: {error}")))?;
    if let Value::Object(object) = &mut value {
        object.insert(
            "cyphesSourceManifest".to_string(),
            serde_json::to_value(&fetch.manifest)
                .map_err(|error| GatewayError::Internal(error.to_string()))?,
        );
    }
    let mut response = Json(value).into_response();
    attach_manifest_headers(response.headers_mut(), &fetch.manifest);
    Ok(response)
}

fn text_response_with_manifest(fetch: FetchedSource) -> Response {
    let mut response = fetch.body.into_response();
    if let Some(content_type) = fetch.content_type {
        if let Ok(value) = HeaderValue::from_str(&content_type) {
            response.headers_mut().insert(header::CONTENT_TYPE, value);
        }
    }
    attach_manifest_headers(response.headers_mut(), &fetch.manifest);
    response
}

fn attach_manifest_headers(headers: &mut HeaderMap, manifest: &SourceManifest) {
    if let Ok(value) = HeaderValue::from_str(&manifest.body_sha256) {
        headers.insert("x-cyphes-source-body-sha256", value);
    }
    if let Ok(value) = HeaderValue::from_str(&manifest.signature.public_key_base64_url) {
        headers.insert("x-cyphes-source-public-key", value);
    }
    if let Ok(value) = serde_json::to_string(manifest) {
        if let Ok(value) = HeaderValue::from_str(&URL_SAFE_NO_PAD.encode(value.as_bytes())) {
            headers.insert("x-cyphes-source-manifest", value);
        }
    }
}

impl TokenProvider {
    fn from_env() -> Result<Self, GatewayError> {
        if let (Ok(app_id), Ok(installation_id)) = (
            std::env::var("CYPHES_GITHUB_APP_ID"),
            std::env::var("CYPHES_GITHUB_INSTALLATION_ID"),
        ) {
            let private_key_pem = std::env::var("CYPHES_GITHUB_PRIVATE_KEY_PEM")
                .or_else(|_| {
                    std::env::var("CYPHES_GITHUB_PRIVATE_KEY_PATH").and_then(|path| {
                        fs::read_to_string(path).map_err(|_| std::env::VarError::NotPresent)
                    })
                })
                .map_err(|_| {
                    GatewayError::Internal(
                        "GitHub App auth requires CYPHES_GITHUB_PRIVATE_KEY_PEM or CYPHES_GITHUB_PRIVATE_KEY_PATH"
                            .to_string(),
                    )
                })?;
            return Ok(TokenProvider::GitHubApp {
                app_id,
                installation_id,
                private_key_pem,
                cached: None,
            });
        }
        Ok(TokenProvider::Static(
            ["CYPHES_GITHUB_TOKEN", "GITHUB_TOKEN"]
                .into_iter()
                .find_map(|key| std::env::var(key).ok())
                .map(|token| token.trim().to_string())
                .filter(|token| !token.is_empty()),
        ))
    }

    async fn token(&mut self, client: &reqwest::Client) -> Result<Option<String>, GatewayError> {
        match self {
            TokenProvider::Static(token) => Ok(token.clone()),
            TokenProvider::GitHubApp {
                app_id,
                installation_id,
                private_key_pem,
                cached,
            } => {
                if let Some(cached) = cached {
                    if cached.expires_at > Utc::now() + chrono::Duration::seconds(60) {
                        return Ok(Some(cached.token.clone()));
                    }
                }
                let token =
                    mint_installation_token(client, app_id, installation_id, private_key_pem)
                        .await?;
                *cached = Some(token.clone());
                Ok(Some(token.token))
            }
        }
    }
}

async fn mint_installation_token(
    client: &reqwest::Client,
    app_id: &str,
    installation_id: &str,
    private_key_pem: &str,
) -> Result<CachedToken, GatewayError> {
    let now = Utc::now().timestamp();
    let claims = GitHubAppClaims {
        iat: now - 60,
        exp: now + 9 * 60,
        iss: app_id.to_string(),
    };
    let jwt = jsonwebtoken::encode(
        &Header::new(Algorithm::RS256),
        &claims,
        &EncodingKey::from_rsa_pem(private_key_pem.as_bytes()).map_err(|error| {
            GatewayError::Internal(format!("GitHub App private key is invalid: {error}"))
        })?,
    )
    .map_err(|error| GatewayError::Internal(format!("GitHub App JWT failed: {error}")))?;
    let url = format!("https://api.github.com/app/installations/{installation_id}/access_tokens");
    let response = client
        .post(url)
        .header(USER_AGENT, USER_AGENT_VALUE)
        .header(ACCEPT, "application/vnd.github+json")
        .header(AUTHORIZATION, format!("Bearer {jwt}"))
        .send()
        .await
        .map_err(|error| {
            GatewayError::Upstream(format!("GitHub App token request failed: {error}"))
        })?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(GatewayError::Upstream(format!(
            "GitHub App token request returned {status}: {}",
            github_error_message(&body).unwrap_or(body)
        )));
    }
    let parsed = response
        .json::<InstallationTokenResponse>()
        .await
        .map_err(|error| {
            GatewayError::Upstream(format!("GitHub App token response invalid: {error}"))
        })?;
    Ok(CachedToken {
        token: parsed.token,
        expires_at: parsed.expires_at,
    })
}

fn read_cached(metadata_path: &Path, body_path: &Path) -> Option<(CacheMetadata, Vec<u8>)> {
    let metadata = fs::read_to_string(metadata_path).ok()?;
    let metadata = serde_json::from_str::<CacheMetadata>(&metadata).ok()?;
    let body = fs::read(body_path).ok()?;
    (metadata.body_sha256 == sha256_ref(&body)).then_some((metadata, body))
}

fn write_cached(
    metadata_path: &Path,
    body_path: &Path,
    metadata: &CacheMetadata,
    body: &[u8],
) -> Result<(), GatewayError> {
    if let Some(parent) = metadata_path.parent() {
        fs::create_dir_all(parent).map_err(|error| GatewayError::Internal(error.to_string()))?;
    }
    fs::write(body_path, body).map_err(|error| GatewayError::Internal(error.to_string()))?;
    let metadata_bytes = serde_json::to_vec_pretty(metadata)
        .map_err(|error| GatewayError::Internal(error.to_string()))?;
    fs::write(metadata_path, metadata_bytes)
        .map_err(|error| GatewayError::Internal(error.to_string()))
}

fn is_expired(metadata: &CacheMetadata) -> bool {
    DateTime::parse_from_rfc3339(&metadata.expires_at)
        .map(|time| time.with_timezone(&Utc) <= Utc::now())
        .unwrap_or(true)
}

fn normalize_repo(value: &str) -> Result<String, GatewayError> {
    let trimmed = value.trim().trim_matches('/');
    let parts = trimmed.split('/').collect::<Vec<_>>();
    if parts.len() != 2
        || parts
            .iter()
            .any(|part| part.is_empty() || !part.chars().all(valid_repo_char))
    {
        return Err(GatewayError::BadRequest(
            "repo must be owner/name using public GitHub path characters".to_string(),
        ));
    }
    Ok(format!("{}/{}", parts[0], parts[1]))
}

fn normalize_ref(value: &str) -> Result<String, GatewayError> {
    let trimmed = value.trim().trim_matches('/');
    if trimmed.is_empty() || trimmed.contains("..") || trimmed.contains('\\') {
        return Err(GatewayError::BadRequest("ref is invalid".to_string()));
    }
    Ok(trimmed.to_string())
}

fn normalize_commit(value: &str) -> Result<String, GatewayError> {
    let trimmed = value.trim();
    if !is_git_sha(trimmed) {
        return Err(GatewayError::BadRequest(
            "commit must be a 40-character Git SHA".to_string(),
        ));
    }
    Ok(trimmed.to_ascii_lowercase())
}

fn normalize_file_path(value: &str) -> Result<String, GatewayError> {
    let trimmed = value.trim().trim_start_matches('/');
    if trimmed.is_empty()
        || trimmed.contains("://")
        || trimmed.contains('\\')
        || trimmed
            .split('/')
            .any(|segment| segment.is_empty() || segment == "..")
    {
        return Err(GatewayError::BadRequest("path is invalid".to_string()));
    }
    Ok(trimmed.to_string())
}

fn valid_repo_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
}

fn is_git_sha(value: &str) -> bool {
    value.len() == 40 && value.chars().all(|character| character.is_ascii_hexdigit())
}

fn encode_path_segment(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(*byte as char);
            }
            other => encoded.push_str(&format!("%{other:02X}")),
        }
    }
    encoded
}

fn encode_raw_path(value: &str) -> String {
    value
        .split('/')
        .map(encode_path_segment)
        .collect::<Vec<_>>()
        .join("/")
}

fn default_ref() -> String {
    "main".to_string()
}

fn cache_key(value: &str) -> String {
    sha256_hex(value.as_bytes())
}

fn sha256_ref(bytes: &[u8]) -> String {
    format!("sha256:{}", sha256_hex(bytes))
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
}

fn format_time(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn github_error_message(body: &str) -> Option<String> {
    serde_json::from_str::<Value>(body).ok().and_then(|value| {
        value
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_string)
    })
}

fn cache_dir() -> PathBuf {
    std::env::var("CYPHES_SOURCE_GATEWAY_CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(".cyphes-source-cache"))
}

fn load_or_create_signing_key(cache_dir: &Path) -> Result<SigningKey, Box<dyn std::error::Error>> {
    if let Ok(value) = std::env::var("CYPHES_SOURCE_GATEWAY_SIGNING_KEY_BASE64") {
        let bytes = URL_SAFE_NO_PAD.decode(value.trim())?;
        let bytes: [u8; 32] = bytes
            .try_into()
            .map_err(|_| "source gateway signing key must be 32 bytes")?;
        return Ok(SigningKey::from_bytes(&bytes));
    }
    let path = cache_dir.join("gateway-signing-key.b64");
    if path.exists() {
        let bytes = URL_SAFE_NO_PAD.decode(fs::read_to_string(&path)?.trim())?;
        let bytes: [u8; 32] = bytes
            .try_into()
            .map_err(|_| "source gateway signing key must be 32 bytes")?;
        return Ok(SigningKey::from_bytes(&bytes));
    }
    let signing_key = SigningKey::generate(&mut OsRng);
    fs::create_dir_all(cache_dir)?;
    fs::write(path, URL_SAFE_NO_PAD.encode(signing_key.to_bytes()))?;
    Ok(signing_key)
}

fn public_key_base64(signer: &SigningKey) -> String {
    URL_SAFE_NO_PAD.encode(signer.verifying_key().to_bytes())
}

#[cfg(test)]
mod tests {
    use super::{encode_path_segment, normalize_commit, normalize_file_path, normalize_repo};

    #[test]
    fn validates_repo_commit_and_path_inputs() {
        assert_eq!(
            normalize_repo("aave/aave-v3-origin").unwrap(),
            "aave/aave-v3-origin"
        );
        assert!(normalize_repo("aave").is_err());
        assert!(normalize_commit("0123456789abcdef0123456789abcdef01234567").is_ok());
        assert!(normalize_commit("main").is_err());
        assert_eq!(
            normalize_file_path("/contracts/protocol/pool/Pool.sol").unwrap(),
            "contracts/protocol/pool/Pool.sol"
        );
        assert!(normalize_file_path("../secret").is_err());
    }

    #[test]
    fn encodes_refs_without_losing_branch_slashes() {
        assert_eq!(encode_path_segment("feature/a b"), "feature%2Fa%20b");
    }
}
