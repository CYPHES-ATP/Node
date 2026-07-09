use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::mpsc;

use crate::{
    atp::{
        agent_id, create_signed_envelope, create_signed_envelope_with_expiry, now_rfc3339, AtpVerb,
    },
    audit_labor::{
        signed_autonomous_finality_verification, signed_contribution_for_work_unit,
        signed_work_unit_claim, AuditFinding, AuditWorkUnit, AuditWorkUnitClaim,
        CampaignAttachment, CampaignReportSnapshot, ContributionArtifact, CoverageItem,
        CreditSummary, NodeContribution, ProtocolAuditCampaign, RuntimeDescriptor,
        VerificationEvidence,
    },
    audit_profile::{is_git_commit_sha, AuditContract, ReceiptApproval, RepositoryTarget},
    audit_runtime::{
        list_local_models, local_model_providers, run_local_audit_skill, LocalModelList,
    },
    bundle::export_campaign_report_bundle,
    github::{self, GitHubAccessStatus},
    p2p::{load_or_create_identity, spawn_swarm, SwarmCommand, ATP_PROTOCOL},
    state::{P2pState, PeerInfo},
    store::{
        campaign_id_for_transaction, data_dir, millis_from_rfc3339, now_millis, AtpStore,
        AuditEventBody, AuditJob, AuditJobPayload, LegacyAuditJob, RepositorySummary,
        MAX_PENDING_CONTRIBUTIONS_PER_WORKER,
    },
    worker::{create_repository_leases, execute_pipeline_audit_result, execute_repository_audit},
};

const GITHUB_REPOSITORY_URL_ERROR: &str =
    "Use a public GitHub repository URL, file URL, or folder URL, for example https://github.com/owner/repo.";
const MAX_SELF_PENDING_CONTRIBUTIONS: usize = MAX_PENDING_CONTRIBUTIONS_PER_WORKER;
// Phase 1 fair-work policy. A work unit is not claimable until it has been open
// for this long, giving peers time to sync it before anyone can claim, so the
// seeder cannot win its own units in the broadcast gap. Combined with the
// no-self-dealing rule, this removes the latency/self-seed advantage that let
// one node take ~90% of the work. Client/command-layer policy for the honest
// autonomous loop; real enforcement moves on-chain in the staking phase.
pub const WORK_UNIT_CLAIMABLE_AFTER_MS: u64 = 60_000;

