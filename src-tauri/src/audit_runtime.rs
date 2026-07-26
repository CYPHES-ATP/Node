use std::time::{Duration, Instant};

use crate::events::EventSink;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    audit_labor::{
        sha256_ref, AuditFinding, AuditWorkUnit, ContributionArtifact, CoverageItem,
        NodeContribution, ProtocolAuditCampaign, RuntimeDescriptor,
    },
    audit_profile::RepositoryTarget,
    github,
};

const AUDIT_SKILL_TEXT: &str = include_str!("../../protocol/skills/cyphes-audit-skill.v0.5.md");
const MAX_TREE_FILES: usize = 20_000;
const MAX_SELECTED_FILES: usize = 16;
const MAX_FILE_BYTES: usize = 28_000;
const MAX_CONTEXT_BYTES: usize = 180_000;
/// Cloud-proxied models carry context windows two orders of magnitude larger
/// than the local tier. Feeding them the local budget was the single largest
/// source of "source not supplied in context" dead ends on the final testnet.
const CLOUD_MAX_SELECTED_FILES: usize = 48;
const CLOUD_MAX_FILE_BYTES: usize = 48_000;
const CLOUD_MAX_CONTEXT_BYTES: usize = 700_000;
/// Cap on files pulled in purely because a seed file referenced them.
const MAX_DEPENDENCY_FILES: usize = 24;
const MAX_MODEL_OUTPUT_TOKENS: u32 = 6_500;
const STRUCTURED_OUTPUT_SYSTEM_PROMPT: &str = "You are CYPHES Audit Skill. Return exactly one valid JSON object that matches the CYPHES Cognition Proof schema. Do not include prose, markdown fences, or commentary outside JSON.";
const STRUCTURED_OUTPUT_CONTRACT: &str = r#"
# Required Cognition Proof Output
Return exactly one JSON object with the fields shown below.

Field rules — read these before copying the shape:
- "severity" is exactly one of: informational, low, medium, high, critical.
  Emit a single value. Never emit a list or a joined string.
- "status" is exactly one of: candidate, non_reportable, duplicate, invalid,
  needs_review, needs_reproduction.
- "title" names the specific defect and where it lives. Never emit a generic
  label or a copy of this instruction text.
- "impact" is required for any finding at low severity or above, and must state
  who can reach the code, whether a mitigation is already present, and whether
  the bad state is reachable with realistic values. Use null only for
  informational findings.
- "evidence" entries cite a file path from the supplied context, plus a function
  or line where you have one, and must come from the file being accused.

Shape (illustration only — do not copy these values):

{
  "summaryMarkdown": "Markdown notes using the headings from the skill pack, starting with the pass objective.",
  "findings": [
    {
      "id": "CYPHES-LOCAL-001",
      "title": "withdraw() skips fee accrual when totalSupply is zero",
      "severity": "low",
      "status": "candidate",
      "impact": "Reachable by any depositor when the vault is empty. No mitigation: the early return at line 214 precedes _accrue(). Bounded to one accrual period, no fund loss.",
      "evidence": ["contracts/Vault.sol: withdraw(), line 214 - early return before _accrue()"],
      "reportable": false
    }
  ],
  "coverage": [
    {
      "area": "exploit-class matrix",
      "status": "completed",
      "evidence": ["contracts/Vault.sol: reentrancy, authorization, rounding reviewed"]
    }
  ],
  "commands": ["Traced withdraw() call path to _accrue() and compared against deposit()"]
}

Rules:
- Use an empty findings array when no vulnerability is found.
- Coverage must be non-empty and evidence-backed. Name the files and functions
  you reviewed; do not restate the area label as its own evidence.
- reportable:true requires concrete file/function/line, exploit path, impact,
  reproduction steps, and a passing Exploitability Gate.
- A no-issue result is valid only when coverage explains what was checked.
- Do not invent line numbers, tools, commands, or findings.
"#;

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

#[derive(Debug, Clone, Copy)]
struct ContextBudget {
    max_files: usize,
    max_file_bytes: usize,
    max_total_bytes: usize,
}

impl ContextBudget {
    fn for_runtime(provider: &str, model: &str) -> Self {
        if provider_class(provider, model) == "cloud-proxy" {
            Self {
                max_files: CLOUD_MAX_SELECTED_FILES,
                max_file_bytes: CLOUD_MAX_FILE_BYTES,
                max_total_bytes: CLOUD_MAX_CONTEXT_BYTES,
            }
        } else {
            Self {
                max_files: MAX_SELECTED_FILES,
                max_file_bytes: MAX_FILE_BYTES,
                max_total_bytes: MAX_CONTEXT_BYTES,
            }
        }
    }
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
    app: &EventSink,
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
    let context = repository_context(
        &client,
        campaign,
        ContextBudget::for_runtime(provider, model),
    )
    .await?;

