use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::mpsc;

use crate::{
    atp::{
        agent_id, create_signed_envelope, create_signed_envelope_with_expiry, now_rfc3339, AtpVerb,
    },
    audit_labor::{
        signed_contribution, signed_verification, AuditFinding, CampaignReportSnapshot,
        ContributionArtifact, CoverageItem, CreditSummary, NodeContribution, ProtocolAuditCampaign,
        RuntimeDescriptor, VerificationEvidence,
    },
    audit_profile::{is_git_commit_sha, AuditContract, ReceiptApproval, RepositoryTarget},
    bundle::export_campaign_report_bundle,
    p2p::{load_or_create_identity, spawn_swarm, SwarmCommand, ATP_PROTOCOL},
    state::{P2pState, PeerInfo},
    store::{
        data_dir, now_millis, AtpStore, AuditEventBody, AuditJob, AuditJobPayload, LegacyAuditJob,
        RepositorySummary,
    },
    worker::{create_repository_leases, execute_repository_audit},
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
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportedReportBundle {
    pub campaign_id: String,
    pub bundle_path: String,
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
    let (keypair, _) = node_runtime(&state)?;
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
        agent_id(&keypair.public()),
    )?;
    let campaign = store.create_protocol_campaign(&campaign)?;
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
        "Run Audit Skill local fixture contribution. Runtime adapter is connected, but OpenClaw/Hermes execution is not yet enabled in this build.".to_string()
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
            title: "Coverage-only audit skill output".to_string(),
            severity: "informational".to_string(),
            status: "non_reportable".to_string(),
            impact: None,
            evidence: vec![
                "This deterministic local fixture records coverage; it does not claim an exploit."
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
pub async fn verify_campaign_contribution(
    app: AppHandle,
    state: State<'_, P2pState>,
    store: State<'_, AtpStore>,
    contribution_id: String,
    decision: String,
    reason_code: String,
    reason: String,
) -> Result<Vec<crate::audit_labor::CreditAllocation>, String> {
    let (keypair, _) = node_runtime(&state)?;
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
