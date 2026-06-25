use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::mpsc;

use crate::{
    atp::{
        agent_id, create_signed_envelope, create_signed_envelope_with_expiry, now_rfc3339, AtpVerb,
    },
    audit_labor::{
        signed_contribution, signed_verification, signed_work_unit_claim, AuditFinding,
        AuditWorkUnit, AuditWorkUnitClaim, CampaignAttachment, CampaignReportSnapshot,
        ContributionArtifact, CoverageItem, CreditSummary, NodeContribution, ProtocolAuditCampaign,
        RuntimeDescriptor, VerificationEvidence,
    },
    audit_profile::{is_git_commit_sha, AuditContract, ReceiptApproval, RepositoryTarget},
    audit_runtime::{
        list_local_models, local_model_providers, run_local_audit_skill, LocalModelList,
    },
    bundle::export_campaign_report_bundle,
    p2p::{load_or_create_identity, spawn_swarm, SwarmCommand, ATP_PROTOCOL},
    state::{P2pState, PeerInfo},
    store::{
        campaign_id_for_transaction, data_dir, now_millis, AtpStore, AuditEventBody, AuditJob,
        AuditJobPayload, LegacyAuditJob, RepositorySummary,
    },
    worker::{create_repository_leases, execute_pipeline_audit_result, execute_repository_audit},
};

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
    pub repo_url: String,
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
    let (keypair, _) = node_runtime(&state)?;
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
    let contribution = signed_contribution(
        &keypair,
        campaign_id,
        work_unit_id,
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
    let (keypair, _) = node_runtime(&state)?;
    let snapshot = store.campaign_report_snapshot(&campaign_id)?;
    let campaign = snapshot.campaign;
    let work_unit = snapshot
        .work_units
        .into_iter()
        .find(|unit| unit.work_unit_id == work_unit_id)
        .ok_or_else(|| "Campaign work unit not found".to_string())?;
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
    let contribution = signed_contribution(
        &keypair,
        campaign.campaign_id.clone(),
        work_unit.work_unit_id,
        output.runtime,
        output.notes_markdown,
        output.findings,
        output.artifacts,
        output.coverage,
        output.commands,
    )?;
    let contribution = store.record_contribution(&contribution)?;
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
    let snapshot = store.campaign_report_snapshot(&campaign_id)?;
    if snapshot.campaign.requester_agent_id == worker_agent_id {
        return Err(
            "Requester already owns this campaign; remote claims are for other nodes.".to_string(),
        );
    }
    let work_unit = snapshot
        .work_units
        .iter()
        .find(|unit| unit.work_unit_id == work_unit_id)
        .cloned()
        .ok_or_else(|| "Campaign work unit not found".to_string())?;
    if work_unit.status != "open" {
        return Err("Only open work units can be claimed.".to_string());
    }
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
    let snapshot = store.campaign_report_snapshot(&campaign_id)?;
    let campaign = snapshot.campaign;
    if campaign.requester_agent_id == worker_agent_id {
        return Err("Use the requester pipeline for campaigns you created.".to_string());
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
    let contribution = signed_contribution(
        &keypair,
        campaign.campaign_id.clone(),
        work_unit.work_unit_id,
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
    let campaign = snapshot.campaign;
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
    let output = run_local_audit_skill(
        &app,
        &campaign,
        &work_unit,
        &provider,
        &model,
        &snapshot.contributions,
    )
    .await?;
    let contribution = signed_contribution(
        &keypair,
        campaign.campaign_id.clone(),
        work_unit.work_unit_id,
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
    for work_unit in work_units {
        let output = run_local_audit_skill(
            app,
            &campaign,
            &work_unit,
            provider,
            model,
            &prior_contributions,
        )
        .await?;
        let contribution = signed_contribution(
            keypair,
            campaign.campaign_id.clone(),
            work_unit.work_unit_id,
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
    let evidence_ref = format!("contribution:{}", contribution.receipt_hash);
    let evidence_hash = crate::audit_labor::sha256_ref(evidence_ref.as_bytes());
    let evidence_size = evidence_ref.len() as u64;
    let verification = signed_verification(
        &keypair,
        contribution.campaign_id.clone(),
        contribution.contribution_id.clone(),
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
    let local_agent_id = agent_id(&keypair.public());
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
    fn guardian_target_index_is_valid_and_honest() {
        let index: GuardianTargetIndex = serde_json::from_str(include_str!(
            "../../protocol/targets/guardian-target-index.json"
        ))
        .expect("guardian target index should parse");

        assert!(!index.targets.is_empty());
        for target in index.targets {
            assert!(target.repo_url.starts_with("https://github.com/"));
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