    emit_progress(app, campaign, work_unit, "Building model prompt", 32, None);
    let prompt = build_prompt(campaign, work_unit, &context, prior_contributions);
    let input_hash = sha256_ref(prompt.as_bytes());
    let skill_hash = sha256_ref(effective_skill_text(campaign).as_bytes());

    emit_progress(app, campaign, work_unit, "Running local model", 44, None);
    let started = Instant::now();
    let mut model_output = match provider {
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
    let mut elapsed = started.elapsed();
    let mut tokens_per_second = model_output
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

    let mut parsed = parse_model_output(&model_output.content);
    let mut structured_repair_attempted = false;
    let mut structured_repair_succeeded = false;
    if parsed.parser_fallback {
        structured_repair_attempted = true;
        emit_progress(
            app,
            campaign,
            work_unit,
            "Repairing structured Cognition Proof JSON",
            80,
            tokens_per_second,
        );
        let repair_prompt = build_repair_prompt(&model_output.content);
        let repair_started = Instant::now();
        if let Ok(repair_output) = match provider {
            "lmstudio" => {
                run_openai_compatible_chat(
                    app,
                    campaign,
                    work_unit,
                    &client,
                    lmstudio_endpoint(),
                    model,
                    &repair_prompt,
                )
                .await
            }
            "ollama" => {
                run_ollama_chat(
                    app,
                    campaign,
                    work_unit,
                    &client,
                    ollama_endpoint(),
                    model,
                    &repair_prompt,
                )
                .await
            }
            _ => Err("unsupported local model provider".to_string()),
        } {
            let repaired = parse_model_output(&repair_output.content);
            if !repaired.parser_fallback {
                elapsed = started.elapsed();
                tokens_per_second = repair_output
                    .tokens_per_second
                    .or_else(|| {
                        repair_output.generated_tokens.and_then(|tokens| {
                            let seconds = repair_started.elapsed().as_secs_f64();
                            (seconds > 0.0).then_some(tokens as f64 / seconds)
                        })
                    })
                    .or_else(|| estimated_tokens_per_second(&repair_output.content, elapsed));
                model_output = repair_output;
                parsed = repaired;
                structured_repair_succeeded = true;
            }
        }
    }
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
    let provider_class = provider_class(provider, model).to_string();
    let declared_parameter_tier = declared_parameter_tier(model);
    let context_window_tokens = declared_context_window_tokens(model);
    let runtime_json = serde_json::to_vec_pretty(&json!({
        "provider": provider,
        "providerLabel": provider_label(provider),
        "endpointClass": "local",
        "providerClass": provider_class.clone(),
        "declaredParameterTier": declared_parameter_tier.clone(),
        "contextWindowTokens": context_window_tokens,
        "appVersion": env!("CARGO_PKG_VERSION"),
        "workerMode": "verifier-and-worker",
        "model": model,
        "skillHash": skill_hash,
        "inputHash": input_hash,
        "outputHash": output_hash,
        "tokensPerSecond": tokens_per_second,
        "parserFallback": parsed.parser_fallback,
        "structuredRepairAttempted": structured_repair_attempted,
        "structuredRepairSucceeded": structured_repair_succeeded,
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
        provider_class: Some(provider_class),
        declared_parameter_tier: Some(declared_parameter_tier),
        context_window_tokens,
        app_version: Some(env!("CARGO_PKG_VERSION").to_string()),
        worker_mode: Some("verifier-and-worker".to_string()),
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
    app: &EventSink,
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
    app: &EventSink,
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
            "temperature": 0.0,
            "max_tokens": MAX_MODEL_OUTPUT_TOKENS,
            "stream": true,
            "stream_options": {
                "include_usage": true
            },
            "messages": [
                {
                    "role": "system",
                    "content": STRUCTURED_OUTPUT_SYSTEM_PROMPT
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
    app: &EventSink,
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
            "format": "json",
            "options": {
                "temperature": 0.0,
                "num_predict": MAX_MODEL_OUTPUT_TOKENS
            },
            "messages": [
                {
                    "role": "system",
                    "content": STRUCTURED_OUTPUT_SYSTEM_PROMPT
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
    app: &EventSink,
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
    budget: ContextBudget,
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
    let seed_paths = select_context_files(&blobs, &scoped_paths, budget);
    let mut selected_files = Vec::new();
    let mut total_bytes = 0usize;

    for path in seed_paths {
        if total_bytes >= budget.max_total_bytes || selected_files.len() >= budget.max_files {
            break;
        }
        match fetch_raw_file(client, repository, &path).await {
            Ok(content) => {
                let content = truncate_bytes(&content, budget.max_file_bytes);
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

    // Second pass: pull in what the seed files actually reference. A finding
    // about `leveragedPosition` is unresolvable without the base contract that
    // carries its `nonReentrant` modifier, and the seed selector never had a
    // reason to fetch it.
    let dependency_paths = dependency_paths(&selected_files, &blobs, budget);
    for path in dependency_paths {
        if total_bytes >= budget.max_total_bytes || selected_files.len() >= budget.max_files {
            break;
        }
        match fetch_raw_file(client, repository, &path).await {
            Ok(content) => {
                let content = truncate_bytes(&content, budget.max_file_bytes);
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

/// Resolve files referenced by already-selected sources: Solidity/Vyper imports,
/// inherited base contracts, and `implements:` declarations. Returns repository
/// paths that are present in the tree and not already selected.
fn dependency_paths(
    selected: &[SelectedFile],
    blobs: &[GitTreeEntry],
    budget: ContextBudget,
) -> Vec<String> {
    let mut symbols: Vec<String> = Vec::new();
    for file in selected {
        collect_referenced_symbols(&file.content, &mut symbols);
    }

    let mut resolved: Vec<String> = Vec::new();
    for symbol in symbols {
        if resolved.len() >= MAX_DEPENDENCY_FILES {
            break;
        }
        let Some(path) = resolve_symbol_to_path(&symbol, blobs, budget) else {
            continue;
        };
        if selected.iter().any(|file| file.path == path) || resolved.contains(&path) {
            continue;
        }
        resolved.push(path);
    }
    resolved
}

/// Pull import targets and base-contract names out of a source file. Deliberately
/// textual: the goal is a candidate list to resolve against the tree, not a parse.
fn collect_referenced_symbols(content: &str, out: &mut Vec<String>) {
    for line in content.lines().take(600) {
        let trimmed = line.trim();

        // Solidity: import "x.sol";  import {A} from "x.sol";
        // Vyper:    from x import y
        if trimmed.starts_with("import") || trimmed.starts_with("from ") {
            if let Some(start) = trimmed.find('"').or_else(|| trimmed.find('\'')) {
                let quote = trimmed.as_bytes()[start] as char;
                if let Some(end) = trimmed[start + 1..].find(quote) {
                    let target = &trimmed[start + 1..start + 1 + end];
                    if !target.is_empty() {
                        out.push(target.to_string());
                    }
                }
            }
        }

        // Solidity: contract X is A, B {   /   abstract contract X is A {
        if let Some(index) = trimmed.find(" is ") {
            let head = &trimmed[..index];
            if head.contains("contract ") || head.contains("interface ") {
                let tail = &trimmed[index + 4..];
                for base in tail
                    .trim_end_matches('{')
                    .split(',')
                    .map(|part| part.trim().trim_end_matches('{').trim())
                {
                    let name = base.split('(').next().unwrap_or(base).trim();
                    if !name.is_empty()
                        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    {
                        out.push(name.to_string());
                    }
                }
            }
        }

        // Vyper: implements: ERC20
        if let Some(rest) = trimmed.strip_prefix("implements:") {
            let name = rest.trim();
            if !name.is_empty() {
                out.push(name.to_string());
            }
        }
    }
}

/// Map an import target or contract name onto a real path in the tree.
fn resolve_symbol_to_path(
    symbol: &str,
    blobs: &[GitTreeEntry],
    budget: ContextBudget,
) -> Option<String> {
    let symbol = symbol.trim();
    if symbol.is_empty() {
        return None;
    }
    let size_ok = |entry: &GitTreeEntry| entry.size.unwrap_or(0) <= budget.max_file_bytes as u64;

    // Import path: match on the trailing path segment chain, so both
    // "./IVault.sol" and "@openzeppelin/contracts/token/ERC20/IERC20.sol"
    // resolve against a vendored or remapped copy when one exists.
    if symbol.contains('.') {
        let needle = symbol.trim_start_matches("./").trim_start_matches('/');
        let tail = needle.rsplit('/').next().unwrap_or(needle);
        if let Some(entry) = blobs
            .iter()
            .find(|entry| {
                entry.path.ends_with(needle) && size_ok(entry) && looks_textual(&entry.path)
            })
            .or_else(|| {
                blobs.iter().find(|entry| {
                    entry
                        .path
                        .rsplit('/')
                        .next()
                        .map(|name| name == tail)
                        .unwrap_or(false)
                        && size_ok(entry)
                        && looks_textual(&entry.path)
                })
            })
        {
            return Some(entry.path.clone());
        }
        return None;
    }

    // Bare contract name: look for <Name>.sol / <Name>.vy.
    for extension in [".sol", ".vy"] {
        let file_name = format!("{symbol}{extension}");
        if let Some(entry) = blobs.iter().find(|entry| {
            entry
                .path
                .rsplit('/')
                .next()
                .map(|name| name == file_name)
                .unwrap_or(false)
                && size_ok(entry)
        }) {
            return Some(entry.path.clone());
        }
    }
    None
}

fn select_context_files(
    blobs: &[GitTreeEntry],
    scoped_paths: &[String],
    budget: ContextBudget,
) -> Vec<String> {
    // Leave headroom so the dependency pass has somewhere to put the files the
    // scoped source actually needs.
    let seed_cap = if budget.max_files > MAX_SELECTED_FILES {
        budget.max_files - MAX_DEPENDENCY_FILES.min(budget.max_files / 2)
    } else {
        budget.max_files
    };
    let mut selected = Vec::new();
    for scoped_path in scoped_paths {
        for entry in blobs {
            if selected.len() >= seed_cap {
                break;
            }
            if selected.iter().any(|path| path == &entry.path) {
                continue;
            }
            let size_ok = entry.size.unwrap_or(0) <= budget.max_file_bytes as u64;
            let in_scope =
                entry.path == *scoped_path || entry.path.starts_with(&format!("{scoped_path}/"));
            if size_ok && in_scope && looks_textual(&entry.path) {
                selected.push(entry.path.clone());
            }
        }
    }
    for entry in blobs {
        if selected.len() >= seed_cap {
            break;
        }
        let path = entry.path.as_str();
        if selected.iter().any(|selected_path| selected_path == path) {
            continue;
        }
        let size_ok = entry.size.unwrap_or(0) <= budget.max_file_bytes as u64;
        if size_ok && is_priority_context_file(path) {
            selected.push(path.to_string());
        }
    }
    if selected.len() < 8 {
        for entry in blobs {
            if selected.len() >= seed_cap {
                break;
            }
            if selected.iter().any(|path| path == &entry.path) {
                continue;
            }
            let size_ok = entry.size.unwrap_or(0) <= budget.max_file_bytes as u64;
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
        || lower == ".gitmodules"
        || lower == "foundry.lock"
        || lower == "remappings.txt"
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
            // Contract languages beyond Solidity. Omitting `vy` silently
            // starved every Curve and Yearn campaign of its scoped source.
            | "vy"
            | "cairo"
            | "move"
            | "huff"
            | "yul"
            | "circom"
            | "lock"
            | "gitmodules"
            | "rst"
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
    prompt.push_str(STRUCTURED_OUTPUT_CONTRACT);
    prompt
}

fn build_repair_prompt(previous_output: &str) -> String {
    format!(
        "{}\n\n# Previous Model Output To Repair\n```text\n{}\n```\n\nReturn only the repaired JSON object. Preserve true security meaning; do not invent findings.",
        STRUCTURED_OUTPUT_CONTRACT,
        truncate_bytes(previous_output, 24_000)
    )
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
    let commands = value
        .get("commands")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .filter(|items| !items.is_empty())
        .ok_or_else(|| "commands array is required".to_string())?;
    let findings = value
        .get("findings")
        .and_then(Value::as_array)
        .ok_or_else(|| "findings array is required".to_string())?
        .iter()
        .enumerate()
        .map(|item| value_to_finding(item, &commands))
        .collect::<Vec<_>>();
    let coverage = value
        .get("coverage")
        .and_then(Value::as_array)
        .ok_or_else(|| "coverage array is required".to_string())?
        .iter()
        .map(value_to_coverage)
        .collect::<Vec<_>>();
    if coverage.is_empty() {
        return Err("coverage array must not be empty".to_string());
    }
    if coverage.iter().all(|item| {
        item.evidence
            .iter()
            .all(|evidence| evidence.trim().is_empty())
    }) {
        return Err("coverage evidence is required".to_string());
    }
    Ok(ParsedModelOutput {
        notes_markdown,
        findings,
        coverage,
        commands,
        parser_fallback: false,
    })
}

fn value_to_finding((index, value): (usize, &Value), commands: &[String]) -> AuditFinding {
    let requested_reportable = value
        .get("reportable")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut finding = AuditFinding {
        id: string_field(value, "id").unwrap_or_else(|| format!("CYPHES-LOCAL-{:03}", index + 1)),
        title: string_field(value, "title").unwrap_or_else(|| "Untitled model lead".to_string()),
        severity: string_field(value, "severity").unwrap_or_else(|| "informational".to_string()),
        status: string_field(value, "status").unwrap_or_else(|| {
            if requested_reportable {
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
        reportable: requested_reportable,
    };
    if requested_reportable && !finding_has_bounty_grade_evidence(&finding, commands) {
        finding.reportable = false;
        if finding.status == "candidate" {
            finding.status = "needs_reproduction".to_string();
        }
    }
    finding
}

fn finding_has_bounty_grade_evidence(finding: &AuditFinding, commands: &[String]) -> bool {
    if is_placeholder_text(&finding.title) {
        return false;
    }
    let Some(impact) = finding.impact.as_deref() else {
        return false;
    };
    if is_placeholder_text(impact) || !has_impact_signal(impact) {
        return false;
    }
    if finding.evidence.is_empty()
        || finding
            .evidence
            .iter()
            .any(|evidence| is_placeholder_text(evidence))
    {
        return false;
    }
    let evidence_text = finding.evidence.join("\n");
    let combined = format!(
        "{}\n{}\n{}\n{}",
        finding.title, finding.status, impact, evidence_text
    )
    .to_ascii_lowercase();
    has_code_location_signal(&combined)
        && has_exploit_path_signal(&combined)
        && has_reproduction_signal(&combined, commands)
}

fn is_placeholder_text(value: &str) -> bool {
    let normalized = value.trim().to_ascii_lowercase();
    normalized.is_empty()
        || normalized == "vulnerability title"
        || normalized == "finding or security lead title"
        || normalized == "impact statement or null"
        || normalized == "potential security impact"
        || normalized == "short evidence-backed audit summary"
        || normalized.contains("file/function/line")
        || normalized.contains("line 123")
        || normalized.contains("contract.sol")
        || normalized.contains("artifact hash: 0x")
        // Strings lifted verbatim from the illustration in
        // STRUCTURED_OUTPUT_CONTRACT and skill pack v0.5.
        //
        // v0.4 used obviously-fake placeholders, and weak models echoed them
        // ("finding or security lead title" was the single most common finding
        // title on the network). v0.5 replaced them with a realistic example to
        // teach shape — and llama3.2:3b immediately began returning the new
        // example verbatim instead. A realistic placeholder is worse than an
        // obvious one: it looks like a genuine finding. Any model that returns
        // the example is not reporting, so treat it as placeholder text.
        || normalized.contains("withdraw() skips fee accrual")
        || normalized.contains("early return before _accrue")
        || normalized.contains("traced withdraw() call path")
        || normalized.contains("markdown notes using the headings")
}

fn has_code_location_signal(value: &str) -> bool {
    let has_file = [
        ".sol", ".vy", ".rs", ".go", ".ts", ".tsx", ".js", ".jsx", ".py", ".yml", ".yaml", ".json",
    ]
    .iter()
    .any(|marker| value.contains(marker));
    let has_function = value.contains("function")
        || value.contains(" fn ")
        || value.contains("contract ")
        || value.contains("::")
        || value.contains("()");
    has_file && has_function && contains_line_marker(value)
}

fn contains_line_marker(value: &str) -> bool {
    value.contains("line ")
        || value
            .as_bytes()
            .windows(2)
            .any(|window| window[0] == b':' && window[1].is_ascii_digit())
}

fn has_exploit_path_signal(value: &str) -> bool {
    [
        "exploit",
        "attack",
        "drain",
        "steal",
        "loss of funds",
        "unauthorized",
        "bypass",
        "reentr",
        "overflow",
        "underflow",
        "dos",
        "denial",
        "manipulat",
        "replay",
        "double-spend",
        "incorrect accounting",
    ]
    .iter()
    .any(|marker| value.contains(marker))
}

fn has_impact_signal(value: &str) -> bool {
    let normalized = value.to_ascii_lowercase();
    normalized.len() >= 24
        && [
            "loss",
            "fund",
            "drain",
            "steal",
            "unauthorized",
            "bypass",
            "governance",
            "liquidat",
            "insolv",
            "denial",
            "dos",
            "incorrect accounting",
            "price",
            "oracle",
            "mint",
            "burn",
        ]
        .iter()
        .any(|marker| normalized.contains(marker))
}

fn has_reproduction_signal(value: &str, commands: &[String]) -> bool {
    let command_text = commands.join("\n").to_ascii_lowercase();
    let combined = format!("{value}\n{command_text}");
    [
        "reproduce",
        "reproduction",
        "repro steps",
        "steps:",
        "poc",
        "proof of concept",
        "forge test",
        "hardhat test",
        "foundry test",
        "echidna",
        "foundry",
        "assert",
        "transaction sequence",
        "call sequence",
    ]
    .iter()
    .any(|marker| combined.contains(marker))
        && !command_text.contains("no repository code execution")
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

fn provider_class(provider: &str, model: &str) -> &'static str {
    let lower = model.to_ascii_lowercase();
    if lower.contains(":cloud")
        || lower.contains("cloud")
        || lower.contains("api")
        || lower.contains("proxy")
        || lower.contains("qwen-max")
        || lower.contains("minimax")
        || lower.contains("kimi")
    {
        "cloud-proxy"
    } else if provider == "ollama" {
        "local"
    } else {
        "local-compatible"
    }
}

fn declared_parameter_tier(model: &str) -> String {
    let lower = model.to_ascii_lowercase();
    for marker in [
        "405b", "120b", "72b", "70b", "34b", "32b", "24b", "22b", "20b", "14b", "13b", "8b", "7b",
        "3b", "1b",
    ] {
        if lower.contains(marker) {
            return marker.to_string();
        }
    }
    if lower.contains("minimax-m3") || lower.contains("qwen-max") || lower.contains("kimi") {
        "frontier-cloud".to_string()
    } else if lower.contains("frontier") {
        "frontier".to_string()
    } else {
        "unknown".to_string()
    }
}

fn declared_context_window_tokens(model: &str) -> Option<u32> {
    let lower = model.to_ascii_lowercase();
    if lower.contains("1m") {
        Some(1_000_000)
    } else if lower.contains("256k") {
        Some(256_000)
    } else if lower.contains("128k") {
        Some(128_000)
    } else if lower.contains("64k") {
        Some(64_000)
    } else if lower.contains("32k") {
        Some(32_000)
    } else {
        None
    }
}

/// Ordered model-tier table. First match wins, so more specific patterns must
/// come first. Replaces a hand-rolled `if/else` cascade that silently paid every
/// GLM release the 0.9 unknown-model floor — the same rate as a 3B local model —
/// because no branch happened to mention "glm".
///
/// Matching is on an operator-controlled string and is therefore a
/// classification, not a proof. See `crate::audit_labor::throughput_gated_multiplier`
/// for the check that stops a renamed local model from claiming the top rate.
const MODEL_TIERS: &[(&str, f64)] = &[
    // Reserved top tier. Matched on the exact family so the bare "kimi" pattern
    // below cannot reach it.
    ("kimi-k3", 50.0),
    // Earned tier. glm-5.2 is the best-measured model ever run on this network:
    // 3.75 findings per pass at 53% unique titles and 81 tok/s across 102 passes,
    // and it carried every pass it ever ran past the coverage-quality gate
    // (102/102 with three or more evidence-backed coverage items). Matched on the
    // exact minor version so a future glm-5.3 must earn the tier on its own data
    // rather than inheriting it.
    ("glm-5.2", 20.0),
    // Frontier-class, cloud-served.
    ("minimax-m3", 10.0),
    ("gpt-oss-120b", 10.0),
    ("qwen-max", 10.0),
    ("qwen3-max", 10.0),
    ("kimi", 10.0),
    ("claude", 10.0),
    ("gpt-5", 10.0),
    ("gpt-4", 10.0),
    ("gemini", 10.0),
    ("deepseek-r1", 10.0),
    ("deepseek-v3", 10.0),
    ("glm-5", 10.0),
    ("glm-4", 10.0),
    ("glm", 10.0),
    ("llama-4", 10.0),
    ("mistral-large", 10.0),
    ("frontier", 10.0),
    ("405b", 10.0),
    ("120b", 10.0),
    // Large local.
    ("gpt-oss-20b", 3.0),
    ("70b", 3.0),
    ("72b", 3.0),
    ("32b", 2.5),
    ("34b", 2.5),
    ("20b", 2.0),
    ("22b", 2.0),
    ("24b", 2.0),
    ("14b", 1.6),
    ("13b", 1.6),
    ("7b", 1.0),
    ("8b", 1.0),
];

/// Highest multiplier a model may earn without its cloud claim surviving the
/// throughput gate. Large *local* models are legitimately slow, so throughput
/// cannot discriminate below this line.
pub const UNVERIFIED_CLOUD_MULTIPLIER_CEILING: f64 = 3.0;
pub const UNKNOWN_MODEL_MULTIPLIER: f64 = 0.9;

fn model_multiplier(model: &str) -> f64 {
    let lower = model.to_ascii_lowercase();
    MODEL_TIERS
        .iter()
        .find(|(pattern, _)| lower.contains(pattern))
        .map(|(_, multiplier)| *multiplier)
        .unwrap_or(UNKNOWN_MODEL_MULTIPLIER)
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
    fn bounty_gate_downgrades_placeholder_reportable_findings() {
        let output = parse_model_output(
            r#"{
              "summaryMarkdown": "Audit Summary",
              "findings": [{
                "id":"X",
                "title":"Vulnerability Title",
                "severity":"critical",
                "status":"candidate",
                "impact":"Potential Security Impact",
                "evidence":["File: contract.sol, Line: 123, Artifact Hash: 0x..."],
                "reportable":true
              }],
              "coverage": [{"area":"scope","status":"completed","evidence":["README.md reviewed"]}],
              "commands": ["no repository code execution"]
            }"#,
        );
        assert_eq!(output.findings.len(), 1);
        assert!(!output.findings[0].reportable);
        assert_eq!(output.findings[0].status, "needs_reproduction");
    }

    #[test]
    fn bounty_gate_preserves_reproducible_concrete_findings() {
        let output = parse_model_output(
            r#"{
              "summaryMarkdown": "Reproduced a concrete withdrawal ordering issue.",
              "findings": [{
                "id":"X",
                "title":"Reentrant withdrawal before accounting update",
                "severity":"high",
                "status":"candidate",
                "impact":"An attacker can reenter withdraw and cause loss of funds before balance accounting is reduced.",
                "evidence":["contracts/Vault.sol:128 function withdraw() transfers before balance update; exploit path: callback reenters withdraw; reproduction: forge test --match-test testReentrantWithdraw"],
                "reportable":true
              }],
              "coverage": [{"area":"withdraw accounting","status":"completed","evidence":["contracts/Vault.sol:128"]}],
              "commands": ["forge test --match-test testReentrantWithdraw"]
            }"#,
        );
        assert_eq!(output.findings.len(), 1);
        assert!(output.findings[0].reportable);
        assert_eq!(output.findings[0].status, "candidate");
    }

    #[test]
    fn bounty_gate_requires_reproduction_not_generic_run_language() {
        let output = parse_model_output(
            r#"{
              "summaryMarkdown": "Specific looking but unreproduced issue.",
              "findings": [{
                "id":"X",
                "title":"Swap math can be bypassed",
                "severity":"high",
                "status":"candidate",
                "impact":"An attacker can bypass accounting and cause loss of funds.",
                "evidence":["contracts/Pool.sol:91 function swap() has unchecked accounting; exploit path: attacker bypasses invariant"],
                "reportable":true
              }],
              "coverage": [{"area":"swap accounting","status":"completed","evidence":["contracts/Pool.sol:91"]}],
              "commands": ["read repository context and run local reasoning"]
            }"#,
        );
        assert!(!output.findings[0].reportable);
        assert_eq!(output.findings[0].status, "needs_reproduction");
    }

    #[test]
    fn unstructured_model_output_does_not_create_findings() {
        let output = parse_model_output("I looked around and it seems okay.");
        assert!(output.findings.is_empty());
        assert_eq!(output.coverage[0].status, "needs_review");
        assert!(output.parser_fallback);
    }

    #[test]
    fn summary_only_json_is_parser_fallback() {
        let output = parse_model_output(r#"{"summaryMarkdown":"Looks fine."}"#);
        assert!(output.parser_fallback);
        assert_eq!(output.coverage[0].status, "needs_review");
    }

    #[test]
    fn repair_prompt_contains_cognition_proof_contract() {
        let prompt = build_repair_prompt("plain prose");
        assert!(prompt.contains("Required Cognition Proof Output"));
        assert!(prompt.contains("Return only the repaired JSON object"));
        assert!(prompt.contains("coverage"));
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
        assert_eq!(model_multiplier("minimax-m3:cloud"), 10.0);
        assert_eq!(model_multiplier("gpt-oss-120b"), 10.0);
        assert_eq!(model_multiplier("kimi-k2"), 10.0);
        assert_eq!(model_multiplier("qwen-max"), 10.0);
        assert_eq!(model_multiplier("gpt-oss-20b"), 3.0);
        assert_eq!(model_multiplier("llama-3.3-70b"), 3.0);
        assert!(model_multiplier("qwen2.5-32b") > model_multiplier("qwen2.5-14b"));
        assert!(model_multiplier("oss-20b") > model_multiplier("qwen2.5-7b"));
        assert!(model_multiplier("unknown-local") < 1.0);
    }

    #[test]
    fn the_contract_example_is_rejected_when_a_model_echoes_it() {
        // Observed live on v0.17.4: llama3.2:3b returned the v0.5 illustration
        // verbatim as its finding. A realistic example is more dangerous than an
        // obvious one because the echo looks like real work.
        assert!(is_placeholder_text(
            "withdraw() skips fee accrual when totalSupply is zero"
        ));
        assert!(is_placeholder_text(
            "contracts/Vault.sol: withdraw(), line 214 - early return before _accrue()"
        ));
        assert!(is_placeholder_text(
            "Traced withdraw() call path to _accrue() and compared against deposit()"
        ));
        // A genuine finding that merely mentions withdraw() must still pass.
        assert!(!is_placeholder_text(
            "actionKick silently skips RPL bond refund when vault has insufficient balance"
        ));
    }

    #[test]
    fn glm_5_2_holds_its_earned_tier_without_widening_it() {
        assert_eq!(model_multiplier("glm-5.2:cloud"), 20.0);
        // Siblings must not inherit the earned tier.
        assert_eq!(model_multiplier("glm-5.1:cloud"), 10.0);
        assert_eq!(model_multiplier("glm-5.3:cloud"), 10.0);
        assert_eq!(model_multiplier("glm-4.6"), 10.0);
        // Above the unverified ceiling, so a relabelled local model claiming it
        // still has to survive the throughput gate.
        assert!(model_multiplier("glm-5.2:cloud") > UNVERIFIED_CLOUD_MULTIPLIER_CEILING);
    }

    #[test]
    fn glm_is_tiered_as_frontier_not_as_an_unknown_model() {
        // Regression: no branch matched "glm", so the two highest-quality models
        // on the final testnet were paid the 0.9 unknown-model floor — the same
        // rate as llama3.2:3b, which produced 1% unique finding titles.
        // glm-5.2 carries its own earned tier; see the test above.
        assert!(model_multiplier("glm-5.2:cloud") >= 10.0);
        assert_eq!(model_multiplier("glm-5.1:cloud"), 10.0);
        assert_eq!(model_multiplier("glm-4.6"), 10.0);
        assert!(model_multiplier("glm-5.2:cloud") > model_multiplier("llama3.2:3b"));
        assert_eq!(model_multiplier("llama3.2:3b"), UNKNOWN_MODEL_MULTIPLIER);
    }

    #[test]
    fn specific_tier_patterns_win_over_generic_size_suffixes() {
        // "gpt-oss-120b" must not fall through to the generic "20b" rule.
        assert_eq!(model_multiplier("gpt-oss-120b"), 10.0);
        assert_eq!(model_multiplier("gpt-oss-20b"), 3.0);
    }

    #[test]
    fn kimi_k3_holds_the_reserved_top_tier_without_widening_it() {
        assert_eq!(model_multiplier("kimi-k3"), 50.0);
        // The bare "kimi" pattern must not reach the reserved tier, or every
        // past and future Kimi release inherits 50x by accident.
        assert_eq!(model_multiplier("kimi-k2"), 10.0);
        assert_eq!(model_multiplier("moonshot-kimi"), 10.0);
    }

    #[test]
    fn the_reserved_top_tier_is_still_subject_to_the_throughput_gate() {
        // 50x makes name-based classification the highest-value target in the
        // system. It must not be reachable without a surviving cloud claim.
        assert!(model_multiplier("kimi-k3") > UNVERIFIED_CLOUD_MULTIPLIER_CEILING);
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

        let selected = select_context_files(
            &blobs,
            &["contracts/UniswapV2ERC20.sol".to_string()],
            ContextBudget::for_runtime("ollama", "llama3.2:3b"),
        );

        assert_eq!(selected[0], "contracts/UniswapV2ERC20.sol");
        assert!(selected.contains(&"README.md".to_string()));
    }

    #[test]
    fn cloud_runtime_gets_a_larger_context_budget_than_local() {
        let local = ContextBudget::for_runtime("ollama", "llama3.2:3b");
        let cloud = ContextBudget::for_runtime("ollama", "glm-5.2:cloud");

        assert_eq!(local.max_files, MAX_SELECTED_FILES);
        assert_eq!(local.max_total_bytes, MAX_CONTEXT_BYTES);
        assert!(cloud.max_files > local.max_files);
        assert!(cloud.max_total_bytes > local.max_total_bytes);
    }

    #[test]
    fn vyper_and_contract_sources_are_selectable() {
        // Omitting `.vy` starved every Curve and Yearn campaign of its scoped source.
        assert!(looks_textual("contracts/pools/3pool/StableSwap3Pool.vy"));
        assert!(looks_textual("contracts/Vault.vy"));
        assert!(looks_textual("src/Comet.sol"));
        assert!(looks_textual(".gitmodules"));
        assert!(!looks_textual("assets/logo.png"));
    }

    #[test]
    fn referenced_symbols_cover_imports_and_base_contracts() {
        let source = r#"
pragma solidity 0.8.17;
import "@openzeppelin/contracts/security/ReentrancyGuard.sol";
import { IVault } from "./interfaces/IVault.sol";

abstract contract FraxlendPairCore is FraxlendPairAccessControl, ERC20, ReentrancyGuard {
    function leveragedPosition() external nonReentrant {}
}
"#;
        let mut symbols = Vec::new();
        collect_referenced_symbols(source, &mut symbols);

        assert!(symbols.iter().any(|s| s.ends_with("ReentrancyGuard.sol")));
        assert!(symbols.iter().any(|s| s.ends_with("IVault.sol")));
        assert!(symbols.iter().any(|s| s == "FraxlendPairAccessControl"));
        assert!(symbols.iter().any(|s| s == "ERC20"));
    }

    #[test]
    fn dependency_resolution_pulls_in_the_base_contract() {
        let blobs = vec![
            GitTreeEntry {
                path: "src/contracts/FraxlendPair.sol".to_string(),
                kind: "blob".to_string(),
                size: Some(4_096),
            },
            GitTreeEntry {
                path: "src/contracts/FraxlendPairCore.sol".to_string(),
                kind: "blob".to_string(),
                size: Some(8_192),
            },
            GitTreeEntry {
                path: "lib/openzeppelin/contracts/security/ReentrancyGuard.sol".to_string(),
                kind: "blob".to_string(),
                size: Some(1_024),
            },
        ];
        let selected = vec![SelectedFile {
            path: "src/contracts/FraxlendPair.sol".to_string(),
            content: r#"
import "@openzeppelin/contracts/security/ReentrancyGuard.sol";
contract FraxlendPair is FraxlendPairCore {
}
"#
            .to_string(),
        }];

        let deps = dependency_paths(
            &selected,
            &blobs,
            ContextBudget::for_runtime("ollama", "glm-5.2:cloud"),
        );

        // Both the base contract carrying `nonReentrant` and the guard itself.
        assert!(deps.contains(&"src/contracts/FraxlendPairCore.sol".to_string()));
        assert!(
            deps.contains(&"lib/openzeppelin/contracts/security/ReentrancyGuard.sol".to_string())
        );
        // Never re-fetch what is already in context.
        assert!(!deps.contains(&"src/contracts/FraxlendPair.sol".to_string()));
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
