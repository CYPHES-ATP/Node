use std::time::{Duration, Instant};

use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

use crate::{
    audit_labor::{
        sha256_ref, AuditFinding, AuditWorkUnit, ContributionArtifact, CoverageItem,
        NodeContribution, ProtocolAuditCampaign, RuntimeDescriptor,
    },
    audit_profile::RepositoryTarget,
    github,
};

const AUDIT_SKILL_TEXT: &str = include_str!("../../protocol/skills/cyphes-audit-skill.v0.4.md");
const MAX_TREE_FILES: usize = 20_000;
const MAX_SELECTED_FILES: usize = 16;
const MAX_FILE_BYTES: usize = 28_000;
const MAX_CONTEXT_BYTES: usize = 180_000;
const MAX_MODEL_OUTPUT_TOKENS: u32 = 6_500;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalModelList {
    pub provider: String,
    pub provider_label: String,
    pub connected: bool,
    pub models: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditRuntimeProgress {
    pub campaign_id: String,
    pub work_unit_id: String,
    pub phase: String,
    pub progress: u8,
    pub tokens_per_second: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct LocalAuditSkillRun {
    pub runtime: RuntimeDescriptor,
    pub notes_markdown: String,
    pub findings: Vec<AuditFinding>,
    pub artifacts: Vec<ContributionArtifact>,
    pub coverage: Vec<CoverageItem>,
    pub commands: Vec<String>,
}

#[derive(Debug)]
struct RepositoryContext {
    inventory: Vec<String>,
    selected_files: Vec<SelectedFile>,
    truncated: bool,
}

#[derive(Debug)]
struct SelectedFile {
    path: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct GitTreeResponse {
    tree: Vec<GitTreeEntry>,
    #[serde(default)]
    truncated: bool,
}

#[derive(Debug, Deserialize)]
struct GitTreeEntry {
    path: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    size: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct OpenAiModelsResponse {
    data: Vec<OpenAiModel>,
}

#[derive(Debug, Deserialize)]
struct OpenAiModel {
    id: String,
}

#[derive(Debug, Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModel>,
}

#[derive(Debug, Deserialize)]
struct OllamaModel {
    name: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamChunk {
    #[serde(default)]
    choices: Vec<OpenAiStreamChoice>,
    usage: Option<OpenAiUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamChoice {
    delta: Option<OpenAiDelta>,
}

#[derive(Debug, Deserialize)]
struct OpenAiDelta {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiUsage {
    #[serde(default)]
    completion_tokens: Option<u64>,
    #[serde(default)]
    total_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct OllamaMessage {
    content: String,
}

#[derive(Debug, Deserialize)]
struct OllamaStreamChunk {
    #[serde(default)]
    message: Option<OllamaMessage>,
    #[serde(default)]
    done: bool,
    #[serde(default)]
    eval_count: Option<u64>,
    #[serde(default)]
    eval_duration: Option<u64>,
}

pub fn local_model_providers() -> Vec<LocalModelList> {
    vec![
        LocalModelList {
            provider: "lmstudio".to_string(),
            provider_label: "LM Studio".to_string(),
            connected: false,
            models: Vec::new(),
            message: "Start LM Studio's local server to load models.".to_string(),
        },
        LocalModelList {
            provider: "ollama".to_string(),
            provider_label: "Ollama".to_string(),
            connected: false,
            models: Vec::new(),
            message: "Start Ollama and pull a local model to load models.".to_string(),
        },
    ]
}

pub async fn list_local_models(provider: &str) -> LocalModelList {
    let provider = normalize_provider(provider);
    let label = provider_label(provider);
    let result = match provider {
        "lmstudio" => list_openai_compatible_models(lmstudio_endpoint()).await,
        "ollama" => list_ollama_models(ollama_endpoint()).await,
        _ => Err("unsupported local model provider".to_string()),
    };

    match result {
        Ok(mut models) => {
            models.sort();
            LocalModelList {
                provider: provider.to_string(),
                provider_label: label.to_string(),
                connected: true,
                message: if models.is_empty() {
                    "Connected, but no models were returned.".to_string()
                } else {
                    format!("{} local model(s) available.", models.len())
                },
                models,
            }
        }
        Err(error) => LocalModelList {
            provider: provider.to_string(),
            provider_label: label.to_string(),
            connected: false,
            models: Vec::new(),
            message: error,
        },
    }
}

pub async fn run_local_audit_skill(
    app: &AppHandle,
    campaign: &ProtocolAuditCampaign,
    work_unit: &AuditWorkUnit,
    provider: &str,
    model: &str,
    prior_contributions: &[NodeContribution],
) -> Result<LocalAuditSkillRun, String> {
    let provider = normalize_provider(provider);
    if model.trim().is_empty() {
        return Err("Select a local model before running the audit skill.".to_string());
    }

    emit_progress(app, campaign, work_unit, "Preparing audit skill", 5, None);
    let client = client()?;

    emit_progress(
        app,
        campaign,
        work_unit,
        "Reading pinned GitHub context",
        18,
        None,
    );
    let context = repository_context(&client, campaign).await?;

    emit_progress(app, campaign, work_unit, "Building model prompt", 32, None);
    let prompt = build_prompt(campaign, work_unit, &context, prior_contributions);
    let input_hash = sha256_ref(prompt.as_bytes());
    let skill_hash = sha256_ref(effective_skill_text(campaign).as_bytes());

    emit_progress(app, campaign, work_unit, "Running local model", 44, None);
    let started = Instant::now();
    let model_output = match provider {
        "lmstudio" => {
            run_openai_compatible_chat(
                app,
                campaign,
                work_unit,
                &client,
                lmstudio_endpoint(),
                model,
                &prompt,
            )
            .await?
        }
        "ollama" => {
            run_ollama_chat(
                app,
                campaign,
                work_unit,
                &client,
                ollama_endpoint(),
                model,
                &prompt,
            )
            .await?
        }
        _ => return Err("unsupported local model provider".to_string()),
    };
    let elapsed = started.elapsed();
    let tokens_per_second = model_output
        .tokens_per_second
        .or_else(|| {
            model_output.generated_tokens.and_then(|tokens| {
                let seconds = elapsed.as_secs_f64();
                (seconds > 0.0).then_some(tokens as f64 / seconds)
            })
        })
        .or_else(|| estimated_tokens_per_second(&model_output.content, elapsed));
    emit_progress(
        app,
        campaign,
        work_unit,
        "Parsing structured output",
        76,
        tokens_per_second,
    );

    let parsed = parse_model_output(&model_output.content);
    if parsed.parser_fallback {
        emit_progress(
            app,
            campaign,
            work_unit,
            "ATP quality deduction: parser fallback, 0 structured findings, -90% projected reward",
            80,
            tokens_per_second,
        );
    }
    let output_hash = sha256_ref(model_output.content.as_bytes());
    let runtime_json = serde_json::to_vec_pretty(&json!({
        "provider": provider,
        "providerLabel": provider_label(provider),
        "endpointClass": "local",
        "model": model,
        "skillHash": skill_hash,
        "inputHash": input_hash,
        "outputHash": output_hash,
        "tokensPerSecond": tokens_per_second,
        "parserFallback": parsed.parser_fallback,
        "structuredFindingCount": parsed.findings.len(),
        "structuredReportableFindingCount": parsed.findings.iter().filter(|finding| finding.reportable).count(),
        "creditQualityMultiplier": if parsed.parser_fallback { 0.10 } else { 1.0 },
        "repository": campaign.repository,
        "workUnitId": work_unit.work_unit_id,
        "selectedFiles": context.selected_files.iter().map(|file| &file.path).collect::<Vec<_>>(),
            "treeTruncated": context.truncated,
            "priorContributionCount": prior_contributions.len(),
    }))
    .map_err(|error| error.to_string())?;
    let findings_json =
        serde_json::to_vec_pretty(&parsed.findings).map_err(|error| error.to_string())?;
    let coverage_json =
        serde_json::to_vec_pretty(&parsed.coverage).map_err(|error| error.to_string())?;

    let artifacts = vec![
        artifact(
            "audit-skill-output.md",
            "text/markdown",
            parsed.notes_markdown.as_bytes(),
        ),
        artifact("findings.json", "application/json", &findings_json),
        artifact("coverage.json", "application/json", &coverage_json),
        artifact("runtime.json", "application/json", &runtime_json),
    ];

    emit_progress(
        app,
        campaign,
        work_unit,
        "Signing model contribution",
        92,
        tokens_per_second,
    );

    let runtime = RuntimeDescriptor {
        operator: "CYPHES local model runtime".to_string(),
        adapter: provider_adapter(provider).to_string(),
        model: model.to_string(),
        model_multiplier: model_multiplier(model),
        tool_policy: vec![
            "github-read-only-pinned-commit".to_string(),
            "no-repository-writes".to_string(),
            "no-untrusted-code-execution".to_string(),
            "local-model-only".to_string(),
        ],
        connected: true,
        endpoint_class: Some("local".to_string()),
        skill_hash: Some(skill_hash),
        input_hash: Some(input_hash),
        output_hash: Some(output_hash),
        tokens_per_second,
    };

    emit_progress(app, campaign, work_unit, "Complete", 100, tokens_per_second);

    Ok(LocalAuditSkillRun {
        runtime,
        notes_markdown: parsed.notes_markdown,
        findings: parsed.findings,
        artifacts,
        coverage: parsed.coverage,
        commands: parsed.commands,
    })
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|error| error.to_string())
}

fn emit_progress(
    app: &AppHandle,
    campaign: &ProtocolAuditCampaign,
    work_unit: &AuditWorkUnit,
    phase: &str,
    progress: u8,
    tokens_per_second: Option<f64>,
) {
    let _ = app.emit(
        "audit:runtime_progress",
        AuditRuntimeProgress {
            campaign_id: campaign.campaign_id.clone(),
            work_unit_id: work_unit.work_unit_id.clone(),
            phase: phase.to_string(),
            progress,
            tokens_per_second,
        },
    );
}

async fn list_openai_compatible_models(endpoint: &str) -> Result<Vec<String>, String> {
    let response = client()?
        .get(format!("{endpoint}/models"))
        .send()
        .await
        .map_err(|error| format!("LM Studio is not reachable. Start the local server. {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "LM Studio model list returned {}",
            response.status()
        ));
    }
    let body = response
        .json::<OpenAiModelsResponse>()
        .await
        .map_err(|error| format!("LM Studio model list was not OpenAI-compatible JSON. {error}"))?;
    Ok(body.data.into_iter().map(|model| model.id).collect())
}

async fn list_ollama_models(endpoint: &str) -> Result<Vec<String>, String> {
    let response = client()?
        .get(format!("{endpoint}/api/tags"))
        .send()
        .await
        .map_err(|error| format!("Ollama is not reachable. Start Ollama locally. {error}"))?;
    if !response.status().is_success() {
        return Err(format!("Ollama model list returned {}", response.status()));
    }
    let body = response
        .json::<OllamaTagsResponse>()
        .await
        .map_err(|error| format!("Ollama tags response was not valid JSON. {error}"))?;
    Ok(body.models.into_iter().map(|model| model.name).collect())
}

async fn run_openai_compatible_chat(
    app: &AppHandle,
    campaign: &ProtocolAuditCampaign,
    work_unit: &AuditWorkUnit,
    client: &reqwest::Client,
    endpoint: &str,
    model: &str,
    prompt: &str,
) -> Result<ModelOutput, String> {
    let response = client
        .post(format!("{endpoint}/chat/completions"))
        .json(&json!({
            "model": model,
            "temperature": 0.2,
            "max_tokens": MAX_MODEL_OUTPUT_TOKENS,
            "stream": true,
            "stream_options": {
                "include_usage": true
            },
            "messages": [
                {
                    "role": "system",
                    "content": "You are CYPHES Audit Skill. Return valid JSON only."
                },
                {
                    "role": "user",
                    "content": prompt
                }
            ]
        }))
        .send()
        .await
        .map_err(|error| format!("Local model request failed: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("Local model returned {}", response.status()));
    }
    let started = Instant::now();
    let mut last_emit = Instant::now() - Duration::from_millis(250);
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut content = String::new();
    let mut tokens = None;
    let mut tokens_per_second = None;
    let mut done = false;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| format!("Local model stream failed: {error}"))?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(index) = buffer.find('\n') {
            let line = buffer.drain(..=index).collect::<String>();
            let before = content.len();
            if handle_openai_stream_line(&line, &mut content, &mut tokens)? {
                done = true;
                break;
            }
            if content.len() != before {
                tokens_per_second = maybe_emit_stream_progress(
                    app,
                    campaign,
                    work_unit,
                    &content,
                    started,
                    &mut last_emit,
                    false,
                );
            }
        }
        if done {
            break;
        }
    }
    if !done && !buffer.trim().is_empty() {
        let before = content.len();
        let _ = handle_openai_stream_line(&buffer, &mut content, &mut tokens)?;
        if content.len() != before {
            tokens_per_second = maybe_emit_stream_progress(
                app,
                campaign,
                work_unit,
                &content,
                started,
                &mut last_emit,
                true,
            );
        }
    }
    if content.trim().is_empty() {
        return Err("Local model returned an empty streamed response".to_string());
    }
    tokens_per_second = tokens
        .and_then(|tokens| {
            let seconds = started.elapsed().as_secs_f64();
            (seconds > 0.0).then_some(tokens as f64 / seconds)
        })
        .or_else(|| stream_tokens_per_second(&content, started))
        .or(tokens_per_second);
    Ok(ModelOutput {
        content,
        generated_tokens: tokens,
        tokens_per_second,
    })
}

async fn run_ollama_chat(
    app: &AppHandle,
    campaign: &ProtocolAuditCampaign,
    work_unit: &AuditWorkUnit,
    client: &reqwest::Client,
    endpoint: &str,
    model: &str,
    prompt: &str,
) -> Result<ModelOutput, String> {
    let response = client
        .post(format!("{endpoint}/api/chat"))
        .json(&json!({
            "model": model,
            "stream": true,
            "options": {
                "temperature": 0.2,
                "num_predict": MAX_MODEL_OUTPUT_TOKENS
            },
            "messages": [
                {
                    "role": "system",
                    "content": "You are CYPHES Audit Skill. Return valid JSON only."
                },
                {
                    "role": "user",
                    "content": prompt
                }
            ]
        }))
        .send()
        .await
        .map_err(|error| format!("Ollama audit request failed: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("Ollama returned {}", response.status()));
    }
    let started = Instant::now();
    let mut last_emit = Instant::now() - Duration::from_millis(250);
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut content = String::new();
    let mut eval_count = None;
    let mut eval_duration = None;
    let mut tokens_per_second = None;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| format!("Ollama stream failed: {error}"))?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(index) = buffer.find('\n') {
            let line = buffer.drain(..=index).collect::<String>();
            let before = content.len();
            let chunk_done = handle_ollama_stream_line(
                &line,
                &mut content,
                &mut eval_count,
                &mut eval_duration,
            )?;
            if content.len() != before {
                tokens_per_second = maybe_emit_stream_progress(
                    app,
                    campaign,
                    work_unit,
                    &content,
                    started,
                    &mut last_emit,
                    false,
                );
            }
            if chunk_done {
                break;
            }
        }
    }
    if !buffer.trim().is_empty() {
        let before = content.len();
        let _ =
            handle_ollama_stream_line(&buffer, &mut content, &mut eval_count, &mut eval_duration)?;
        if content.len() != before {
            tokens_per_second = maybe_emit_stream_progress(
                app,
                campaign,
                work_unit,
                &content,
                started,
                &mut last_emit,
                true,
            );
        }
    }
    if content.trim().is_empty() {
        return Err("Ollama returned an empty streamed response".to_string());
    }
    let tokens_per_second = match (eval_count, eval_duration) {
        (Some(count), Some(duration)) if duration > 0 => {
            Some((count as f64) / ((duration as f64) / 1_000_000_000.0))
        }
        _ => tokens_per_second,
    };
    Ok(ModelOutput {
        content,
        generated_tokens: eval_count,
        tokens_per_second,
    })
}

fn handle_openai_stream_line(
    line: &str,
    content: &mut String,
    tokens: &mut Option<u64>,
) -> Result<bool, String> {
    let line = line.trim();
    if line.is_empty() || line.starts_with(':') {
        return Ok(false);
    }
    let data = if let Some(data) = line.strip_prefix("data:") {
        data.trim()
    } else if line.starts_with('{') {
        line
    } else {
        return Ok(false);
    };
    if data == "[DONE]" {
        return Ok(true);
    }
    let chunk = serde_json::from_str::<OpenAiStreamChunk>(data)
        .map_err(|error| format!("Local model stream was not OpenAI-compatible JSON. {error}"))?;
    if let Some(usage) = chunk.usage {
        *tokens = usage.completion_tokens.or(usage.total_tokens).or(*tokens);
    }
    for choice in chunk.choices {
        if let Some(part) = choice.delta.and_then(|delta| delta.content) {
            content.push_str(&part);
        }
    }
    Ok(false)
}

fn handle_ollama_stream_line(
    line: &str,
    content: &mut String,
    eval_count: &mut Option<u64>,
    eval_duration: &mut Option<u64>,
) -> Result<bool, String> {
    let line = line.trim();
    if line.is_empty() {
        return Ok(false);
    }
    let chunk = serde_json::from_str::<OllamaStreamChunk>(line)
        .map_err(|error| format!("Ollama stream line was not valid JSON. {error}"))?;
    if let Some(message) = chunk.message {
        content.push_str(&message.content);
    }
    if chunk.eval_count.is_some() {
        *eval_count = chunk.eval_count;
    }
    if chunk.eval_duration.is_some() {
        *eval_duration = chunk.eval_duration;
    }
    Ok(chunk.done)
}

fn maybe_emit_stream_progress(
    app: &AppHandle,
    campaign: &ProtocolAuditCampaign,
    work_unit: &AuditWorkUnit,
    content: &str,
    started: Instant,
    last_emit: &mut Instant,
    force: bool,
) -> Option<f64> {
    let tokens_per_second = stream_tokens_per_second(content, started);
    if force || last_emit.elapsed() >= Duration::from_millis(200) {
        emit_progress(
            app,
            campaign,
            work_unit,
            "Streaming local model output",
            stream_progress(started, content),
            tokens_per_second,
        );
        *last_emit = Instant::now();
    }
    tokens_per_second
}

fn stream_progress(started: Instant, content: &str) -> u8 {
    let elapsed_motion = started.elapsed().as_secs_f64() * 1.8;
    let token_motion = estimated_token_count(content) / 80.0;
    44 + ((elapsed_motion + token_motion).floor() as u8).min(30)
}

fn stream_tokens_per_second(content: &str, started: Instant) -> Option<f64> {
    let elapsed = started.elapsed();
    let seconds = elapsed.as_secs_f64();
    if seconds <= 0.0 {
        return None;
    }
    Some(estimated_token_count(content) / seconds)
}

#[derive(Debug)]
struct ModelOutput {
    content: String,
    generated_tokens: Option<u64>,
    tokens_per_second: Option<f64>,
}

async fn repository_context(
    client: &reqwest::Client,
    campaign: &ProtocolAuditCampaign,
) -> Result<RepositoryContext, String> {
    let repository = &campaign.repository;
    let tree_url = format!(
        "https://api.github.com/repos/{}/git/trees/{}?recursive=1",
        repository.full_name, repository.commit_sha
    );
    let tree = github::get_json::<GitTreeResponse>(client, &tree_url)
        .await
        .map_err(|error| format!("GitHub tree read failed. {error}"))?;
    let blobs = tree
        .tree
        .into_iter()
        .filter(|entry| entry.kind == "blob")
        .take(MAX_TREE_FILES)
        .collect::<Vec<_>>();
    let inventory = blobs
        .iter()
        .map(|entry| entry.path.clone())
        .collect::<Vec<_>>();
    let scoped_paths = scoped_paths_from_campaign(campaign);
    let selected_paths = select_context_files(&blobs, &scoped_paths);
    let mut selected_files = Vec::new();
    let mut total_bytes = 0usize;
    for path in selected_paths {
        if total_bytes >= MAX_CONTEXT_BYTES {
            break;
        }
        match fetch_raw_file(client, repository, &path).await {
            Ok(content) => {
                let content = truncate_bytes(&content, MAX_FILE_BYTES);
                total_bytes += content.len();
                selected_files.push(SelectedFile { path, content });
            }
            Err(error)
                if error.contains("GitHub rate limit")
                    || error.contains("GitHub paused")
                    || error.contains("GitHub API") =>
            {
                return Err(error);
            }
            Err(_) => {}
        }
    }

    Ok(RepositoryContext {
        inventory,
        selected_files,
        truncated: tree.truncated,
    })
}

fn select_context_files(blobs: &[GitTreeEntry], scoped_paths: &[String]) -> Vec<String> {
    let mut selected = Vec::new();
    for scoped_path in scoped_paths {
        for entry in blobs {
            if selected.len() >= MAX_SELECTED_FILES {
                break;
            }
            if selected.iter().any(|path| path == &entry.path) {
                continue;
            }
            let size_ok = entry.size.unwrap_or(0) <= MAX_FILE_BYTES as u64;
            let in_scope =
                entry.path == *scoped_path || entry.path.starts_with(&format!("{scoped_path}/"));
            if size_ok && in_scope && looks_textual(&entry.path) {
                selected.push(entry.path.clone());
            }
        }
    }
    for entry in blobs {
        if selected.len() >= MAX_SELECTED_FILES {
            break;
        }
        let path = entry.path.as_str();
        let size_ok = entry.size.unwrap_or(0) <= MAX_FILE_BYTES as u64;
        if size_ok && is_priority_context_file(path) {
            selected.push(path.to_string());
        }
    }
    if selected.len() < 8 {
        for entry in blobs {
            if selected.len() >= MAX_SELECTED_FILES {
                break;
            }
            if selected.iter().any(|path| path == &entry.path) {
                continue;
            }
            let size_ok = entry.size.unwrap_or(0) <= MAX_FILE_BYTES as u64;
            if size_ok && looks_textual(&entry.path) {
                selected.push(entry.path.clone());
            }
        }
    }
    selected
}

fn scoped_paths_from_campaign(campaign: &ProtocolAuditCampaign) -> Vec<String> {
    let mut paths = Vec::new();
    for source in [
        Some(campaign.scope_text.as_str()),
        campaign.audit_brief_text.as_deref(),
        campaign.custom_skill_text.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        for line in source.lines() {
            let trimmed = line
                .trim()
                .trim_start_matches("- ")
                .trim_start_matches("* ")
                .trim();
            let lower = trimmed.to_ascii_lowercase();
            for prefix in [
                "focused path:",
                "focused file:",
                "focused directory:",
                "in-scope path:",
                "in scope path:",
            ] {
                if lower.starts_with(prefix) {
                    if let Some(path) = normalize_scoped_path(&trimmed[prefix.len()..]) {
                        if !paths.iter().any(|existing| existing == &path) {
                            paths.push(path);
                        }
                    }
                }
            }
        }
    }
    for attachment in &campaign.attachments {
        if let Some(text) = &attachment.text {
            for line in text.lines() {
                let trimmed = line
                    .trim()
                    .trim_start_matches("- ")
                    .trim_start_matches("* ")
                    .trim();
                let lower = trimmed.to_ascii_lowercase();
                for prefix in [
                    "focused path:",
                    "focused file:",
                    "focused directory:",
                    "in-scope path:",
                    "in scope path:",
                ] {
                    if lower.starts_with(prefix) {
                        if let Some(path) = normalize_scoped_path(&trimmed[prefix.len()..]) {
                            if !paths.iter().any(|existing| existing == &path) {
                                paths.push(path);
                            }
                        }
                    }
                }
            }
        }
    }
    paths
}

fn normalize_scoped_path(value: &str) -> Option<String> {
    let path = value
        .trim()
        .trim_matches('`')
        .trim_matches('"')
        .trim_matches('\'')
        .trim_start_matches('/')
        .trim();
    if path.is_empty()
        || path.contains("://")
        || path == "."
        || path.split('/').any(|segment| segment == "..")
    {
        return None;
    }
    Some(path.to_string())
}

fn is_priority_context_file(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower == "readme.md"
        || lower == "security.md"
        || lower == "package.json"
        || lower == "package-lock.json"
        || lower == "pnpm-lock.yaml"
        || lower == "yarn.lock"
        || lower == "cargo.toml"
        || lower == "cargo.lock"
        || lower == "go.mod"
        || lower == "go.sum"
        || lower == "pyproject.toml"
        || lower == "requirements.txt"
        || lower == "foundry.toml"
        || lower == "hardhat.config.ts"
        || lower == "hardhat.config.js"
        || lower == ".github/dependabot.yml"
        || lower.starts_with(".github/workflows/")
}

fn looks_textual(path: &str) -> bool {
    matches!(
        path.rsplit('.')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "md" | "txt"
            | "json"
            | "toml"
            | "yaml"
            | "yml"
            | "rs"
            | "ts"
            | "tsx"
            | "js"
            | "jsx"
            | "go"
            | "py"
            | "sol"
    )
}

async fn fetch_raw_file(
    client: &reqwest::Client,
    repository: &RepositoryTarget,
    path: &str,
) -> Result<String, String> {
    let url = format!(
        "https://raw.githubusercontent.com/{}/{}/{}",
        repository.full_name,
        repository.commit_sha,
        path.replace(' ', "%20")
    );
    github::get_text(client, &url)
        .await
        .map_err(|error| format!("raw file read failed for {path}: {error}"))
}

fn build_prompt(
    campaign: &ProtocolAuditCampaign,
    work_unit: &AuditWorkUnit,
    context: &RepositoryContext,
    prior_contributions: &[NodeContribution],
) -> String {
    let skill_text = effective_skill_text(campaign);
    let attachment_digest = attachment_digest(campaign);
    let mut prompt = format!(
        "{}\n\n\
         # Campaign\n\
         Protocol: {}\n\
         Repository: {} at {}\n\
         Skill pack: {} {} ({})\n\
         Custom SKILL hash: {}\n\
         Scope:\n{}\n\n\
         Audit brief:\n{}\n\n\
         In-scope impacts: {}\n\
         Out-of-scope: {}\n\n\
         # Requester Attachments\n{}\n\n\
         # Work Unit\n\
         Kind: {}\n\
         Title: {}\n\
         Instructions: {}\n\n\
         # Repository Inventory\n\
         Tree truncated by GitHub: {}\n\
         Files inventoried: {}\n{}\n\n\
         # Prior Accepted Or Submitted CYPHES Passes\n{}\n\n\
         # Selected File Context\n",
        skill_text,
        campaign.protocol_name,
        campaign.repository.full_name,
        campaign.repository.commit_sha,
        campaign.skill_pack.skill_pack_id,
        campaign.skill_pack.version,
        campaign.skill_pack.hash,
        campaign.custom_skill_hash.as_deref().unwrap_or("none"),
        campaign.scope_text,
        campaign
            .audit_brief_text
            .as_deref()
            .unwrap_or("No requester audit brief supplied."),
        if campaign.impacts_in_scope.is_empty() {
            "not supplied".to_string()
        } else {
            campaign.impacts_in_scope.join("; ")
        },
        if campaign.out_of_scope.is_empty() {
            "not supplied".to_string()
        } else {
            campaign.out_of_scope.join("; ")
        },
        attachment_digest,
        work_unit.kind,
        work_unit.title,
        work_unit.instructions,
        context.truncated,
        context.inventory.len(),
        context
            .inventory
            .iter()
            .take(250)
            .map(|path| format!("- {path}"))
            .collect::<Vec<_>>()
            .join("\n"),
        prior_contribution_digest(prior_contributions)
    );

    for file in &context.selected_files {
        prompt.push_str(&format!(
            "\n## {}\n```text\n{}\n```\n",
            file.path, file.content
        ));
    }
    prompt
}

fn effective_skill_text(campaign: &ProtocolAuditCampaign) -> String {
    match campaign.custom_skill_text.as_deref() {
        Some(custom) if !custom.trim().is_empty() => format!(
            "{}\n\n# Requester Custom SKILL.md Overlay\n\n{}",
            AUDIT_SKILL_TEXT,
            custom.trim()
        ),
        _ => AUDIT_SKILL_TEXT.to_string(),
    }
}

fn attachment_digest(campaign: &ProtocolAuditCampaign) -> String {
    if campaign.attachments.is_empty() {
        return "No requester attachments supplied.".to_string();
    }
    campaign
        .attachments
        .iter()
        .map(|attachment| {
            format!(
                "## {}\nMedia type: {}\nHash: {}\nBytes: {}\n{}",
                attachment.label,
                attachment.media_type,
                attachment.sha256,
                attachment.size_bytes,
                attachment
                    .text
                    .as_deref()
                    .map(|text| truncate_bytes(text, 24_000))
                    .unwrap_or_else(
                        || "Binary or external attachment text not embedded.".to_string()
                    )
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn prior_contribution_digest(contributions: &[NodeContribution]) -> String {
    if contributions.is_empty() {
        return "No prior passes supplied for this campaign run.".to_string();
    }
    contributions
        .iter()
        .take(8)
        .map(|contribution| {
            let findings = if contribution.findings.is_empty() {
                "no findings recorded".to_string()
            } else {
                contribution
                    .findings
                    .iter()
                    .take(6)
                    .map(|finding| {
                        format!(
                            "{} [{} / {}]: {}",
                            finding.id, finding.severity, finding.status, finding.title
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("; ")
            };
            let coverage = contribution
                .coverage
                .iter()
                .take(6)
                .map(|item| format!("{}={}", item.area, item.status))
                .collect::<Vec<_>>()
                .join("; ");
            format!(
                "## Prior pass: {}\nWork unit: {}\nReceipt: {}\nModel: {} / {}\nCoverage: {}\nFindings/leads: {}\nNotes:\n{}\n",
                contribution.contribution_id,
                contribution.work_unit_id,
                contribution.receipt_hash,
                contribution.runtime.adapter,
                contribution.runtime.model,
                if coverage.is_empty() { "none declared" } else { &coverage },
                findings,
                truncate_bytes(&contribution.notes_markdown, 6_000)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Debug)]
struct ParsedModelOutput {
    notes_markdown: String,
    findings: Vec<AuditFinding>,
    coverage: Vec<CoverageItem>,
    commands: Vec<String>,
    parser_fallback: bool,
}

fn parse_model_output(content: &str) -> ParsedModelOutput {
    let parsed = extract_json(content).and_then(|value| parse_json_output(&value));
    parsed.unwrap_or_else(|reason| ParsedModelOutput {
        notes_markdown: format!(
            "{}\n\n> CYPHES parser note: model output was not valid structured JSON: {}",
            content.trim(),
            reason
        ),
        findings: Vec::new(),
        coverage: vec![CoverageItem {
            area: "local model output".to_string(),
            status: "needs_review".to_string(),
            evidence: vec![
                "Model returned unstructured output; no reportable finding accepted.".to_string(),
            ],
        }],
        commands: vec![
            "local model audit skill response captured; structured parse failed".to_string(),
        ],
        parser_fallback: true,
    })
}

fn extract_json(content: &str) -> Result<Value, String> {
    let trimmed = content.trim();
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return Ok(value);
    }
    let start = trimmed
        .find('{')
        .ok_or_else(|| "no JSON object start found".to_string())?;
    let end = trimmed
        .rfind('}')
        .ok_or_else(|| "no JSON object end found".to_string())?;
    serde_json::from_str::<Value>(&trimmed[start..=end]).map_err(|error| error.to_string())
}

fn parse_json_output(value: &Value) -> Result<ParsedModelOutput, String> {
    let notes_markdown = value
        .get("summaryMarkdown")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "summaryMarkdown is required".to_string())?
        .to_string();
    let findings = value
        .get("findings")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .enumerate()
                .map(value_to_finding)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let coverage = value
        .get("coverage")
        .and_then(Value::as_array)
        .map(|items| items.iter().map(value_to_coverage).collect::<Vec<_>>())
        .unwrap_or_else(|| {
            vec![CoverageItem {
                area: "model review".to_string(),
                status: "completed".to_string(),
                evidence: vec!["Model returned summaryMarkdown.".to_string()],
            }]
        });
    let commands = value
        .get("commands")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .filter(|items| !items.is_empty())
        .unwrap_or_else(|| vec!["local model audit skill completed".to_string()]);
    Ok(ParsedModelOutput {
        notes_markdown,
        findings,
        coverage,
        commands,
        parser_fallback: false,
    })
}

fn value_to_finding((index, value): (usize, &Value)) -> AuditFinding {
    let reportable = value
        .get("reportable")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    AuditFinding {
        id: string_field(value, "id").unwrap_or_else(|| format!("CYPHES-LOCAL-{:03}", index + 1)),
        title: string_field(value, "title").unwrap_or_else(|| "Untitled model lead".to_string()),
        severity: string_field(value, "severity").unwrap_or_else(|| "informational".to_string()),
        status: string_field(value, "status").unwrap_or_else(|| {
            if reportable {
                "candidate".to_string()
            } else {
                "non_reportable".to_string()
            }
        }),
        impact: value
            .get("impact")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        evidence: string_array(value, "evidence"),
        reportable,
    }
}

fn value_to_coverage(value: &Value) -> CoverageItem {
    CoverageItem {
        area: string_field(value, "area").unwrap_or_else(|| "model review".to_string()),
        status: string_field(value, "status").unwrap_or_else(|| "completed".to_string()),
        evidence: string_array(value, "evidence"),
    }
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn string_array(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn artifact(path: &str, media_type: &str, bytes: &[u8]) -> ContributionArtifact {
    ContributionArtifact {
        path: path.to_string(),
        media_type: media_type.to_string(),
        sha256: sha256_ref(bytes),
        size_bytes: bytes.len() as u64,
    }
}

fn normalize_provider(provider: &str) -> &str {
    match provider {
        "ollama" => "ollama",
        _ => "lmstudio",
    }
}

fn provider_label(provider: &str) -> &str {
    match provider {
        "ollama" => "Ollama",
        _ => "LM Studio",
    }
}

fn provider_adapter(provider: &str) -> &str {
    match provider {
        "ollama" => "ollama-local",
        _ => "lmstudio-openai-compatible",
    }
}

fn lmstudio_endpoint() -> &'static str {
    "http://localhost:1234/v1"
}

fn ollama_endpoint() -> &'static str {
    "http://localhost:11434"
}

fn model_multiplier(model: &str) -> f64 {
    let lower = model.to_ascii_lowercase();
    if lower.contains("70b") || lower.contains("72b") {
        1.0
    } else if lower.contains("32b") || lower.contains("34b") || lower.contains("20b") {
        0.9
    } else if lower.contains("14b") || lower.contains("13b") {
        0.8
    } else if lower.contains("7b") || lower.contains("8b") {
        0.7
    } else {
        0.75
    }
}

fn estimated_tokens_per_second(content: &str, elapsed: Duration) -> Option<f64> {
    let seconds = elapsed.as_secs_f64();
    if seconds <= 0.0 {
        return None;
    }
    Some(estimated_token_count(content) / seconds)
}

fn estimated_token_count(content: &str) -> f64 {
    let words = content.split_whitespace().count() as f64;
    if words == 0.0 && !content.is_empty() {
        1.0
    } else {
        words * 1.3
    }
}

fn truncate_bytes(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) && end > 0 {
        end -= 1;
    }
    format!("{}\n\n[truncated by CYPHES]", &value[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_structured_model_output() {
        let output = parse_model_output(
            r#"{
              "summaryMarkdown": "Reviewed README and workflow.",
              "findings": [{"id":"X","title":"Lead","severity":"low","status":"non_reportable","impact":null,"evidence":["README.md"],"reportable":false}],
              "coverage": [{"area":"scope","status":"completed","evidence":["README.md"]}],
              "commands": ["read GitHub context"]
            }"#,
        );
        assert_eq!(output.findings.len(), 1);
        assert_eq!(output.coverage[0].area, "scope");
        assert!(output.notes_markdown.contains("Reviewed"));
    }

    #[test]
    fn unstructured_model_output_does_not_create_findings() {
        let output = parse_model_output("I looked around and it seems okay.");
        assert!(output.findings.is_empty());
        assert_eq!(output.coverage[0].status, "needs_review");
        assert!(output.parser_fallback);
    }

    #[test]
    fn parses_openai_compatible_stream_chunks() {
        let mut content = String::new();
        let mut tokens = None;
        let done = handle_openai_stream_line(
            r#"data: {"choices":[{"delta":{"content":"{\"summaryMarkdown\":\""}}]}"#,
            &mut content,
            &mut tokens,
        )
        .unwrap();
        assert!(!done);
        let done = handle_openai_stream_line(
            r#"data: {"choices":[{"delta":{"content":"streamed\"}"}}],"usage":{"completion_tokens":42}}"#,
            &mut content,
            &mut tokens,
        )
        .unwrap();
        assert!(!done);
        assert_eq!(tokens, Some(42));
        assert_eq!(content, r#"{"summaryMarkdown":"streamed"}"#);
        assert!(handle_openai_stream_line("data: [DONE]", &mut content, &mut tokens).unwrap());
    }

    #[test]
    fn parses_ollama_stream_chunks() {
        let mut content = String::new();
        let mut eval_count = None;
        let mut eval_duration = None;
        let done = handle_ollama_stream_line(
            r#"{"message":{"content":"{\"summaryMarkdown\":\""},"done":false}"#,
            &mut content,
            &mut eval_count,
            &mut eval_duration,
        )
        .unwrap();
        assert!(!done);
        let done = handle_ollama_stream_line(
            r#"{"message":{"content":"streamed\"}"},"done":true,"eval_count":24,"eval_duration":1200000000}"#,
            &mut content,
            &mut eval_count,
            &mut eval_duration,
        )
        .unwrap();
        assert!(done);
        assert_eq!(eval_count, Some(24));
        assert_eq!(eval_duration, Some(1_200_000_000));
        assert_eq!(content, r#"{"summaryMarkdown":"streamed"}"#);
    }

    #[test]
    fn model_multiplier_rewards_larger_local_models_without_maxing_unknowns() {
        assert!(model_multiplier("oss-20b") > model_multiplier("qwen2.5-7b"));
        assert!(model_multiplier("unknown-local") < 1.0);
    }

    #[test]
    fn scoped_file_is_selected_before_generic_context() {
        let blobs = vec![
            GitTreeEntry {
                path: "README.md".to_string(),
                kind: "blob".to_string(),
                size: Some(512),
            },
            GitTreeEntry {
                path: "contracts/UniswapV2ERC20.sol".to_string(),
                kind: "blob".to_string(),
                size: Some(2_048),
            },
            GitTreeEntry {
                path: "package.json".to_string(),
                kind: "blob".to_string(),
                size: Some(512),
            },
        ];

        let selected = select_context_files(&blobs, &["contracts/UniswapV2ERC20.sol".to_string()]);

        assert_eq!(selected[0], "contracts/UniswapV2ERC20.sol");
        assert!(selected.contains(&"README.md".to_string()));
    }

    #[test]
    fn scoped_path_parser_rejects_urls_and_parent_traversal() {
        assert_eq!(
            normalize_scoped_path("`contracts/UniswapV2ERC20.sol`"),
            Some("contracts/UniswapV2ERC20.sol".to_string())
        );
        assert_eq!(normalize_scoped_path("https://github.com/x/y"), None);
        assert_eq!(normalize_scoped_path("../secret"), None);
    }
}