/// Fair-work guard: a node may not work a campaign it seeded, and a unit is
/// only claimable after its broadcast window elapses.
fn ensure_fair_work_claim(
    requester_agent_id: &str,
    worker_agent_id: &str,
    unit_created_at_ms: u64,
    now_ms: u64,
) -> Result<(), String> {
    if requester_agent_id == worker_agent_id {
        return Err(
            "Fair-work policy: a node cannot claim or run work from a campaign it seeded."
                .to_string(),
        );
    }
    let claimable_at = unit_created_at_ms.saturating_add(WORK_UNIT_CLAIMABLE_AFTER_MS);
    if now_ms < claimable_at {
        let wait_secs = claimable_at.saturating_sub(now_ms).div_ceil(1000);
        return Err(format!(
            "Fair-work policy: work unit is in its {}s broadcast window; claimable in ~{wait_secs}s.",
            WORK_UNIT_CLAIMABLE_AFTER_MS / 1000
        ));
    }
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct StartNodeResponse {
    pub peer_id: String,
    pub agent_id: String,
    pub protocol: String,
    pub listen_addrs: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct MigrationResult {
    pub migrated: usize,
    pub skipped: usize,
}

#[derive(Debug, Serialize)]
pub struct NetworkInfo {
    pub peer_id: String,
    pub agent_id: String,
    pub protocol: String,
    pub listen_addrs: Vec<String>,
    pub relay_configured: bool,
    pub relay_connected: bool,
    pub rendezvous_registered: bool,
    pub bootstrap_source: Option<String>,
    pub connected_peers: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolCampaignRequest {
    pub protocol_name: String,
    pub repository: RepositorySummary,
    pub scope_text: String,
    pub bounty_url: Option<String>,
    pub impacts_in_scope: Vec<String>,
    pub out_of_scope: Vec<String>,
    pub audit_brief_text: Option<String>,
    pub attachment_text: Option<String>,
    pub custom_skill_text: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportedReportBundle {
    pub campaign_id: String,
    pub bundle_path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectedRepository {
    pub repository: RepositorySummary,
    pub focus_path: Option<String>,
    pub focus_ref: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GitHubRepositoryResponse {
    full_name: String,
    description: Option<String>,
    language: Option<String>,
    default_branch: String,
    stargazers_count: u64,
    private: bool,
}

#[derive(Debug, Deserialize)]
struct GitHubCommitResponse {
    sha: String,
}

#[derive(Debug)]
struct GitHubInputTarget {
    api_url: String,
    kind: GitHubInputKind,
    path_segments: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
enum GitHubInputKind {
    Repository,
    Blob,
    Tree,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GuardianTargetIndex {
    targets: Vec<GuardianTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardianTarget {
    pub target_id: String,
    pub protocol_name: String,
    #[serde(default)]
    pub source: Vec<String>,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub chains: Vec<String>,
    #[serde(default)]
    pub tvl_risk_rank: u32,
    pub repo_url: String,
    #[serde(default)]
    pub repo_urls: Vec<String>,
    #[serde(default)]
    pub contract_paths: Vec<String>,
    pub docs_url: Option<String>,
    pub security_url: Option<String>,
    pub in_scope_text: Option<String>,
    pub out_of_scope_text: Option<String>,
    pub last_audited_commit: Option<String>,
    pub last_observed_commit: Option<String>,
    #[serde(default)]
    pub contract_criticality: u32,
    #[serde(default)]
    pub priority_score: u32,
    pub scope_text: String,
    pub audit_brief: String,
    pub credit_budget: u32,
    pub cadence: String,
    pub tags: Vec<String>,
}

#[tauri::command]
pub async fn list_guardian_targets() -> Result<Vec<GuardianTarget>, String> {
    let index: GuardianTargetIndex = serde_json::from_str(include_str!(
        "../../protocol/targets/guardian-target-index.json"
    ))
    .map_err(|error| format!("invalid guardian target index: {error}"))?;
    Ok(index.targets)
}

#[tauri::command]
pub async fn list_local_model_providers() -> Result<Vec<LocalModelList>, String> {
    Ok(local_model_providers())
}

#[tauri::command]
pub async fn list_local_model_models(provider: String) -> Result<LocalModelList, String> {
    Ok(list_local_models(&provider).await)
}

#[tauri::command]
pub fn get_github_access_status() -> Result<GitHubAccessStatus, String> {
    Ok(github::github_access_status())
}

#[tauri::command]
pub async fn inspect_github_repository(url: String) -> Result<InspectedRepository, String> {
    let target = parse_github_input(&url)?;
    let client = github::client()?;
    let repository = github::get_json::<GitHubRepositoryResponse>(&client, &target.api_url)
        .await
        .map_err(|error| format!("GitHub repository read failed. {error}"))?;
    if repository.private {
        return Err("Private repositories are not supported in this build.".to_string());
    }
    let (commit_sha, focus_path, focus_ref) =
        resolve_github_path(&client, &target.api_url, &repository, &target).await?;
    if !is_git_commit_sha(&commit_sha) {
        return Err("GitHub did not return an exact commit SHA for this campaign.".to_string());
    }
    Ok(InspectedRepository {
        repository: RepositorySummary {
            url: format!("https://github.com/{}", repository.full_name),
            full_name: repository.full_name,
            description: repository.description,
            language: repository.language,
            default_branch: repository.default_branch,
            stars: repository.stargazers_count,
            is_private: repository.private,
            commit_sha,
        },
        focus_path,
        focus_ref,
    })
}

#[tauri::command]
pub async fn start_node(
    app: AppHandle,
    state: State<'_, P2pState>,
    store: State<'_, AtpStore>,
) -> Result<StartNodeResponse, String> {
    {
        let inner = state.inner.lock().map_err(|error| error.to_string())?;
        if inner.started {
            let keypair = inner
                .keypair
                .as_ref()
                .ok_or_else(|| "P2P identity missing".to_string())?;
            return Ok(StartNodeResponse {
                peer_id: inner.local_peer_id.clone().unwrap_or_default(),
                agent_id: agent_id(&keypair.public()),
                protocol: ATP_PROTOCOL.to_string(),
                listen_addrs: inner.listen_addrs.clone(),
            });
        }
    }

    let keypair = load_or_create_identity()?;
    let agent = agent_id(&keypair.public());
    let (tx, rx) = mpsc::unbounded_channel();
    let (peer_id, listen_addrs) = spawn_swarm(
        app,
        state.inner().clone(),
        store.inner().clone(),
        keypair.clone(),
        rx,
    )
    .await?;

    let mut inner = state.inner.lock().map_err(|error| error.to_string())?;
    inner.started = true;
    inner.local_peer_id = Some(peer_id.clone());
    inner.keypair = Some(keypair);
    inner.sender = Some(tx);
    inner.listen_addrs = listen_addrs.clone();

    Ok(StartNodeResponse {
        peer_id,
        agent_id: agent,
        protocol: ATP_PROTOCOL.to_string(),
        listen_addrs,
    })
}

#[tauri::command]
pub async fn get_network_info(state: State<'_, P2pState>) -> Result<NetworkInfo, String> {
    let inner = state.inner.lock().map_err(|error| error.to_string())?;
    let keypair = inner
        .keypair
        .as_ref()
        .ok_or_else(|| "P2P node has not started".to_string())?;
    Ok(NetworkInfo {
        peer_id: inner.local_peer_id.clone().unwrap_or_default(),
        agent_id: agent_id(&keypair.public()),
        protocol: ATP_PROTOCOL.to_string(),
        listen_addrs: inner.listen_addrs.clone(),
        relay_configured: inner.relay_configured,
        relay_connected: inner.relay_connected,
        rendezvous_registered: inner.rendezvous_registered,
        bootstrap_source: inner.bootstrap_source.clone(),
        connected_peers: inner.peers.len(),
    })
}

#[tauri::command]
pub async fn connect_peer(state: State<'_, P2pState>, address: String) -> Result<(), String> {
    let address = address
        .parse::<libp2p::Multiaddr>()
        .map_err(|error| format!("invalid libp2p multiaddress: {error}"))?;
    let (_, sender) = node_runtime(&state)?;
    sender
        .send(SwarmCommand::Dial(address))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn list_audits(store: State<'_, AtpStore>) -> Result<Vec<AuditJob>, String> {
    store.list_jobs()
}

#[tauri::command]
pub async fn list_protocol_campaigns(
    store: State<'_, AtpStore>,
) -> Result<Vec<ProtocolAuditCampaign>, String> {
    store.list_protocol_campaigns()
}

#[tauri::command]
pub async fn get_campaign_snapshot(
    store: State<'_, AtpStore>,
    campaign_id: String,
) -> Result<CampaignReportSnapshot, String> {
    store.campaign_report_snapshot(&campaign_id)
}

#[tauri::command]
pub async fn get_campaign_live_snapshot(
    store: State<'_, AtpStore>,
    campaign_id: String,
) -> Result<CampaignReportSnapshot, String> {
    store.campaign_live_snapshot(&campaign_id)
}

#[tauri::command]
pub async fn create_protocol_campaign(
    app: AppHandle,
    state: State<'_, P2pState>,
    store: State<'_, AtpStore>,
    request: ProtocolCampaignRequest,
) -> Result<ProtocolAuditCampaign, String> {
    if request.repository.is_private {
        return Err("Private repositories are not supported".to_string());
    }
    if !is_git_commit_sha(&request.repository.commit_sha) {
        return Err("Protocol audit campaigns must pin an exact Git commit SHA".to_string());
    }
    let (keypair, sender) = node_runtime(&state)?;
    let mut attachments = Vec::new();
    if let Some(text) = request
        .attachment_text
        .as_ref()
        .filter(|text| !text.trim().is_empty())
    {
        attachments.push(CampaignAttachment::from_text(
            "Requester attachment".to_string(),
            text.clone(),
        )?);
    }
    let campaign = ProtocolAuditCampaign::new(
        request.protocol_name,
        RepositoryTarget {
            full_name: request.repository.full_name,
            url: request.repository.url,
            commit_sha: request.repository.commit_sha,
        },
        request.scope_text,
        request.bounty_url,
        request.impacts_in_scope,
        request.out_of_scope,
        request.audit_brief_text,
        None,
        attachments,
        request.custom_skill_text,
        agent_id(&keypair.public()),
    )?;
    let campaign = store.create_protocol_campaign(&campaign)?;
    sender
        .send(SwarmCommand::SendCampaign(campaign.clone()))
        .map_err(|error| error.to_string())?;
    let _ = app.emit("audit:labor_changed", ());
    Ok(campaign)
}

#[tauri::command]
pub async fn record_campaign_contribution(
    app: AppHandle,
    state: State<'_, P2pState>,
    store: State<'_, AtpStore>,
    campaign_id: String,
    work_unit_id: String,
    notes_markdown: String,
) -> Result<NodeContribution, String> {
    let (keypair, sender) = node_runtime(&state)?;
    let snapshot = store.campaign_report_snapshot(&campaign_id)?;
    let campaign = snapshot.campaign.clone();
    let work_unit = snapshot
        .work_units
        .iter()
        .find(|unit| unit.work_unit_id == work_unit_id)
        .cloned()
        .ok_or_else(|| "Campaign work unit not found".to_string())?;
    let claim = ensure_work_unit_claim(&store, &keypair, &campaign, &work_unit, &snapshot.claims)?;
    let note = if notes_markdown.trim().is_empty() {
        "Manual coverage contribution. Use Run Audit Pipeline with LM Studio or Ollama for local model execution.".to_string()
    } else {
        notes_markdown
    };
    let artifact_bytes = note.as_bytes();
    let artifact = ContributionArtifact {
        path: "notes.md".to_string(),
        media_type: "text/markdown".to_string(),
        sha256: crate::audit_labor::sha256_ref(artifact_bytes),
        size_bytes: artifact_bytes.len() as u64,
    };
    let contribution = signed_contribution_for_work_unit(
        &keypair,
        &campaign,
        &work_unit,
        RuntimeDescriptor::deterministic_fixture(),
        note,
        vec![AuditFinding {
            id: "CYPHES-COVERAGE-001".to_string(),
            title: "Manual coverage-only output".to_string(),
            severity: "informational".to_string(),
            status: "non_reportable".to_string(),
            impact: None,
            evidence: vec![
                "This manual coverage path records notes; it does not claim an exploit."
                    .to_string(),
            ],
            reportable: false,
        }],
        vec![artifact],
        vec![CoverageItem {
            area: "scope mapping".to_string(),
            status: "completed".to_string(),
            evidence: vec![
                "Repository, pinned commit, scope, and runtime policy recorded.".to_string(),
            ],
        }],
        vec!["CYPHES deterministic fixture: no repository code execution".to_string()],
    )?;
    let contribution = store.record_contribution(&contribution)?;
    sender
        .send(SwarmCommand::SendWorkUnitClaim {
            claim,
            audience: campaign.requester_agent_id.clone(),
        })
        .map_err(|error| error.to_string())?;
    sender
        .send(SwarmCommand::SendContribution {
            contribution: contribution.clone(),
            audience: campaign.requester_agent_id,
        })
        .map_err(|error| error.to_string())?;
    let _ = app.emit("audit:labor_changed", ());
    Ok(contribution)
}

#[tauri::command]
pub async fn run_campaign_audit_skill(
    app: AppHandle,
    state: State<'_, P2pState>,
    store: State<'_, AtpStore>,
    campaign_id: String,
    work_unit_id: String,
    provider: String,
    model: String,
) -> Result<NodeContribution, String> {
    let (keypair, sender) = node_runtime(&state)?;
    let snapshot = store.campaign_report_snapshot(&campaign_id)?;
    let campaign = snapshot.campaign.clone();
    let work_unit = snapshot
        .work_units
        .iter()
        .find(|unit| unit.work_unit_id == work_unit_id)
        .cloned()
        .ok_or_else(|| "Campaign work unit not found".to_string())?;
    let claim = ensure_work_unit_claim(&store, &keypair, &campaign, &work_unit, &snapshot.claims)?;
    let prior_contributions = snapshot.contributions;
    let output = run_local_audit_skill(
        &app,
        &campaign,
        &work_unit,
        &provider,
        &model,
        &prior_contributions,
    )
    .await?;
    let contribution = signed_contribution_for_work_unit(
        &keypair,
        &campaign,
        &work_unit,
        output.runtime,
        output.notes_markdown,
        output.findings,
        output.artifacts,
        output.coverage,
        output.commands,
    )?;
    let contribution = store.record_contribution(&contribution)?;
    sender
        .send(SwarmCommand::SendWorkUnitClaim {
            claim,
            audience: campaign.requester_agent_id.clone(),
        })
        .map_err(|error| error.to_string())?;
    sender
        .send(SwarmCommand::SendContribution {
            contribution: contribution.clone(),
            audience: campaign.requester_agent_id,
        })
        .map_err(|error| error.to_string())?;
    let _ = app.emit("audit:labor_changed", ());
    Ok(contribution)
}

#[tauri::command]
pub async fn claim_campaign_work_unit(
    app: AppHandle,
    state: State<'_, P2pState>,
    store: State<'_, AtpStore>,
    campaign_id: String,
    work_unit_id: String,
) -> Result<AuditWorkUnitClaim, String> {
    let (keypair, sender) = node_runtime(&state)?;
    let worker_agent_id = agent_id(&keypair.public());
    ensure_verification_pool_clear(&store, &worker_agent_id)?;
    let snapshot = store.campaign_report_snapshot(&campaign_id)?;
    let work_unit = snapshot
        .work_units
        .iter()
        .find(|unit| unit.work_unit_id == work_unit_id)
        .cloned()
        .ok_or_else(|| "Campaign work unit not found".to_string())?;
    if work_unit.status != "open" {
        return Err("Only open work units can be claimed.".to_string());
    }
    ensure_fair_work_claim(
        &snapshot.campaign.requester_agent_id,
        &worker_agent_id,
        millis_from_rfc3339(&work_unit.created_at)?.max(0) as u64,
        now_millis(),
    )?;
    let claim = signed_work_unit_claim(&keypair, &snapshot.campaign, &work_unit)?;
    let claim = store.record_work_unit_claim(&claim)?;
    sender
        .send(SwarmCommand::SendWorkUnitClaim {
            claim: claim.clone(),
            audience: snapshot.campaign.requester_agent_id,
        })
        .map_err(|error| error.to_string())?;
    let _ = app.emit("audit:labor_changed", ());
    Ok(claim)
}

#[tauri::command]
pub async fn run_claimed_work_unit(
    app: AppHandle,
    state: State<'_, P2pState>,
    store: State<'_, AtpStore>,
    campaign_id: String,
    work_unit_id: String,
    provider: String,
    model: String,
    max_runtime_seconds: Option<u64>,
) -> Result<NodeContribution, String> {
    let (keypair, sender) = node_runtime(&state)?;
    let worker_agent_id = agent_id(&keypair.public());
    ensure_verification_pool_clear(&store, &worker_agent_id)?;
    let snapshot = store.campaign_report_snapshot(&campaign_id)?;
    let campaign = snapshot.campaign;
    if campaign.requester_agent_id == worker_agent_id {
        return Err(
            "Fair-work policy: a node cannot claim or run work from a campaign it seeded."
                .to_string(),
        );
    }
    let claim = snapshot
        .claims
        .iter()
        .find(|claim| {
            claim.work_unit_id == work_unit_id
                && claim.worker_agent_id == worker_agent_id
                && claim.status == "claimed"
        })
        .ok_or_else(|| "Claim the work unit before running it.".to_string())?;
    let work_unit = snapshot
        .work_units
        .into_iter()
        .find(|unit| unit.work_unit_id == claim.work_unit_id)
        .ok_or_else(|| "Campaign work unit not found".to_string())?;
    if store.release_claim_if_work_unit_settled(&campaign_id, &work_unit_id, &worker_agent_id)? {
        let _ = app.emit("audit:labor_changed", ());
        return Err(
            "Work unit was already settled by the network; local claim released.".to_string(),
        );
    }
    let run = run_local_audit_skill(
        &app,
        &campaign,
        &work_unit,
        &provider,
        &model,
        &snapshot.contributions,
    );
    let output = if let Some(seconds) = max_runtime_seconds.filter(|seconds| *seconds > 0) {
        tokio::time::timeout(Duration::from_secs(seconds), run)
            .await
            .map_err(|_| {
                "Audit runtime exceeded the Genesis Auto Mode policy limit".to_string()
            })??
    } else {
        run.await?
    };
    let contribution = signed_contribution_for_work_unit(
        &keypair,
        &campaign,
        &work_unit,
        output.runtime,
        output.notes_markdown,
        output.findings,
        output.artifacts,
        output.coverage,
        output.commands,
    )?;
    let contribution = store.record_contribution(&contribution)?;
    sender
        .send(SwarmCommand::SendContribution {
            contribution: contribution.clone(),
            audience: campaign.requester_agent_id,
        })
        .map_err(|error| error.to_string())?;
    let _ = app.emit("audit:labor_changed", ());
    Ok(contribution)
}

#[tauri::command]
pub async fn run_campaign_audit_pipeline(
    app: AppHandle,
    state: State<'_, P2pState>,
    store: State<'_, AtpStore>,
    campaign_id: String,
    provider: String,
    model: String,
) -> Result<Vec<NodeContribution>, String> {
    let (keypair, _) = node_runtime(&state)?;
    let snapshot = store.campaign_report_snapshot(&campaign_id)?;
    let contributions =
        run_professional_audit_pipeline(&app, &store, &keypair, snapshot, &provider, &model)
            .await?;
    let _ = app.emit("audit:labor_changed", ());
    Ok(contributions)
}

#[tauri::command]
pub async fn run_accepted_audit_skill(
    app: AppHandle,
    state: State<'_, P2pState>,
    store: State<'_, AtpStore>,
    job_id: String,
    provider: String,
    model: String,
) -> Result<NodeContribution, String> {
    let (keypair, sender) = node_runtime(&state)?;
    let worker_agent_id = agent_id(&keypair.public());
    let job = store.get_job(&job_id)?;
    if job.worker_agent_id.as_deref() != Some(worker_agent_id.as_str()) {
        return Err("only the selected worker can execute this audit".to_string());
    }
    if job.status != "routed" {
        return Err("the requester must issue an active context lease first".to_string());
    }
    let campaign_id = campaign_id_for_transaction(&job.transaction_id);
    let snapshot = store.campaign_report_snapshot(&campaign_id)?;
    let campaign = snapshot.campaign.clone();
    let preferred_kind = if campaign.scope_text.to_ascii_lowercase().contains(".sol") {
        "defi-exploit-class-pass"
    } else {
        "dependency-config-review"
    };
    let work_unit = snapshot
        .work_units
        .iter()
        .find(|unit| unit.status == "open" && unit.kind == preferred_kind)
        .or_else(|| {
            snapshot
                .work_units
                .iter()
                .find(|unit| unit.status == "open")
        })
        .cloned()
        .ok_or_else(|| "Campaign has no open work units.".to_string())?;
    let claim = ensure_work_unit_claim(&store, &keypair, &campaign, &work_unit, &snapshot.claims)?;
    sender
        .send(SwarmCommand::SendWorkUnitClaim {
            claim,
            audience: campaign.requester_agent_id.clone(),
        })
        .map_err(|error| error.to_string())?;
    let output = run_local_audit_skill(
        &app,
        &campaign,
        &work_unit,
        &provider,
        &model,
        &snapshot.contributions,
    )
    .await?;
    let contribution = signed_contribution_for_work_unit(
        &keypair,
        &campaign,
        &work_unit,
        output.runtime,
        output.notes_markdown,
        output.findings,
        output.artifacts,
        output.coverage,
        output.commands,
    )?;
    let contribution = store.record_contribution(&contribution)?;
    sender
        .send(SwarmCommand::SendContribution {
            contribution: contribution.clone(),
            audience: job.requester_agent_id.clone(),
        })
        .map_err(|error| error.to_string())?;

    let contract_hash = job
        .contract_hash
        .clone()
        .ok_or_else(|| "routed audit has no contract hash".to_string())?;
    let contract = store.get_contract(&job.transaction_id)?;
    let leases = store.get_leases(&job.transaction_id)?;
    let result = execute_pipeline_audit_result(
        &keypair,
        &contract,
        &contract_hash,
        &leases,
        std::slice::from_ref(&contribution),
    )?;
    store.save_execution_result(&result)?;
    sender
        .send(SwarmCommand::SendExecutionResult {
            result,
            audience: contract.requester_agent_id,
        })
        .map_err(|error| error.to_string())?;
    let _ = app.emit("audit:labor_changed", ());
    let _ = app.emit("atp:jobs_changed", ());
    Ok(contribution)
}

#[tauri::command]
pub async fn run_accepted_audit_pipeline(
    app: AppHandle,
    state: State<'_, P2pState>,
    store: State<'_, AtpStore>,
    job_id: String,
    provider: String,
    model: String,
) -> Result<Vec<NodeContribution>, String> {
    let (keypair, sender) = node_runtime(&state)?;
    let worker_agent_id = agent_id(&keypair.public());
    let job = store.get_job(&job_id)?;
    if job.worker_agent_id.as_deref() != Some(worker_agent_id.as_str()) {
        return Err("only the selected worker can execute this audit".to_string());
    }
    if job.status != "routed" {
        return Err("the requester must issue an active context lease first".to_string());
    }

    let campaign_id = campaign_id_for_transaction(&job.transaction_id);
    let snapshot = store.campaign_report_snapshot(&campaign_id)?;
    let contributions =
        run_professional_audit_pipeline(&app, &store, &keypair, snapshot, &provider, &model)
            .await?;
    for contribution in &contributions {
        sender
            .send(SwarmCommand::SendContribution {
                contribution: contribution.clone(),
                audience: job.requester_agent_id.clone(),
            })
            .map_err(|error| error.to_string())?;
    }

    let contract_hash = job
        .contract_hash
        .clone()
        .ok_or_else(|| "routed audit has no contract hash".to_string())?;
    let contract = store.get_contract(&job.transaction_id)?;
    let leases = store.get_leases(&job.transaction_id)?;
    let result = execute_pipeline_audit_result(
        &keypair,
        &contract,
        &contract_hash,
        &leases,
        &contributions,
    )?;
    store.save_execution_result(&result)?;
    sender
        .send(SwarmCommand::SendExecutionResult {
            result,
            audience: contract.requester_agent_id,
        })
        .map_err(|error| error.to_string())?;
    let _ = app.emit("audit:labor_changed", ());
    let _ = app.emit("atp:jobs_changed", ());
    Ok(contributions)
}

async fn run_professional_audit_pipeline(
    app: &AppHandle,
    store: &AtpStore,
    keypair: &libp2p::identity::Keypair,
    snapshot: CampaignReportSnapshot,
    provider: &str,
    model: &str,
) -> Result<Vec<NodeContribution>, String> {
    if model.trim().is_empty() {
        return Err("Select a local model before running the audit pipeline.".to_string());
    }
    let campaign = snapshot.campaign.clone();
    let mut prior_contributions = snapshot.contributions.clone();
    let work_units = professional_pipeline_work_units(&snapshot);
    if work_units.is_empty() {
        return Err("Campaign has no pipeline work units.".to_string());
    }

    let mut contributions = Vec::new();
    let claims = snapshot.claims.clone();
    for work_unit in work_units {
        ensure_work_unit_claim(store, keypair, &campaign, &work_unit, &claims)?;
        let output = run_local_audit_skill(
            app,
            &campaign,
            &work_unit,
            provider,
            model,
            &prior_contributions,
        )
        .await?;
        let contribution = signed_contribution_for_work_unit(
            keypair,
            &campaign,
            &work_unit,
            output.runtime,
            output.notes_markdown,
            output.findings,
            output.artifacts,
            output.coverage,
            output.commands,
        )?;
        let contribution = store.record_contribution(&contribution)?;
        prior_contributions.push(contribution.clone());
        contributions.push(contribution);
    }
    Ok(contributions)
}

fn ensure_work_unit_claim(
    store: &AtpStore,
    keypair: &libp2p::identity::Keypair,
    campaign: &ProtocolAuditCampaign,
    work_unit: &AuditWorkUnit,
    claims: &[AuditWorkUnitClaim],
) -> Result<AuditWorkUnitClaim, String> {
    let worker_agent_id = agent_id(&keypair.public());
    if let Some(claimed_by) = work_unit.claimed_by_agent_id.as_deref() {
        if claimed_by == worker_agent_id {
            return claims
                .iter()
                .find(|claim| {
                    claim.work_unit_id == work_unit.work_unit_id
                        && claim.worker_agent_id == worker_agent_id
                        && claim.status == "claimed"
                })
                .cloned()
                .ok_or_else(|| "work unit claim is missing locally".to_string());
        }
        return Err("work unit is claimed by another worker".to_string());
    }
    let claim = signed_work_unit_claim(keypair, campaign, work_unit)?;
    store.record_work_unit_claim(&claim)
}

fn professional_pipeline_work_units(snapshot: &CampaignReportSnapshot) -> Vec<AuditWorkUnit> {
    let contributed = snapshot
        .contributions
        .iter()
        .map(|contribution| contribution.work_unit_id.as_str())
        .collect::<std::collections::HashSet<_>>();
    let is_smart_contract_scope = snapshot
        .campaign
        .scope_text
        .to_ascii_lowercase()
        .contains(".sol")
        || snapshot
            .work_units
            .iter()
            .any(|unit| unit.kind == "defi-exploit-class-pass");
    let mut kinds = vec![
        "scope-mapping",
        "repo-inventory",
        "dependency-config-review",
    ];
    if is_smart_contract_scope {
        kinds.push("defi-exploit-class-pass");
    }
    kinds.extend(["finding-validation", "final-report-section"]);

    let selected = kinds
        .iter()
        .filter_map(|kind| {
            snapshot
                .work_units
                .iter()
                .find(|unit| {
                    unit.kind == *kind && !contributed.contains(unit.work_unit_id.as_str())
                })
                .cloned()
        })
        .collect::<Vec<_>>();
    if !selected.is_empty() {
        return selected;
    }

    kinds
        .iter()
        .filter_map(|kind| {
            snapshot
                .work_units
                .iter()
                .find(|unit| unit.kind == *kind)
                .cloned()
        })
        .collect()
}

#[tauri::command]
pub async fn verify_campaign_contribution(
    app: AppHandle,
    state: State<'_, P2pState>,
    store: State<'_, AtpStore>,
    contribution_id: String,
    decision: String,
    reason_code: String,
    reason: String,
) -> Result<Vec<crate::audit_labor::CreditAllocation>, String> {
    let (keypair, sender) = node_runtime(&state)?;
    let contribution = store.get_contribution(&contribution_id)?;
    let local_agent_id = agent_id(&keypair.public());
    if let Some((verification, allocations)) =
        store.verification_bundle_for_contribution(&contribution_id)?
    {
        if contribution.worker_agent_id != local_agent_id {
            sender
                .send(SwarmCommand::SendVerificationResult {
                    verification,
                    allocations: allocations.clone(),
                    audience: contribution.worker_agent_id,
                })
                .map_err(|error| error.to_string())?;
        }
        return Ok(allocations);
    }
    let evidence_ref = format!("contribution:{}", contribution.receipt_hash);
    let evidence_hash = crate::audit_labor::sha256_ref(evidence_ref.as_bytes());
    let evidence_size = evidence_ref.len() as u64;
    let verification = signed_autonomous_finality_verification(
        &keypair,
        &contribution,
        decision,
        reason_code,
        reason,
        vec![VerificationEvidence {
            label: "signed contribution receipt".to_string(),
            reference: evidence_ref,
        }],
        vec![ContributionArtifact {
            path: "verification.md".to_string(),
            media_type: "text/markdown".to_string(),
            sha256: evidence_hash,
            size_bytes: evidence_size,
        }],
    )?;
    let allocations = store.record_verification(&verification)?;
    if contribution.worker_agent_id != local_agent_id {
        sender
            .send(SwarmCommand::SendVerificationResult {
                verification: verification.clone(),
                allocations: allocations.clone(),
                audience: contribution.worker_agent_id,
            })
            .map_err(|error| error.to_string())?;
    }
    let _ = app.emit("audit:labor_changed", ());
    Ok(allocations)
}

#[tauri::command]
pub async fn export_campaign_report(
    store: State<'_, AtpStore>,
    campaign_id: String,
) -> Result<ExportedReportBundle, String> {
    let bundle = export_campaign_report_bundle(&store, &campaign_id)?;
    Ok(ExportedReportBundle {
        campaign_id,
        bundle_path: bundle.to_string_lossy().to_string(),
    })
}

#[tauri::command]
pub async fn get_credit_summary(
    state: State<'_, P2pState>,
    store: State<'_, AtpStore>,
) -> Result<CreditSummary, String> {
    let (keypair, _) = node_runtime(&state)?;
    store.credit_summary(&agent_id(&keypair.public()))
}

#[tauri::command]
pub async fn create_audit(
    app: AppHandle,
    state: State<'_, P2pState>,
    store: State<'_, AtpStore>,
    repository: RepositorySummary,
    compensation: String,
    scope: Vec<String>,
    audit_brief_text: Option<String>,
    attachment_text: Option<String>,
    custom_skill_text: Option<String>,
) -> Result<AuditJob, String> {
    if repository.is_private {
        return Err("Private repositories are not supported".to_string());
    }
    if !is_git_commit_sha(&repository.commit_sha) {
        return Err("Audit requests must pin an exact Git commit SHA".to_string());
    }
    if compensation
        .parse::<f64>()
        .map_or(true, |amount| amount <= 0.0)
    {
        return Err("ATP Credits budget must be greater than zero".to_string());
    }

    let (keypair, sender) = node_runtime(&state)?;
    let requester_agent_id = agent_id(&keypair.public());
    let created_at = now_millis();
    let id = format!(
        "audit_{}_{}",
        created_at,
        keypair
            .public()
            .to_peer_id()
            .to_string()
            .chars()
            .rev()
            .take(8)
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>()
    );
    let payload = AuditJobPayload {
        id: id.clone(),
        repository,
        compensation,
        currency: "ATP Credits".to_string(),
        scope,
        audit_brief_text,
        attachment_text,
        custom_skill_text,
        requester_agent_id: requester_agent_id.clone(),
        created_at,
    };
    let envelope = create_signed_envelope(
        &keypair,
        AtpVerb::Discover,
        id.clone(),
        None,
        None,
        serde_json::to_value(AuditEventBody::Announce { job: payload })
            .map_err(|error| error.to_string())?,
    )?;

    store.commit_envelope(&envelope, &requester_agent_id, None)?;
    sender
        .send(SwarmCommand::SendEnvelope(envelope))
        .map_err(|error| error.to_string())?;
    let _ = app.emit("atp:jobs_changed", ());
    let _ = app.emit("audit:labor_changed", ());
    store.get_job(&id)
}

#[tauri::command]
pub async fn offer_audit(
    app: AppHandle,
    state: State<'_, P2pState>,
    store: State<'_, AtpStore>,
    job_id: String,
) -> Result<AuditJob, String> {
    let (keypair, sender) = node_runtime(&state)?;
    let worker_agent_id = agent_id(&keypair.public());
    let job = store.get_job(&job_id)?;
    if job.requester_agent_id == worker_agent_id {
        return Err("You cannot offer to fulfill your own audit".to_string());
    }
    if job.status != "discovered" {
        return Err("This audit is not open for a new worker offer".to_string());
    }

    let expires_at = (chrono::Utc::now() + chrono::Duration::minutes(30))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let contract = AuditContract::repository_audit(
        job.transaction_id.clone(),
        job.requester_agent_id.clone(),
        worker_agent_id.clone(),
        RepositoryTarget {
            full_name: job.repository.full_name.clone(),
            url: job.repository.url.clone(),
            commit_sha: job.repository.commit_sha.clone(),
        },
        job.scope.clone(),
        job.compensation.clone(),
        expires_at.clone(),
    );
    let envelope = create_signed_envelope_with_expiry(
        &keypair,
        AtpVerb::Negotiate,
        job.transaction_id.clone(),
        Some(job.requester_agent_id.clone()),
        Some(job.last_event_hash.clone()),
        serde_json::to_value(AuditEventBody::WorkerOffer {
            job_id: job.id.clone(),
            worker_agent_id,
            contract,
        })
        .map_err(|error| error.to_string())?,
        Some(expires_at),
    )?;

    store.commit_envelope(&envelope, &agent_id(&keypair.public()), None)?;
    sender
        .send(SwarmCommand::SendEnvelope(envelope))
        .map_err(|error| error.to_string())?;
    let _ = app.emit("atp:jobs_changed", ());
    store.get_job(&job_id)
}

#[tauri::command]
pub async fn accept_offer(
    app: AppHandle,
    state: State<'_, P2pState>,
    store: State<'_, AtpStore>,
    job_id: String,
) -> Result<AuditJob, String> {
    let (keypair, sender) = node_runtime(&state)?;
    let requester_agent_id = agent_id(&keypair.public());
    let job = store.get_job(&job_id)?;
    if job.requester_agent_id != requester_agent_id {
        return Err("Only the requester can select a worker".to_string());
    }
    if job.status != "negotiating" {
        return Err("This audit does not have a pending worker offer".to_string());
    }
    let worker_agent_id = job
        .worker_agent_id
        .clone()
        .ok_or_else(|| "The pending offer has no worker identity".to_string())?;
    let contract_hash = job
        .contract_hash
        .clone()
        .ok_or_else(|| "The pending offer has no canonical contract hash".to_string())?;

    let envelope = create_signed_envelope(
        &keypair,
        AtpVerb::Negotiate,
        job.transaction_id.clone(),
        Some(worker_agent_id.clone()),
        Some(job.last_event_hash.clone()),
        serde_json::to_value(AuditEventBody::WorkerSelected {
            job_id: job.id.clone(),
            worker_agent_id,
            contract_hash,
        })
        .map_err(|error| error.to_string())?,
    )?;

    store.commit_envelope(&envelope, &requester_agent_id, None)?;
    sender
        .send(SwarmCommand::SendEnvelope(envelope))
        .map_err(|error| error.to_string())?;
    let _ = app.emit("atp:jobs_changed", ());
    store.get_job(&job_id)
}

#[tauri::command]
pub async fn route_audit(
    app: AppHandle,
    state: State<'_, P2pState>,
    store: State<'_, AtpStore>,
    job_id: String,
) -> Result<AuditJob, String> {
    let (keypair, sender) = node_runtime(&state)?;
    let requester_agent_id = agent_id(&keypair.public());
    let job = store.get_job(&job_id)?;
    if job.requester_agent_id != requester_agent_id {
        return Err("only the requester can issue the audit context lease".to_string());
    }
    if job.status != "negotiated" {
        return Err("the audit must have an accepted worker before it can be routed".to_string());
    }
    let worker_agent_id = job
        .worker_agent_id
        .clone()
        .ok_or_else(|| "accepted audit has no worker".to_string())?;
    let contract_hash = job
        .contract_hash
        .clone()
        .ok_or_else(|| "accepted audit has no contract hash".to_string())?;
    let contract = store.get_contract(&job.transaction_id)?;
    let leases = create_repository_leases(&keypair, &contract)?;
    let envelope = create_signed_envelope(
        &keypair,
        AtpVerb::Route,
        job.transaction_id.clone(),
        Some(worker_agent_id),
        Some(job.last_event_hash.clone()),
        serde_json::to_value(AuditEventBody::RouteAudit {
            job_id: job.id.clone(),
            contract_hash,
            leases,
        })
        .map_err(|error| error.to_string())?,
    )?;

    store.commit_envelope(&envelope, &requester_agent_id, None)?;
    sender
        .send(SwarmCommand::SendEnvelope(envelope))
        .map_err(|error| error.to_string())?;
    let _ = app.emit("atp:jobs_changed", ());
    store.get_job(&job_id)
}

#[tauri::command]
pub async fn run_audit(
    app: AppHandle,
    state: State<'_, P2pState>,
    store: State<'_, AtpStore>,
    job_id: String,
) -> Result<AuditJob, String> {
    let (keypair, sender) = node_runtime(&state)?;
    let worker_agent_id = agent_id(&keypair.public());
    let job = store.get_job(&job_id)?;
    if job.worker_agent_id.as_deref() != Some(worker_agent_id.as_str()) {
        return Err("only the selected worker can execute this audit".to_string());
    }
    if job.status != "routed" {
        return Err("the requester must issue an active context lease first".to_string());
    }
    let contract_hash = job
        .contract_hash
        .clone()
        .ok_or_else(|| "routed audit has no contract hash".to_string())?;
    let contract = store.get_contract(&job.transaction_id)?;
    let leases = store.get_leases(&job.transaction_id)?;
    let result =
        execute_repository_audit(&keypair, &contract, &contract_hash, &leases, &data_dir()?)
            .await?;
    store.save_execution_result(&result)?;
    sender
        .send(SwarmCommand::SendExecutionResult {
            result,
            audience: contract.requester_agent_id,
        })
        .map_err(|error| error.to_string())?;
    let _ = app.emit("atp:jobs_changed", ());
    store.get_job(&job_id)
}

#[tauri::command]
pub async fn approve_result(
    app: AppHandle,
    state: State<'_, P2pState>,
    store: State<'_, AtpStore>,
    job_id: String,
) -> Result<AuditJob, String> {
    let (keypair, sender) = node_runtime(&state)?;
    let requester_agent_id = agent_id(&keypair.public());
    let job = store.get_job(&job_id)?;
    if job.requester_agent_id != requester_agent_id {
        return Err("only the requester can approve the worker result".to_string());
    }
    if job.status != "routed" {
        return Err("the audit result can only settle from the routed state".to_string());
    }
    let worker_agent_id = job
        .worker_agent_id
        .clone()
        .ok_or_else(|| "routed audit has no worker".to_string())?;
    let contract_hash = job
        .contract_hash
        .clone()
        .ok_or_else(|| "routed audit has no contract hash".to_string())?;
    let result = store.get_execution_result(&job.transaction_id)?;
    let approved = ReceiptApproval {
        by: requester_agent_id.clone(),
        method: "requester-verified-result".to_string(),
        time: now_rfc3339(),
    };
    let envelope = create_signed_envelope(
        &keypair,
        AtpVerb::Settle,
        job.transaction_id.clone(),
        Some(worker_agent_id),
        Some(job.last_event_hash.clone()),
        serde_json::to_value(AuditEventBody::SettlementApproved {
            job_id: job.id.clone(),
            contract_hash,
            result_hash: result.result_hash,
            approved,
        })
        .map_err(|error| error.to_string())?,
    )?;

    store.commit_envelope(&envelope, &requester_agent_id, None)?;
    sender
        .send(SwarmCommand::SendEnvelope(envelope))
        .map_err(|error| error.to_string())?;
    let _ = app.emit("atp:jobs_changed", ());
    store.get_job(&job_id)
}

#[tauri::command]
pub async fn migrate_legacy_jobs(
    app: AppHandle,
    state: State<'_, P2pState>,
    store: State<'_, AtpStore>,
    jobs: Vec<LegacyAuditJob>,
) -> Result<MigrationResult, String> {
    let (keypair, sender) = node_runtime(&state)?;
    let peer_id = keypair.public().to_peer_id().to_string();
    let local_agent_id = agent_id(&keypair.public());
    let mut migrated = 0;
    let mut skipped = 0;

    for legacy in jobs {
        if store.contains_job(&legacy.id)? {
            migrated += 1;
            continue;
        }
        if legacy.requester_peer_id != peer_id
            || !matches!(legacy.currency.as_str(), "USDC" | "ATP Credits")
            || !is_git_commit_sha(&legacy.repository.commit_sha)
        {
            skipped += 1;
            continue;
        }

        let payload = AuditJobPayload {
            id: legacy.id.clone(),
            repository: legacy.repository,
            compensation: legacy.compensation,
            currency: legacy.currency,
            scope: legacy.scope,
            audit_brief_text: None,
            attachment_text: None,
            custom_skill_text: None,
            requester_agent_id: local_agent_id.clone(),
            created_at: legacy.created_at,
        };
        let envelope = create_signed_envelope(
            &keypair,
            AtpVerb::Discover,
            legacy.id,
            None,
            None,
            serde_json::to_value(AuditEventBody::Announce { job: payload })
                .map_err(|error| error.to_string())?,
        )?;
        store.commit_envelope(&envelope, &local_agent_id, None)?;
        sender
            .send(SwarmCommand::SendEnvelope(envelope))
            .map_err(|error| error.to_string())?;
        migrated += 1;
    }

    if migrated > 0 {
        let _ = app.emit("atp:jobs_changed", ());
    }
    Ok(MigrationResult { migrated, skipped })
}

#[tauri::command]
pub async fn get_peers(state: State<'_, P2pState>) -> Result<Vec<PeerInfo>, String> {
    let inner = state.inner.lock().map_err(|error| error.to_string())?;
    Ok(inner.peers.values().cloned().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fair_work_claim_blocks_self_dealing_and_the_broadcast_window() {
        let now = 10_000_000u64;
        let aged = now - WORK_UNIT_CLAIMABLE_AFTER_MS - 1;
        let fresh = now - 1;

        // Seeder cannot work its own campaign, even for an aged unit.
        assert!(ensure_fair_work_claim("agentA", "agentA", aged, now)
            .unwrap_err()
            .contains("campaign it seeded"));

        // Different node, but the unit is still inside its broadcast window.
        assert!(ensure_fair_work_claim("agentA", "agentB", fresh, now)
            .unwrap_err()
            .contains("broadcast window"));

        // Different node and past the window: allowed.
        assert!(ensure_fair_work_claim("agentA", "agentB", aged, now).is_ok());
    }

    #[test]
    fn guardian_target_index_is_valid_and_honest() {
        let index: GuardianTargetIndex = serde_json::from_str(include_str!(
            "../../protocol/targets/guardian-target-index.json"
        ))
        .expect("guardian target index should parse");

        assert!(index.targets.len() >= 142);
        for target in index.targets {
            assert!(target.repo_url.starts_with("https://github.com/"));
            assert!(target.source.iter().any(|source| source == "github"));
            assert!(!target.category.trim().is_empty());
            assert!(target.tvl_risk_rank > 0);
            assert!(target.contract_criticality > 0);
            assert!(target.priority_score > 0);
            assert!(target.credit_budget > 0);
            assert!(target.scope_text.contains("No repository writes"));
            assert!(!target.audit_brief.to_lowercase().contains("immunefi"));
            assert!(target.audit_brief.contains("Do not submit externally"));
        }
    }
}

fn node_runtime(
    state: &State<'_, P2pState>,
) -> Result<
    (
        libp2p::identity::Keypair,
        mpsc::UnboundedSender<SwarmCommand>,
    ),
    String,
> {
    let inner = state.inner.lock().map_err(|error| error.to_string())?;
    let keypair = inner
        .keypair
        .clone()
        .ok_or_else(|| "P2P node has not started".to_string())?;
    let sender = inner
        .sender
        .clone()
        .ok_or_else(|| "P2P node has not started".to_string())?;
    Ok((keypair, sender))
}

fn ensure_verification_pool_clear(store: &AtpStore, local_agent_id: &str) -> Result<(), String> {
    let pending = store.pending_network_verification_count_for_verifier(local_agent_id)?;
    if pending == 0 {
        let self_pending = store.pending_contribution_count_for_worker(local_agent_id)?;
        if self_pending < MAX_SELF_PENDING_CONTRIBUTIONS {
            return Ok(());
        }
        return Err(format!(
            "Worker backpressure active: {self_pending} self-authored receipt{} awaiting independent verification; pause new audit work until the network clears below {MAX_SELF_PENDING_CONTRIBUTIONS}.",
            if self_pending == 1 { "" } else { "s" }
        ));
    }
    Err(format!(
        "Verifier duty active: clear {pending} independently verifiable pending receipt{} before claiming or running new audit work.",
        if pending == 1 { "" } else { "s" }
    ))
}

fn parse_github_input(value: &str) -> Result<GitHubInputTarget, String> {
    let parsed = reqwest::Url::parse(value.trim()).map_err(|_| GITHUB_REPOSITORY_URL_ERROR)?;
    if parsed.scheme() != "https" {
        return Err(GITHUB_REPOSITORY_URL_ERROR.to_string());
    }
    let host = parsed
        .host_str()
        .map(|host| host.to_ascii_lowercase())
        .unwrap_or_default();
    if host != "github.com" && host != "www.github.com" {
        return Err(GITHUB_REPOSITORY_URL_ERROR.to_string());
    }
    let segments = parsed
        .path_segments()
        .map(|segments| {
            segments
                .filter(|segment| !segment.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if segments.len() < 2 {
        return Err(GITHUB_REPOSITORY_URL_ERROR.to_string());
    }
    let owner = segments[0].trim().to_string();
    let repo = segments[1].trim().trim_end_matches(".git").to_string();
    if owner.is_empty() || repo.is_empty() || owner == "." || repo == "." {
        return Err(GITHUB_REPOSITORY_URL_ERROR.to_string());
    }
    let route = segments.get(2).map(|segment| segment.to_ascii_lowercase());
    let kind = match route.as_deref() {
        Some("blob") => GitHubInputKind::Blob,
        Some("tree") => GitHubInputKind::Tree,
        _ => GitHubInputKind::Repository,
    };
    let path_segments = if kind == GitHubInputKind::Repository {
        Vec::new()
    } else {
        segments.into_iter().skip(3).collect()
    };
    Ok(GitHubInputTarget {
        api_url: format!(
            "https://api.github.com/repos/{}/{}",
            encode_path_segment(&owner),
            encode_path_segment(&repo)
        ),
        kind,
        path_segments,
    })
}

async fn resolve_github_path(
    client: &reqwest::Client,
    api_url: &str,
    repository: &GitHubRepositoryResponse,
    target: &GitHubInputTarget,
) -> Result<(String, Option<String>, Option<String>), String> {
    if target.kind == GitHubInputKind::Repository || target.path_segments.is_empty() {
        return Ok((
            resolve_commit(client, api_url, &repository.default_branch, false)
                .await?
                .expect("required commit resolution should return a commit"),
            None,
            None,
        ));
    }

    if target.kind == GitHubInputKind::Blob && target.path_segments.len() < 2 {
        return Err("That GitHub file URL is missing a branch or path.".to_string());
    }

    let default_branch_segments = repository.default_branch.split('/').collect::<Vec<_>>();
    let starts_with_default_branch =
        default_branch_segments
            .iter()
            .enumerate()
            .all(|(index, segment)| {
                target
                    .path_segments
                    .get(index)
                    .is_some_and(|value| value == segment)
            });
    if starts_with_default_branch {
        let focus_path = target
            .path_segments
            .iter()
            .skip(default_branch_segments.len())
            .cloned()
            .collect::<Vec<_>>()
            .join("/");
        return Ok((
            resolve_commit(client, api_url, &repository.default_branch, false)
                .await?
                .expect("required commit resolution should return a commit"),
            (!focus_path.is_empty()).then_some(focus_path),
            Some(repository.default_branch.clone()),
        ));
    }

    let max_ref_segments = if target.kind == GitHubInputKind::Blob {
        target.path_segments.len().saturating_sub(1).max(1)
    } else {
        target.path_segments.len()
    };
    for index in (1..=max_ref_segments).rev() {
        let focus_ref = target.path_segments[..index].join("/");
        if let Some(commit_sha) = resolve_commit(client, api_url, &focus_ref, true).await? {
            let focus_path = target.path_segments[index..].join("/");
            return Ok((
                commit_sha,
                (!focus_path.is_empty()).then_some(focus_path),
                Some(focus_ref),
            ));
        }
    }

    Err(
        "GitHub resolved the repository, but CYPHES could not resolve the branch or file path from that URL."
            .to_string(),
    )
}

async fn resolve_commit(
    client: &reqwest::Client,
    api_url: &str,
    reference: &str,
    optional: bool,
) -> Result<Option<String>, String> {
    let url = format!("{api_url}/commits/{}", encode_path_segment(reference));
    match github::get_json::<GitHubCommitResponse>(client, &url).await {
        Ok(commit) => {
            if !is_git_commit_sha(&commit.sha) {
                Err(format!(
                    "GitHub returned an invalid commit for {reference}."
                ))
            } else {
                Ok(Some(commit.sha))
            }
        }
        Err(error) if optional && error.contains("404") => Ok(None),
        Err(error) => Err(format!(
            "GitHub could not resolve {reference} to a commit. {error}"
        )),
    }
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
