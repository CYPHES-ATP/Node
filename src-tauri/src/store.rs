use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use base64::Engine as _;
use chrono::{DateTime, SecondsFormat, TimeZone, Utc};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};

use crate::{
    atp::{
        event_hash, now_rfc3339, transition, verify_envelope, AtpAck, AtpEnvelope, AtpVerb,
        ATP_GENESIS_HASH,
    },
    audit_labor::{
        allocate_credits, default_work_units, validate_campaign, verify_signed_contribution,
        verify_signed_verification, AuditWorkUnit, CampaignReportSnapshot, CreditAllocation,
        CreditSummary, NodeContribution, ProtocolAuditCampaign, VerificationResult,
    },
    audit_profile::{
        contract_hash, receipt_signature_value, validate_contract, validate_receipt,
        validate_receipt_parties, AuditContract, AuditReceipt, ReceiptApproval, RepositoryTarget,
    },
    worker::{verify_execution_result, ContextLease, SignedExecutionResult},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositorySummary {
    pub full_name: String,
    pub url: String,
    pub description: Option<String>,
    pub language: Option<String>,
    pub default_branch: String,
    pub stars: u64,
    pub is_private: bool,
    #[serde(default)]
    pub commit_sha: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditJobPayload {
    pub id: String,
    pub repository: RepositorySummary,
    pub compensation: String,
    pub currency: String,
    pub scope: Vec<String>,
    pub requester_agent_id: String,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditJob {
    pub id: String,
    pub transaction_id: String,
    pub repository: RepositorySummary,
    pub compensation: String,
    pub currency: String,
    pub scope: Vec<String>,
    pub status: String,
    pub delivery_state: String,
    pub requester_agent_id: String,
    pub worker_agent_id: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
    pub last_event_hash: String,
    pub contract_hash: Option<String>,
    pub result_hash: Option<String>,
    pub receipt_hash: Option<String>,
    pub bundle_path: Option<String>,
    pub acknowledged_peers: u64,
    pub origin: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "action",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum AuditEventBody {
    Announce {
        job: AuditJobPayload,
    },
    WorkerOffer {
        job_id: String,
        worker_agent_id: String,
        contract: AuditContract,
    },
    WorkerSelected {
        job_id: String,
        worker_agent_id: String,
        contract_hash: String,
    },
    RouteAudit {
        job_id: String,
        contract_hash: String,
        leases: Vec<ContextLease>,
    },
    SettlementApproved {
        job_id: String,
        contract_hash: String,
        result_hash: String,
        approved: ReceiptApproval,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyAuditJob {
    pub id: String,
    pub repository: RepositorySummary,
    pub compensation: String,
    pub currency: String,
    pub scope: Vec<String>,
    pub requester_peer_id: String,
    pub created_at: u64,
}

#[derive(Debug, Clone)]
pub struct TransactionContext {
    pub state: Option<String>,
    pub last_event_hash: Option<String>,
    pub requester_agent_id: Option<String>,
    pub worker_agent_id: Option<String>,
    pub repository: Option<RepositorySummary>,
    pub scope: Option<Vec<String>>,
    pub compensation: Option<String>,
    pub currency: Option<String>,
    pub contract_hash: Option<String>,
    pub result_hash: Option<String>,
}

#[derive(Clone)]
pub struct AtpStore {
    connection: Arc<Mutex<Connection>>,
}

pub fn campaign_id_for_transaction(transaction_id: &str) -> String {
    format!("campaign_{transaction_id}")
}

impl AtpStore {
    pub fn open_default() -> Result<Self, String> {
        let path = database_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }

        let connection = Connection::open(&path).map_err(|error| error.to_string())?;
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .map_err(|error| error.to_string())?;
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .map_err(|error| error.to_string())?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
                .map_err(|error| error.to_string())?;
        }

        let store = Self {
            connection: Arc::new(Mutex::new(connection)),
        };
        store.initialize()?;
        Ok(store)
    }

    pub fn contains_job(&self, job_id: &str) -> Result<bool, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        let exists = connection
            .query_row(
                "SELECT 1 FROM audit_jobs WHERE id = ?1",
                params![job_id],
                |_| Ok(()),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .is_some();
        Ok(exists)
    }

    pub fn list_jobs(&self) -> Result<Vec<AuditJob>, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        let mut statement = connection
            .prepare(
                "SELECT id, transaction_id, repository_json, compensation, currency, scope_json,
                        status, delivery_state, requester_agent_id, worker_agent_id, created_at,
                        updated_at, last_event_hash,
                        (SELECT contract_hash FROM audit_contracts c
                         WHERE c.transaction_id = audit_jobs.transaction_id),
                        (SELECT result_hash FROM audit_execution_results r
                         WHERE r.transaction_id = audit_jobs.transaction_id),
                        (SELECT receipt_hash FROM audit_receipts r
                         WHERE r.transaction_id = audit_jobs.transaction_id),
                        (SELECT bundle_path FROM audit_receipts r
                         WHERE r.transaction_id = audit_jobs.transaction_id),
                        origin,
                        (SELECT COUNT(*) FROM deliveries d
                         WHERE d.transaction_id = audit_jobs.transaction_id
                           AND d.accepted = 1)
                 FROM audit_jobs
                 ORDER BY created_at DESC",
            )
            .map_err(|error| error.to_string())?;

        let rows = statement
            .query_map([], row_to_job)
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())
    }

    pub fn get_job(&self, job_id: &str) -> Result<AuditJob, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        connection
            .query_row(
                "SELECT id, transaction_id, repository_json, compensation, currency, scope_json,
                        status, delivery_state, requester_agent_id, worker_agent_id, created_at,
                        updated_at, last_event_hash,
                        (SELECT contract_hash FROM audit_contracts c
                         WHERE c.transaction_id = audit_jobs.transaction_id),
                        (SELECT result_hash FROM audit_execution_results r
                         WHERE r.transaction_id = audit_jobs.transaction_id),
                        (SELECT receipt_hash FROM audit_receipts r
                         WHERE r.transaction_id = audit_jobs.transaction_id),
                        (SELECT bundle_path FROM audit_receipts r
                         WHERE r.transaction_id = audit_jobs.transaction_id),
                        origin,
                        (SELECT COUNT(*) FROM deliveries d
                         WHERE d.transaction_id = audit_jobs.transaction_id
                           AND d.accepted = 1)
                 FROM audit_jobs
                 WHERE id = ?1",
                params![job_id],
                row_to_job,
            )
            .map_err(|error| error.to_string())
    }

    pub fn get_contract(&self, transaction_id: &str) -> Result<AuditContract, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        contract_in_connection(&connection, transaction_id)
    }

    pub fn get_leases(&self, transaction_id: &str) -> Result<Vec<ContextLease>, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        leases_in_connection(&connection, transaction_id)
    }

    pub fn save_execution_result(&self, result: &SignedExecutionResult) -> Result<(), String> {
        result.verify()?;
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        let contract = contract_in_connection(&connection, &result.transaction_id)?;
        let leases = leases_in_connection(&connection, &result.transaction_id)?;
        verify_execution_result(result, &contract, &leases)?;
        connection
            .execute(
                "INSERT INTO audit_execution_results
                    (transaction_id, result_hash, result_json, created_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(transaction_id) DO UPDATE SET
                    result_hash = excluded.result_hash,
                    result_json = excluded.result_json,
                    created_at = excluded.created_at",
                params![
                    result.transaction_id,
                    result.result_hash,
                    serde_json::to_string(result).map_err(|error| error.to_string())?,
                    now_millis() as i64,
                ],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn get_execution_result(
        &self,
        transaction_id: &str,
    ) -> Result<SignedExecutionResult, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        execution_result_in_connection(&connection, transaction_id)
    }

    pub fn get_receipt(&self, transaction_id: &str) -> Result<AuditReceipt, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        let value = connection
            .query_row(
                "SELECT receipt_json FROM audit_receipts WHERE transaction_id = ?1",
                params![transaction_id],
                |row| row.get::<_, String>(0),
            )
            .map_err(|error| error.to_string())?;
        serde_json::from_str(&value).map_err(|error| error.to_string())
    }

    pub fn create_protocol_campaign(
        &self,
        campaign: &ProtocolAuditCampaign,
    ) -> Result<ProtocolAuditCampaign, String> {
        validate_campaign(campaign)?;
        let work_units = default_work_units(campaign);
        let mut connection = self.connection.lock().map_err(|error| error.to_string())?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "INSERT INTO protocol_audit_campaigns
                    (campaign_id, campaign_json, status, requester_agent_id, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    campaign.campaign_id,
                    serde_json::to_string(campaign).map_err(|error| error.to_string())?,
                    campaign.status,
                    campaign.requester_agent_id,
                    millis_from_rfc3339(&campaign.created_at)?,
                    millis_from_rfc3339(&campaign.updated_at)?,
                ],
            )
            .map_err(|error| error.to_string())?;
        for work_unit in &work_units {
            insert_work_unit(&transaction, work_unit)?;
        }
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(campaign.clone())
    }

    pub fn list_protocol_campaigns(&self) -> Result<Vec<ProtocolAuditCampaign>, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        let mut statement = connection
            .prepare(
                "SELECT campaign_json FROM protocol_audit_campaigns
                 ORDER BY created_at DESC",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?;
        rows.map(|row| {
            let json = row.map_err(|error| error.to_string())?;
            serde_json::from_str(&json).map_err(|error| error.to_string())
        })
        .collect()
    }

    #[cfg(test)]
    pub fn list_work_units(&self, campaign_id: &str) -> Result<Vec<AuditWorkUnit>, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        work_units_in_connection(&connection, campaign_id)
    }

    pub fn record_contribution(
        &self,
        contribution: &NodeContribution,
    ) -> Result<NodeContribution, String> {
        verify_signed_contribution(contribution)?;
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        let campaign: Option<String> = connection
            .query_row(
                "SELECT campaign_id FROM protocol_audit_campaigns WHERE campaign_id = ?1",
                params![contribution.campaign_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if campaign.is_none() {
            return Err("contribution campaign is not known locally".to_string());
        }
        let work_unit: Option<String> = connection
            .query_row(
                "SELECT work_unit_id FROM audit_work_units
                 WHERE campaign_id = ?1 AND work_unit_id = ?2",
                params![contribution.campaign_id, contribution.work_unit_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if work_unit.is_none() {
            return Err("contribution work unit is not known locally".to_string());
        }
        connection
            .execute(
                "INSERT INTO audit_contributions
                    (contribution_id, campaign_id, work_unit_id, worker_agent_id,
                     receipt_hash, contribution_json, status, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'submitted', ?7)",
                params![
                    contribution.contribution_id,
                    contribution.campaign_id,
                    contribution.work_unit_id,
                    contribution.worker_agent_id,
                    contribution.receipt_hash,
                    serde_json::to_string(contribution).map_err(|error| error.to_string())?,
                    millis_from_rfc3339(&contribution.created_at)?,
                ],
            )
            .map_err(|error| error.to_string())?;
        update_work_unit_status(
            &connection,
            &contribution.campaign_id,
            &contribution.work_unit_id,
            "submitted",
        )?;
        Ok(contribution.clone())
    }

    pub fn record_verification(
        &self,
        verification: &VerificationResult,
    ) -> Result<Vec<CreditAllocation>, String> {
        verify_signed_verification(verification)?;
        let contribution = self.get_contribution(&verification.target_contribution_id)?;
        let allocations = if verification.decision == "accepted" {
            allocate_credits(&contribution, verification)?
        } else {
            Vec::new()
        };
        let mut connection = self.connection.lock().map_err(|error| error.to_string())?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "INSERT INTO audit_verifications
                    (verification_id, campaign_id, target_contribution_id, verifier_agent_id,
                     decision, verification_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    verification.verification_id,
                    verification.campaign_id,
                    verification.target_contribution_id,
                    verification.verifier_agent_id,
                    verification.decision,
                    serde_json::to_string(verification).map_err(|error| error.to_string())?,
                    millis_from_rfc3339(&verification.created_at)?,
                ],
            )
            .map_err(|error| error.to_string())?;
        let contribution_status = match verification.decision.as_str() {
            "accepted" | "reproduced" => "accepted",
            "rejected" => "rejected",
            "challenged" => "challenged",
            "revision_requested" => "revision_requested",
            _ => "reviewed",
        };
        transaction
            .execute(
                "UPDATE audit_contributions
                 SET status = ?2
                 WHERE contribution_id = ?1",
                params![verification.target_contribution_id, contribution_status],
            )
            .map_err(|error| error.to_string())?;
        update_work_unit_status(
            &transaction,
            &contribution.campaign_id,
            &contribution.work_unit_id,
            contribution_status,
        )?;
        for allocation in &allocations {
            transaction
                .execute(
                    "INSERT INTO credit_allocations
                        (allocation_id, campaign_id, contribution_id, verification_id,
                         receiver_agent_id, total, allocation_json, issued_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        allocation.allocation_id,
                        allocation.campaign_id,
                        allocation.contribution_id,
                        allocation.verification_id,
                        allocation.receiver_agent_id,
                        allocation.total as i64,
                        serde_json::to_string(allocation).map_err(|error| error.to_string())?,
                        millis_from_rfc3339(&allocation.issued_at)?,
                    ],
                )
                .map_err(|error| error.to_string())?;
        }
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(allocations)
    }

    pub fn get_contribution(&self, contribution_id: &str) -> Result<NodeContribution, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        contribution_in_connection(&connection, contribution_id)
    }

    pub fn credit_summary(&self, receiver_agent_id: &str) -> Result<CreditSummary, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        let mut statement = connection
            .prepare(
                "SELECT allocation_json FROM credit_allocations
                 WHERE receiver_agent_id = ?1
                 ORDER BY issued_at DESC",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(params![receiver_agent_id], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?;
        let allocations = rows
            .map(|row| {
                let json = row.map_err(|error| error.to_string())?;
                serde_json::from_str(&json).map_err(|error| error.to_string())
            })
            .collect::<Result<Vec<CreditAllocation>, String>>()?;
        let total = allocations.iter().map(|allocation| allocation.total).sum();
        Ok(CreditSummary { total, allocations })
    }

    pub fn campaign_report_snapshot(
        &self,
        campaign_id: &str,
    ) -> Result<CampaignReportSnapshot, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        let campaign = campaign_in_connection(&connection, campaign_id)?;
        Ok(CampaignReportSnapshot {
            campaign,
            work_units: work_units_in_connection(&connection, campaign_id)?,
            contributions: contributions_in_connection(&connection, campaign_id)?,
            verifications: verifications_in_connection(&connection, campaign_id)?,
            credits: credits_in_connection(&connection, campaign_id)?,
        })
    }

    pub fn build_worker_receipt(
        &self,
        transaction_id: &str,
        event_root: &str,
        approved: ReceiptApproval,
        keypair: &libp2p::identity::Keypair,
    ) -> Result<AuditReceipt, String> {
        let contract = self.get_contract(transaction_id)?;
        if crate::atp::agent_id(&keypair.public()) != contract.worker_agent_id {
            return Err("only the selected worker can sign the receipt".to_string());
        }
        let result = self.get_execution_result(transaction_id)?;
        let leases = self.get_leases(transaction_id)?;
        verify_execution_result(&result, &contract, &leases)?;
        if approved.by != contract.requester_agent_id {
            return Err("receipt approval is not from the requester".to_string());
        }

        let mut receipt = AuditReceipt {
            receipt_type: "ProofOfCognition".to_string(),
            atp: crate::atp::ATP_VERSION.to_string(),
            profile: crate::audit_profile::AUDIT_RECEIPT_PROFILE.to_string(),
            profile_version: crate::audit_profile::AUDIT_PROFILE_VERSION.to_string(),
            transaction_id: transaction_id.to_string(),
            requested: crate::audit_profile::ReceiptRequested {
                contract_hash: contract_hash(&contract)?,
                repository: contract.repository.clone(),
                scope: contract.scope.clone(),
            },
            accessed: crate::audit_profile::ReceiptAccessed {
                leases: result.lease_ids.clone(),
                resources: vec![format!(
                    "github:{}@{}",
                    contract.repository.full_name, contract.repository.commit_sha
                )],
            },
            changed: crate::audit_profile::ReceiptChanged {
                artifacts: result
                    .artifacts
                    .iter()
                    .map(|artifact| artifact.receipt_record())
                    .collect(),
                external_state: "none".to_string(),
            },
            approved,
            paid: contract.settlement.clone(),
            event_root: event_root.to_string(),
            receipt_hash: String::new(),
            signatures: Vec::new(),
        };
        receipt.receipt_hash = crate::audit_profile::receipt_hash(&receipt)?;
        let signature = crate::atp::sign_canonical(keypair, &receipt_signature_value(&receipt)?)?;
        receipt
            .signatures
            .push(crate::audit_profile::ReceiptSignature {
                signature_type: "Ed25519".to_string(),
                signer: contract.worker_agent_id.clone(),
                kid: format!("{}#identity", contract.worker_agent_id),
                signature,
            });
        validate_receipt(&receipt).map_err(|error| error.to_string())?;
        validate_receipt_parties(&receipt, &contract).map_err(|error| error.to_string())?;
        Ok(receipt)
    }

    pub fn set_bundle_path(&self, transaction_id: &str, path: &str) -> Result<(), String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        connection
            .execute(
                "UPDATE audit_receipts SET bundle_path = ?2 WHERE transaction_id = ?1",
                params![transaction_id, path],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn transaction_envelopes(&self, transaction_id: &str) -> Result<Vec<AtpEnvelope>, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        let mut statement = connection
            .prepare(
                "SELECT envelope_json FROM atp_events
                 WHERE transaction_id = ?1 ORDER BY sequence",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(params![transaction_id], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?;
        rows.map(|row| {
            let value = row.map_err(|error| error.to_string())?;
            serde_json::from_str(&value).map_err(|error| error.to_string())
        })
        .collect()
    }

    pub fn commit_envelope(
        &self,
        envelope: &AtpEnvelope,
        receiver_agent_id: &str,
        source_peer_id: Option<&str>,
    ) -> Result<AtpAck, String> {
        verify_envelope(envelope)?;
        if let Some(source_peer_id) = source_peer_id {
            let expected = format!("urn:libp2p:{source_peer_id}");
            if envelope.issuer != expected {
                return Err(
                    "ATP issuer does not match the authenticated transport peer".to_string()
                );
            }
        }

        let hash = event_hash(envelope)?;
        let mut connection = self.connection.lock().map_err(|error| error.to_string())?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;

        if let Some(state) = transaction
            .query_row(
                "SELECT state FROM atp_events WHERE event_hash = ?1",
                params![hash],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?
        {
            return Ok(AtpAck {
                accepted: true,
                duplicate: true,
                event_hash: hash,
                transaction_id: envelope.transaction_id.clone(),
                state: Some(state),
                receiver_agent_id: receiver_agent_id.to_string(),
                committed_at: now_rfc3339(),
                reason_code: None,
                reason: None,
            });
        }

        ensure_not_replayed(&transaction, envelope)?;
        let current = transaction_context_in(&transaction, &envelope.transaction_id)?;
        let expected_prev = current
            .last_event_hash
            .clone()
            .unwrap_or_else(|| ATP_GENESIS_HASH.to_string());
        if envelope.prev.as_deref() != Some(expected_prev.as_str()) {
            return Err("ATP event does not extend the committed transaction head".to_string());
        }

        let next_state = transition(current.state.as_deref(), envelope.verb)?;
        let audit_body = if envelope.verb == AtpVerb::Attest {
            None
        } else {
            let body: AuditEventBody =
                serde_json::from_value(envelope.body.clone()).map_err(|error| error.to_string())?;
            validate_body(&transaction, &body, envelope, &current)?;
            Some(body)
        };
        let receipt = if envelope.verb == AtpVerb::Attest {
            let receipt: AuditReceipt =
                serde_json::from_value(envelope.body.clone()).map_err(|error| error.to_string())?;
            validate_attestation(&transaction, &receipt, envelope, &current)?;
            Some(receipt)
        } else {
            None
        };
        let sequence = next_sequence(&transaction, &envelope.transaction_id)?;
        let envelope_json = serde_json::to_string(envelope).map_err(|error| error.to_string())?;

        transaction
            .execute(
                "INSERT INTO replay_nonces (issuer, nonce, event_hash) VALUES (?1, ?2, ?3)",
                params![envelope.issuer, envelope.nonce, hash],
            )
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "INSERT INTO idempotency_keys (issuer, idempotency_key, event_hash)
                 VALUES (?1, ?2, ?3)",
                params![envelope.issuer, envelope.idempotency_key, hash],
            )
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "INSERT INTO atp_events
                    (event_hash, transaction_id, sequence, verb, issuer, audience, prev,
                     state, envelope_json, committed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    hash,
                    envelope.transaction_id,
                    sequence,
                    envelope.verb.as_str(),
                    envelope.issuer,
                    envelope.audience,
                    envelope.prev,
                    next_state,
                    envelope_json,
                    now_millis() as i64,
                ],
            )
            .map_err(|error| error.to_string())?;

        if let Some(body) = audit_body.as_ref() {
            apply_audit_event(
                &transaction,
                body,
                envelope,
                &hash,
                next_state,
                receiver_agent_id,
            )?;
        }
        if let Some(receipt) = receipt.as_ref() {
            apply_attestation(&transaction, receipt, envelope, &hash, next_state)?;
        }
        transaction.commit().map_err(|error| error.to_string())?;

        Ok(AtpAck {
            accepted: true,
            duplicate: false,
            event_hash: hash,
            transaction_id: envelope.transaction_id.clone(),
            state: Some(next_state.to_string()),
            receiver_agent_id: receiver_agent_id.to_string(),
            committed_at: now_rfc3339(),
            reason_code: None,
            reason: None,
        })
    }

    pub fn mark_delivery(&self, peer_id: &str, ack: &AtpAck) -> Result<(), String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        connection
            .execute(
                "INSERT INTO deliveries
                    (event_hash, transaction_id, peer_id, accepted, duplicate, reason_code, reason,
                     updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(event_hash, peer_id) DO UPDATE SET
                    accepted = excluded.accepted,
                    duplicate = excluded.duplicate,
                    reason_code = excluded.reason_code,
                    reason = excluded.reason,
                    updated_at = excluded.updated_at",
                params![
                    ack.event_hash,
                    ack.transaction_id,
                    peer_id,
                    ack.accepted,
                    ack.duplicate,
                    ack.reason_code,
                    ack.reason,
                    now_millis() as i64,
                ],
            )
            .map_err(|error| error.to_string())?;

        if ack.accepted {
            connection
                .execute(
                    "UPDATE audit_jobs
                     SET delivery_state = 'acknowledged', updated_at = ?2
                     WHERE transaction_id = ?1 AND origin = 'local'",
                    params![ack.transaction_id, now_millis() as i64],
                )
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    pub fn envelopes_for_peer(
        &self,
        local_agent_id: &str,
        peer_agent_id: &str,
    ) -> Result<Vec<AtpEnvelope>, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        let mut statement = connection
            .prepare(
                "SELECT envelope_json
                 FROM atp_events
                 WHERE issuer = ?1 AND (audience IS NULL OR audience = ?2)
                 ORDER BY committed_at, sequence",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(params![local_agent_id, peer_agent_id], |row| {
                row.get::<_, String>(0)
            })
            .map_err(|error| error.to_string())?;

        rows.map(|row| {
            let value = row.map_err(|error| error.to_string())?;
            serde_json::from_str(&value).map_err(|error| error.to_string())
        })
        .collect()
    }

    fn initialize(&self) -> Result<(), String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS atp_events (
                    event_hash TEXT PRIMARY KEY,
                    transaction_id TEXT NOT NULL,
                    sequence INTEGER NOT NULL,
                    verb TEXT NOT NULL,
                    issuer TEXT NOT NULL,
                    audience TEXT,
                    prev TEXT,
                    state TEXT NOT NULL,
                    envelope_json TEXT NOT NULL,
                    committed_at INTEGER NOT NULL,
                    UNIQUE(transaction_id, sequence)
                 );
                 CREATE INDEX IF NOT EXISTS atp_events_transaction
                    ON atp_events(transaction_id, sequence);

                 CREATE TABLE IF NOT EXISTS replay_nonces (
                    issuer TEXT NOT NULL,
                    nonce TEXT NOT NULL,
                    event_hash TEXT NOT NULL,
                    PRIMARY KEY(issuer, nonce)
                 );

                 CREATE TABLE IF NOT EXISTS idempotency_keys (
                    issuer TEXT NOT NULL,
                    idempotency_key TEXT NOT NULL,
                    event_hash TEXT NOT NULL,
                    PRIMARY KEY(issuer, idempotency_key)
                 );

                 CREATE TABLE IF NOT EXISTS audit_jobs (
                    id TEXT PRIMARY KEY,
                    transaction_id TEXT NOT NULL UNIQUE,
                    repository_json TEXT NOT NULL,
                    compensation TEXT NOT NULL,
                    currency TEXT NOT NULL,
                    scope_json TEXT NOT NULL,
                    status TEXT NOT NULL,
                    delivery_state TEXT NOT NULL,
                    requester_agent_id TEXT NOT NULL,
                    worker_agent_id TEXT,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    last_event_hash TEXT NOT NULL,
                    origin TEXT NOT NULL
                 );

                 CREATE TABLE IF NOT EXISTS audit_contracts (
                    transaction_id TEXT PRIMARY KEY,
                    profile TEXT NOT NULL,
                    contract_hash TEXT NOT NULL UNIQUE,
                    contract_json TEXT NOT NULL,
                    accepted INTEGER NOT NULL DEFAULT 0,
                    created_at INTEGER NOT NULL,
                    accepted_at INTEGER
                 );

                 CREATE TABLE IF NOT EXISTS audit_leases (
                    transaction_id TEXT NOT NULL,
                    lease_id TEXT PRIMARY KEY,
                    lease_json TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                 );

                 CREATE TABLE IF NOT EXISTS audit_execution_results (
                    transaction_id TEXT PRIMARY KEY,
                    result_hash TEXT NOT NULL UNIQUE,
                    result_json TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                 );

                 CREATE TABLE IF NOT EXISTS audit_receipts (
                    transaction_id TEXT PRIMARY KEY,
                    receipt_hash TEXT NOT NULL UNIQUE,
                    receipt_json TEXT NOT NULL,
                    bundle_path TEXT,
                    created_at INTEGER NOT NULL
                 );

                 CREATE TABLE IF NOT EXISTS deliveries (
                    event_hash TEXT NOT NULL,
                    transaction_id TEXT NOT NULL,
                    peer_id TEXT NOT NULL,
                    accepted INTEGER NOT NULL,
                    duplicate INTEGER NOT NULL,
                    reason_code TEXT,
                    reason TEXT,
                    updated_at INTEGER NOT NULL,
                    PRIMARY KEY(event_hash, peer_id)
                 );

                 CREATE TABLE IF NOT EXISTS protocol_audit_campaigns (
                    campaign_id TEXT PRIMARY KEY,
                    campaign_json TEXT NOT NULL,
                    status TEXT NOT NULL,
                    requester_agent_id TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL
                 );

                 CREATE TABLE IF NOT EXISTS audit_work_units (
                    work_unit_id TEXT PRIMARY KEY,
                    campaign_id TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    status TEXT NOT NULL,
                    work_unit_json TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS audit_work_units_campaign
                    ON audit_work_units(campaign_id, created_at);

                 CREATE TABLE IF NOT EXISTS audit_contributions (
                    contribution_id TEXT PRIMARY KEY,
                    campaign_id TEXT NOT NULL,
                    work_unit_id TEXT NOT NULL,
                    worker_agent_id TEXT NOT NULL,
                    receipt_hash TEXT NOT NULL UNIQUE,
                    contribution_json TEXT NOT NULL,
                    status TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS audit_contributions_campaign
                    ON audit_contributions(campaign_id, created_at);

                 CREATE TABLE IF NOT EXISTS audit_verifications (
                    verification_id TEXT PRIMARY KEY,
                    campaign_id TEXT NOT NULL,
                    target_contribution_id TEXT NOT NULL,
                    verifier_agent_id TEXT NOT NULL,
                    decision TEXT NOT NULL,
                    verification_json TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS audit_verifications_campaign
                    ON audit_verifications(campaign_id, created_at);

                 CREATE TABLE IF NOT EXISTS credit_allocations (
                    allocation_id TEXT PRIMARY KEY,
                    campaign_id TEXT NOT NULL,
                    contribution_id TEXT NOT NULL,
                    verification_id TEXT NOT NULL,
                    receiver_agent_id TEXT NOT NULL,
                    total INTEGER NOT NULL,
                    allocation_json TEXT NOT NULL,
                    issued_at INTEGER NOT NULL
                 );",
            )
            .map_err(|error| error.to_string())?;

        let has_delivery_reason = {
            let mut statement = connection
                .prepare("PRAGMA table_info(deliveries)")
                .map_err(|error| error.to_string())?;
            let columns = statement
                .query_map([], |row| row.get::<_, String>(1))
                .map_err(|error| error.to_string())?;
            columns
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| error.to_string())?
                .iter()
                .any(|column| column == "reason")
        };
        if !has_delivery_reason {
            connection
                .execute("ALTER TABLE deliveries ADD COLUMN reason TEXT", [])
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

pub fn rejection_ack(envelope: &AtpEnvelope, receiver_agent_id: &str, reason: String) -> AtpAck {
    let reason_code = reason
        .split_once(':')
        .map(|(prefix, _)| prefix)
        .filter(|prefix| prefix.starts_with("ATP_"))
        .unwrap_or("ATP_VALIDATION_FAILED");
    AtpAck {
        accepted: false,
        duplicate: false,
        event_hash: event_hash(envelope).unwrap_or_default(),
        transaction_id: envelope.transaction_id.clone(),
        state: None,
        receiver_agent_id: receiver_agent_id.to_string(),
        committed_at: now_rfc3339(),
        reason_code: Some(reason_code.to_string()),
        reason: Some(reason),
    }
}

fn ensure_not_replayed(
    transaction: &Transaction<'_>,
    envelope: &AtpEnvelope,
) -> Result<(), String> {
    let nonce_exists = transaction
        .query_row(
            "SELECT 1 FROM replay_nonces WHERE issuer = ?1 AND nonce = ?2",
            params![envelope.issuer, envelope.nonce],
            |_| Ok(()),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .is_some();
    if nonce_exists {
        return Err("ATP nonce has already been committed".to_string());
    }

    let idempotency_exists = transaction
        .query_row(
            "SELECT 1 FROM idempotency_keys WHERE issuer = ?1 AND idempotency_key = ?2",
            params![envelope.issuer, envelope.idempotency_key],
            |_| Ok(()),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .is_some();
    if idempotency_exists {
        return Err("ATP idempotency key has already been committed".to_string());
    }
    Ok(())
}

fn transaction_context_in(
    transaction: &Transaction<'_>,
    transaction_id: &str,
) -> Result<TransactionContext, String> {
    transaction
        .query_row(
            "SELECT status, last_event_hash, requester_agent_id, worker_agent_id,
                    repository_json, scope_json, compensation, currency,
                    (SELECT contract_hash FROM audit_contracts c
                     WHERE c.transaction_id = audit_jobs.transaction_id),
                    (SELECT result_hash FROM audit_execution_results r
                     WHERE r.transaction_id = audit_jobs.transaction_id)
             FROM audit_jobs
             WHERE transaction_id = ?1",
            params![transaction_id],
            |row| {
                let repository_json: String = row.get(4)?;
                let scope_json: String = row.get(5)?;
                let repository = serde_json::from_str(&repository_json).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        4,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?;
                let scope = serde_json::from_str(&scope_json).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        5,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?;
                Ok(TransactionContext {
                    state: Some(row.get(0)?),
                    last_event_hash: Some(row.get(1)?),
                    requester_agent_id: Some(row.get(2)?),
                    worker_agent_id: row.get(3)?,
                    repository: Some(repository),
                    scope: Some(scope),
                    compensation: Some(row.get(6)?),
                    currency: Some(row.get(7)?),
                    contract_hash: row.get(8)?,
                    result_hash: row.get(9)?,
                })
            },
        )
        .optional()
        .map(|context| {
            context.unwrap_or(TransactionContext {
                state: None,
                last_event_hash: None,
                requester_agent_id: None,
                worker_agent_id: None,
                repository: None,
                scope: None,
                compensation: None,
                currency: None,
                contract_hash: None,
                result_hash: None,
            })
        })
        .map_err(|error| error.to_string())
}

fn next_sequence(transaction: &Transaction<'_>, transaction_id: &str) -> Result<i64, String> {
    transaction
        .query_row(
            "SELECT COALESCE(MAX(sequence), -1) + 1 FROM atp_events WHERE transaction_id = ?1",
            params![transaction_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())
}

fn validate_body(
    transaction: &Transaction<'_>,
    body: &AuditEventBody,
    envelope: &AtpEnvelope,
    current: &TransactionContext,
) -> Result<(), String> {
    match body {
        AuditEventBody::Announce { job } => {
            if envelope.verb != AtpVerb::Discover {
                return Err("Audit announcement must use ATP DISCOVER".to_string());
            }
            if job.id != envelope.transaction_id {
                return Err("Audit job id must equal the ATP transaction id".to_string());
            }
            if job.requester_agent_id != envelope.issuer {
                return Err("Audit requester must be the ATP issuer".to_string());
            }
            if job.repository.is_private {
                return Err("Private repositories are not supported".to_string());
            }
            if !crate::audit_profile::is_git_commit_sha(&job.repository.commit_sha) {
                return Err(
                    "ATP_BAD_STATE: AUDIT_REQUEST_REPOSITORY_UNPINNED: audit requests must pin an exact repository commit"
                        .to_string(),
                );
            }
        }
        AuditEventBody::WorkerOffer {
            job_id,
            worker_agent_id,
            contract,
        } => {
            if envelope.verb != AtpVerb::Negotiate || job_id != &envelope.transaction_id {
                return Err(
                    "Worker offer must negotiate the existing audit transaction".to_string()
                );
            }
            if worker_agent_id != &envelope.issuer {
                return Err("Worker offer must be issued by the proposed worker".to_string());
            }
            if envelope.audience.as_deref() != current.requester_agent_id.as_deref() {
                return Err(
                    "ATP_BAD_STATE: AUDIT_CONTRACT_AUDIENCE_MISMATCH: worker offer must target the requester"
                        .to_string(),
                );
            }
            if current.requester_agent_id.as_deref() == Some(worker_agent_id) {
                return Err("The requester cannot offer to fulfill its own audit".to_string());
            }
            validate_contract(contract).map_err(|error| error.to_string())?;
            if contract.transaction_id != envelope.transaction_id
                || contract.worker_agent_id != *worker_agent_id
                || current.requester_agent_id.as_deref()
                    != Some(contract.requester_agent_id.as_str())
            {
                return Err(
                    "ATP_BAD_STATE: AUDIT_CONTRACT_PARTY_MISMATCH: contract parties must match the committed transaction"
                        .to_string(),
                );
            }
            let repository = current.repository.as_ref().ok_or_else(|| {
                "ATP_BAD_STATE: AUDIT_CONTRACT_REQUEST_MISSING: committed request is unavailable"
                    .to_string()
            })?;
            let expected_repository = RepositoryTarget {
                full_name: repository.full_name.clone(),
                url: repository.url.clone(),
                commit_sha: repository.commit_sha.clone(),
            };
            if contract.repository != expected_repository
                || current.scope.as_ref() != Some(&contract.scope)
                || current.compensation.as_deref()
                    != Some(contract.proposed_compensation.amount.as_str())
                || current.currency.as_deref()
                    != Some(contract.proposed_compensation.asset.as_str())
            {
                return Err(
                    "ATP_BAD_STATE: AUDIT_CONTRACT_REQUEST_MISMATCH: contract must preserve the requested repository, commit, scope, and proposed compensation"
                        .to_string(),
                );
            }
            if envelope.expires_at.as_deref() != Some(contract.expires_at.as_str()) {
                return Err(
                    "ATP_BAD_STATE: AUDIT_CONTRACT_EXPIRY_MISMATCH: offer and contract expiry must match"
                        .to_string(),
                );
            }
        }
        AuditEventBody::WorkerSelected {
            job_id,
            worker_agent_id,
            contract_hash,
        } => {
            if envelope.verb != AtpVerb::Negotiate || job_id != &envelope.transaction_id {
                return Err(
                    "Worker selection must negotiate the existing audit transaction".to_string(),
                );
            }
            if current.requester_agent_id.as_deref() != Some(envelope.issuer.as_str()) {
                return Err("Only the requester can select a worker".to_string());
            }
            if envelope.audience.as_deref() != Some(worker_agent_id.as_str()) {
                return Err(
                    "ATP_BAD_STATE: AUDIT_CONTRACT_AUDIENCE_MISMATCH: worker selection must target the offered worker"
                        .to_string(),
                );
            }
            if current.worker_agent_id.as_deref() != Some(worker_agent_id) {
                return Err("Selected worker does not match the committed offer".to_string());
            }
            if current.contract_hash.as_deref() != Some(contract_hash.as_str()) {
                return Err(
                    "ATP_BAD_STATE: AUDIT_CONTRACT_HASH_MISMATCH: selection must bind the offered contract hash"
                        .to_string(),
                );
            }
        }
        AuditEventBody::RouteAudit {
            job_id,
            contract_hash,
            leases,
        } => {
            if envelope.verb != AtpVerb::Route || job_id != &envelope.transaction_id {
                return Err("Audit route must extend the negotiated transaction".to_string());
            }
            if current.requester_agent_id.as_deref() != Some(envelope.issuer.as_str()) {
                return Err("Only the requester can route the accepted audit".to_string());
            }
            if envelope.audience.as_deref() != current.worker_agent_id.as_deref() {
                return Err("ATP_BAD_STATE: AUDIT_ROUTE_AUDIENCE_MISMATCH".to_string());
            }
            if current.contract_hash.as_deref() != Some(contract_hash.as_str()) {
                return Err("ATP_BAD_STATE: AUDIT_ROUTE_CONTRACT_MISMATCH".to_string());
            }
            let contract = contract_in(transaction, &envelope.transaction_id)?;
            let requester_key = envelope_public_key(envelope)?;
            crate::worker::verify_leases(leases, &requester_key, &contract)
                .map_err(|reason| format!("ATP_LEASE_DENIED: {reason}"))?;
        }
        AuditEventBody::SettlementApproved {
            job_id,
            contract_hash,
            result_hash,
            approved,
        } => {
            if envelope.verb != AtpVerb::Settle || job_id != &envelope.transaction_id {
                return Err("Audit settlement must extend the routed transaction".to_string());
            }
            if current.requester_agent_id.as_deref() != Some(envelope.issuer.as_str())
                || approved.by != envelope.issuer
                || approved.method != "requester-verified-result"
            {
                return Err("Only the requester can approve the verified result".to_string());
            }
            if envelope.audience.as_deref() != current.worker_agent_id.as_deref()
                || current.contract_hash.as_deref() != Some(contract_hash.as_str())
                || current.result_hash.as_deref() != Some(result_hash.as_str())
            {
                return Err("ATP_BAD_STATE: AUDIT_SETTLEMENT_BINDING_MISMATCH".to_string());
            }
            DateTime::parse_from_rfc3339(&approved.time)
                .map_err(|_| "Audit approval time must be RFC3339".to_string())?;
            let contract = contract_in(transaction, &envelope.transaction_id)?;
            let result = execution_result_in(transaction, &envelope.transaction_id)?;
            let leases = leases_in(transaction, &envelope.transaction_id)?;
            verify_execution_result(&result, &contract, &leases)
                .map_err(|reason| format!("ATP_PROOF_UNSATISFIED: {reason}"))?;
        }
    }
    Ok(())
}

fn validate_attestation(
    transaction: &Transaction<'_>,
    receipt: &AuditReceipt,
    envelope: &AtpEnvelope,
    current: &TransactionContext,
) -> Result<(), String> {
    validate_receipt(receipt).map_err(|error| error.to_string())?;
    let contract = contract_in(transaction, &envelope.transaction_id)?;
    validate_receipt_parties(receipt, &contract).map_err(|error| error.to_string())?;
    if envelope.issuer != contract.worker_agent_id
        || envelope.audience.as_deref() != Some(contract.requester_agent_id.as_str())
        || receipt.event_root != current.last_event_hash.as_deref().unwrap_or_default()
    {
        return Err("ATP_PROOF_UNSATISFIED: AUDIT_RECEIPT_EVENT_BINDING_INVALID".to_string());
    }
    let result = execution_result_in(transaction, &envelope.transaction_id)?;
    let public_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(&result.public_key_base64_url)
        .map_err(|_| "worker receipt key is not valid base64url".to_string())?;
    let public_key = crate::atp::public_key_from_raw_ed25519(&public_bytes)?;
    let worker_signature = receipt
        .signatures
        .iter()
        .find(|signature| signature.signer == contract.worker_agent_id)
        .ok_or_else(|| "worker receipt signature missing".to_string())?;
    crate::atp::verify_canonical(
        &public_key,
        &receipt_signature_value(receipt)?,
        &worker_signature.signature,
    )?;
    Ok(())
}

fn apply_attestation(
    transaction: &Transaction<'_>,
    receipt: &AuditReceipt,
    envelope: &AtpEnvelope,
    hash: &str,
    next_state: &str,
) -> Result<(), String> {
    transaction
        .execute(
            "INSERT INTO audit_receipts
                (transaction_id, receipt_hash, receipt_json, bundle_path, created_at)
             VALUES (?1, ?2, ?3, NULL, ?4)
             ON CONFLICT(transaction_id) DO UPDATE SET
                receipt_hash = excluded.receipt_hash,
                receipt_json = excluded.receipt_json,
                created_at = excluded.created_at",
            params![
                envelope.transaction_id,
                receipt.receipt_hash,
                serde_json::to_string(receipt).map_err(|error| error.to_string())?,
                now_millis() as i64,
            ],
        )
        .map_err(|error| error.to_string())?;
    update_job_state(transaction, envelope, hash, next_state)
}

fn envelope_public_key(envelope: &AtpEnvelope) -> Result<libp2p::identity::PublicKey, String> {
    use base64::Engine as _;
    let proof = envelope
        .proofs
        .first()
        .ok_or_else(|| "ATP envelope proof missing".to_string())?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(&proof.public_key)
        .map_err(|_| "ATP proof public key is not valid base64url".to_string())?;
    libp2p::identity::PublicKey::try_decode_protobuf(&bytes)
        .map_err(|_| "ATP proof public key is invalid".to_string())
}

fn contract_in(
    transaction: &Transaction<'_>,
    transaction_id: &str,
) -> Result<AuditContract, String> {
    let json = transaction
        .query_row(
            "SELECT contract_json FROM audit_contracts WHERE transaction_id = ?1",
            params![transaction_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(|error| error.to_string())?;
    serde_json::from_str(&json).map_err(|error| error.to_string())
}

fn execution_result_in(
    transaction: &Transaction<'_>,
    transaction_id: &str,
) -> Result<SignedExecutionResult, String> {
    let json = transaction
        .query_row(
            "SELECT result_json FROM audit_execution_results WHERE transaction_id = ?1",
            params![transaction_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(|error| error.to_string())?;
    serde_json::from_str(&json).map_err(|error| error.to_string())
}

fn leases_in(
    transaction: &Transaction<'_>,
    transaction_id: &str,
) -> Result<Vec<ContextLease>, String> {
    let mut statement = transaction
        .prepare(
            "SELECT lease_json FROM audit_leases
             WHERE transaction_id = ?1 ORDER BY lease_id",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![transaction_id], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?;
    rows.map(|row| {
        let json = row.map_err(|error| error.to_string())?;
        serde_json::from_str(&json).map_err(|error| error.to_string())
    })
    .collect()
}

fn contract_in_connection(
    connection: &Connection,
    transaction_id: &str,
) -> Result<AuditContract, String> {
    let json = connection
        .query_row(
            "SELECT contract_json FROM audit_contracts WHERE transaction_id = ?1",
            params![transaction_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(|error| error.to_string())?;
    serde_json::from_str(&json).map_err(|error| error.to_string())
}

fn execution_result_in_connection(
    connection: &Connection,
    transaction_id: &str,
) -> Result<SignedExecutionResult, String> {
    let json = connection
        .query_row(
            "SELECT result_json FROM audit_execution_results WHERE transaction_id = ?1",
            params![transaction_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(|error| error.to_string())?;
    serde_json::from_str(&json).map_err(|error| error.to_string())
}

fn leases_in_connection(
    connection: &Connection,
    transaction_id: &str,
) -> Result<Vec<ContextLease>, String> {
    let mut statement = connection
        .prepare(
            "SELECT lease_json FROM audit_leases
             WHERE transaction_id = ?1 ORDER BY lease_id",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![transaction_id], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?;
    rows.map(|row| {
        let json = row.map_err(|error| error.to_string())?;
        serde_json::from_str(&json).map_err(|error| error.to_string())
    })
    .collect()
}

fn apply_audit_event(
    transaction: &Transaction<'_>,
    body: &AuditEventBody,
    envelope: &AtpEnvelope,
    hash: &str,
    next_state: &str,
    receiver_agent_id: &str,
) -> Result<(), String> {
    match body {
        AuditEventBody::Announce { job } => {
            let repository_json =
                serde_json::to_string(&job.repository).map_err(|error| error.to_string())?;
            let scope_json =
                serde_json::to_string(&job.scope).map_err(|error| error.to_string())?;
            let origin = if envelope.issuer == receiver_agent_id {
                "local"
            } else {
                "remote"
            };
            let delivery_state = if origin == "local" {
                "queued"
            } else {
                "received"
            };
            transaction
                .execute(
                    "INSERT INTO audit_jobs
                        (id, transaction_id, repository_json, compensation, currency, scope_json,
                         status, delivery_state, requester_agent_id, worker_agent_id, created_at,
                         updated_at, last_event_hash, origin)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, ?10, ?11, ?12, ?13)",
                    params![
                        job.id,
                        envelope.transaction_id,
                        repository_json,
                        job.compensation,
                        job.currency,
                        scope_json,
                        next_state,
                        delivery_state,
                        job.requester_agent_id,
                        job.created_at as i64,
                        now_millis() as i64,
                        hash,
                        origin,
                    ],
                )
                .map_err(|error| error.to_string())?;
            materialize_campaign_for_job(transaction, job)?;
        }
        AuditEventBody::WorkerOffer {
            worker_agent_id,
            contract,
            ..
        } => {
            let canonical_contract_hash = contract_hash(contract)?;
            let contract_json =
                serde_json::to_string(contract).map_err(|error| error.to_string())?;
            transaction
                .execute(
                    "INSERT INTO audit_contracts
                        (transaction_id, profile, contract_hash, contract_json, accepted, created_at)
                     VALUES (?1, ?2, ?3, ?4, 0, ?5)",
                    params![
                        envelope.transaction_id,
                        contract.profile,
                        canonical_contract_hash,
                        contract_json,
                        now_millis() as i64,
                    ],
                )
                .map_err(|error| error.to_string())?;
            transaction
                .execute(
                    "UPDATE audit_jobs
                     SET status = ?2, worker_agent_id = ?3, last_event_hash = ?4, updated_at = ?5
                     WHERE transaction_id = ?1",
                    params![
                        envelope.transaction_id,
                        next_state,
                        worker_agent_id,
                        hash,
                        now_millis() as i64,
                    ],
                )
                .map_err(|error| error.to_string())?;
        }
        AuditEventBody::WorkerSelected { contract_hash, .. } => {
            transaction
                .execute(
                    "UPDATE audit_contracts
                     SET accepted = 1, accepted_at = ?3
                     WHERE transaction_id = ?1 AND contract_hash = ?2",
                    params![envelope.transaction_id, contract_hash, now_millis() as i64,],
                )
                .map_err(|error| error.to_string())?;
            transaction
                .execute(
                    "UPDATE audit_jobs
                     SET status = ?2, last_event_hash = ?3, updated_at = ?4
                     WHERE transaction_id = ?1",
                    params![
                        envelope.transaction_id,
                        next_state,
                        hash,
                        now_millis() as i64,
                    ],
                )
                .map_err(|error| error.to_string())?;
        }
        AuditEventBody::RouteAudit { leases, .. } => {
            for lease in leases {
                transaction
                    .execute(
                        "INSERT INTO audit_leases
                            (transaction_id, lease_id, lease_json, created_at)
                         VALUES (?1, ?2, ?3, ?4)",
                        params![
                            envelope.transaction_id,
                            lease.id,
                            serde_json::to_string(lease).map_err(|error| error.to_string())?,
                            now_millis() as i64,
                        ],
                    )
                    .map_err(|error| error.to_string())?;
            }
            update_job_state(transaction, envelope, hash, next_state)?;
        }
        AuditEventBody::SettlementApproved { .. } => {
            update_job_state(transaction, envelope, hash, next_state)?;
        }
    }
    Ok(())
}

fn materialize_campaign_for_job(
    transaction: &Transaction<'_>,
    job: &AuditJobPayload,
) -> Result<(), String> {
    let campaign_id = campaign_id_for_transaction(&job.id);
    let exists = transaction
        .query_row(
            "SELECT 1 FROM protocol_audit_campaigns WHERE campaign_id = ?1",
            params![campaign_id],
            |_| Ok(()),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .is_some();
    if exists {
        return Ok(());
    }

    let created_at = rfc3339_from_millis(job.created_at);
    let mut campaign = ProtocolAuditCampaign::new(
        protocol_name_from_repository(&job.repository.full_name),
        RepositoryTarget {
            full_name: job.repository.full_name.clone(),
            url: job.repository.url.clone(),
            commit_sha: job.repository.commit_sha.clone(),
        },
        job.scope.join("\n"),
        None,
        vec![
            "Evidence-backed repository risk".to_string(),
            "Reportable security impact if proven".to_string(),
        ],
        vec![
            "Best-practice-only notes".to_string(),
            "Claims without reproducible evidence".to_string(),
            "Production testing or unauthorized external interaction".to_string(),
        ],
        Some(format!(
            "ATP transaction: {}. ATP Credits budget: {} {}.",
            job.id, job.compensation, job.currency
        )),
        job.requester_agent_id.clone(),
    )?;
    campaign.campaign_id = campaign_id;
    campaign.created_at = created_at.clone();
    campaign.updated_at = created_at;
    validate_campaign(&campaign)?;
    transaction
        .execute(
            "INSERT INTO protocol_audit_campaigns
                (campaign_id, campaign_json, status, requester_agent_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                campaign.campaign_id,
                serde_json::to_string(&campaign).map_err(|error| error.to_string())?,
                campaign.status,
                campaign.requester_agent_id,
                millis_from_rfc3339(&campaign.created_at)?,
                millis_from_rfc3339(&campaign.updated_at)?,
            ],
        )
        .map_err(|error| error.to_string())?;
    for work_unit in default_work_units(&campaign) {
        insert_work_unit(transaction, &work_unit)?;
    }
    Ok(())
}

fn protocol_name_from_repository(full_name: &str) -> String {
    full_name
        .split('/')
        .next()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(full_name)
        .to_string()
}

fn rfc3339_from_millis(millis: u64) -> String {
    Utc.timestamp_millis_opt(millis as i64)
        .single()
        .unwrap_or_else(Utc::now)
        .to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn update_job_state(
    transaction: &Transaction<'_>,
    envelope: &AtpEnvelope,
    hash: &str,
    next_state: &str,
) -> Result<(), String> {
    transaction
        .execute(
            "UPDATE audit_jobs
             SET status = ?2, last_event_hash = ?3, updated_at = ?4
             WHERE transaction_id = ?1",
            params![
                envelope.transaction_id,
                next_state,
                hash,
                now_millis() as i64,
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn row_to_job(row: &rusqlite::Row<'_>) -> rusqlite::Result<AuditJob> {
    let repository_json: String = row.get(2)?;
    let scope_json: String = row.get(5)?;
    let repository = serde_json::from_str(&repository_json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let scope = serde_json::from_str(&scope_json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(error))
    })?;

    Ok(AuditJob {
        id: row.get(0)?,
        transaction_id: row.get(1)?,
        repository,
        compensation: row.get(3)?,
        currency: row.get(4)?,
        scope,
        status: row.get(6)?,
        delivery_state: row.get(7)?,
        requester_agent_id: row.get(8)?,
        worker_agent_id: row.get(9)?,
        created_at: row.get::<_, i64>(10)? as u64,
        updated_at: row.get::<_, i64>(11)? as u64,
        last_event_hash: row.get(12)?,
        contract_hash: row.get(13)?,
        result_hash: row.get(14)?,
        receipt_hash: row.get(15)?,
        bundle_path: row.get(16)?,
        origin: row.get(17)?,
        acknowledged_peers: row.get::<_, i64>(18)? as u64,
    })
}

fn insert_work_unit(
    transaction: &Transaction<'_>,
    work_unit: &AuditWorkUnit,
) -> Result<(), String> {
    transaction
        .execute(
            "INSERT INTO audit_work_units
                (work_unit_id, campaign_id, kind, status, work_unit_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                work_unit.work_unit_id,
                work_unit.campaign_id,
                work_unit.kind,
                work_unit.status,
                serde_json::to_string(work_unit).map_err(|error| error.to_string())?,
                millis_from_rfc3339(&work_unit.created_at)?,
                millis_from_rfc3339(&work_unit.created_at)?,
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn update_work_unit_status(
    connection: &Connection,
    campaign_id: &str,
    work_unit_id: &str,
    status: &str,
) -> Result<(), String> {
    let json = connection
        .query_row(
            "SELECT work_unit_json FROM audit_work_units
             WHERE campaign_id = ?1 AND work_unit_id = ?2",
            params![campaign_id, work_unit_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(|error| error.to_string())?;
    let mut work_unit: AuditWorkUnit =
        serde_json::from_str(&json).map_err(|error| error.to_string())?;
    work_unit.status = status.to_string();
    connection
        .execute(
            "UPDATE audit_work_units
             SET status = ?3, work_unit_json = ?4, updated_at = ?5
             WHERE campaign_id = ?1 AND work_unit_id = ?2",
            params![
                campaign_id,
                work_unit_id,
                status,
                serde_json::to_string(&work_unit).map_err(|error| error.to_string())?,
                now_millis() as i64,
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn campaign_in_connection(
    connection: &Connection,
    campaign_id: &str,
) -> Result<ProtocolAuditCampaign, String> {
    let json = connection
        .query_row(
            "SELECT campaign_json FROM protocol_audit_campaigns WHERE campaign_id = ?1",
            params![campaign_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(|error| error.to_string())?;
    serde_json::from_str(&json).map_err(|error| error.to_string())
}

fn work_units_in_connection(
    connection: &Connection,
    campaign_id: &str,
) -> Result<Vec<AuditWorkUnit>, String> {
    let mut statement = connection
        .prepare(
            "SELECT work_unit_json FROM audit_work_units
             WHERE campaign_id = ?1 ORDER BY created_at, work_unit_id",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![campaign_id], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?;
    rows.map(|row| {
        let json = row.map_err(|error| error.to_string())?;
        serde_json::from_str(&json).map_err(|error| error.to_string())
    })
    .collect()
}

fn contribution_in_connection(
    connection: &Connection,
    contribution_id: &str,
) -> Result<NodeContribution, String> {
    let json = connection
        .query_row(
            "SELECT contribution_json FROM audit_contributions WHERE contribution_id = ?1",
            params![contribution_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(|error| error.to_string())?;
    serde_json::from_str(&json).map_err(|error| error.to_string())
}

fn contributions_in_connection(
    connection: &Connection,
    campaign_id: &str,
) -> Result<Vec<NodeContribution>, String> {
    let mut statement = connection
        .prepare(
            "SELECT contribution_json FROM audit_contributions
             WHERE campaign_id = ?1 ORDER BY created_at, contribution_id",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![campaign_id], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?;
    rows.map(|row| {
        let json = row.map_err(|error| error.to_string())?;
        serde_json::from_str(&json).map_err(|error| error.to_string())
    })
    .collect()
}

fn verifications_in_connection(
    connection: &Connection,
    campaign_id: &str,
) -> Result<Vec<VerificationResult>, String> {
    let mut statement = connection
        .prepare(
            "SELECT verification_json FROM audit_verifications
             WHERE campaign_id = ?1 ORDER BY created_at, verification_id",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![campaign_id], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?;
    rows.map(|row| {
        let json = row.map_err(|error| error.to_string())?;
        serde_json::from_str(&json).map_err(|error| error.to_string())
    })
    .collect()
}

fn credits_in_connection(
    connection: &Connection,
    campaign_id: &str,
) -> Result<Vec<CreditAllocation>, String> {
    let mut statement = connection
        .prepare(
            "SELECT allocation_json FROM credit_allocations
             WHERE campaign_id = ?1 ORDER BY issued_at, allocation_id",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![campaign_id], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?;
    rows.map(|row| {
        let json = row.map_err(|error| error.to_string())?;
        serde_json::from_str(&json).map_err(|error| error.to_string())
    })
    .collect()
}

fn millis_from_rfc3339(value: &str) -> Result<i64, String> {
    DateTime::parse_from_rfc3339(value)
        .map(|time| time.timestamp_millis())
        .map_err(|_| "timestamp must be RFC3339".to_string())
}

pub fn now_millis() -> u64 {
    chrono::Utc::now().timestamp_millis() as u64
}

pub fn data_dir() -> Result<PathBuf, String> {
    if let Ok(data_dir) = std::env::var("CYPHES_DATA_DIR") {
        return Ok(PathBuf::from(data_dir));
    }
    let home = dirs::home_dir().ok_or_else(|| "Could not resolve home directory".to_string())?;
    Ok(home.join(".cyphes"))
}

fn database_path() -> Result<PathBuf, String> {
    Ok(data_dir()?.join("atp.sqlite3"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        atp::{agent_id, create_signed_envelope, create_signed_envelope_with_expiry, AtpVerb},
        audit_labor::{
            signed_contribution, signed_verification, AuditFinding, ContributionArtifact,
            CoverageItem, RuntimeDescriptor, VerificationEvidence,
        },
        audit_profile::{contract_hash, AuditContract, ReceiptApproval, RepositoryTarget},
        bundle::{export_campaign_report_bundle_to, export_receipt_bundle_to},
        worker::{create_repository_leases, execute_repository_audit},
    };
    use chrono::{Duration, SecondsFormat, Utc};

    fn test_store() -> AtpStore {
        let connection = Connection::open_in_memory().unwrap();
        let store = AtpStore {
            connection: Arc::new(Mutex::new(connection)),
        };
        store.initialize().unwrap();
        store
    }

    fn labor_artifact(path: &str) -> ContributionArtifact {
        ContributionArtifact {
            path: path.to_string(),
            media_type: "text/markdown".to_string(),
            sha256: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string(),
            size_bytes: 128,
        }
    }

    fn labor_campaign(requester_agent: String) -> ProtocolAuditCampaign {
        ProtocolAuditCampaign::new(
            "Aave".to_string(),
            RepositoryTarget {
                full_name: "aave-dao/aave-v3-origin".to_string(),
                url: "https://github.com/aave-dao/aave-v3-origin".to_string(),
                commit_sha: "fd1fbd9150426ca8ace9cee45b4acf912ae84f5b".to_string(),
            },
            "Audit pool accounting and liquidation invariants.".to_string(),
            Some("https://immunefi.com/bug-bounty/aave/scope/".to_string()),
            vec![
                "Principal theft".to_string(),
                "Protocol insolvency".to_string(),
            ],
            vec![
                "Best practice notes".to_string(),
                "Privileged key compromise".to_string(),
            ],
            Some("AAVE Immunefi scope handoff".to_string()),
            requester_agent,
        )
        .unwrap()
    }

    #[test]
    fn commits_a_signed_announcement_and_replays_idempotently() {
        let store = test_store();
        let keypair = libp2p::identity::Keypair::generate_ed25519();
        let local_agent = agent_id(&keypair.public());
        let job = AuditJobPayload {
            id: "audit-1".to_string(),
            repository: RepositorySummary {
                full_name: "bitcoin/bitcoin".to_string(),
                url: "https://github.com/bitcoin/bitcoin".to_string(),
                description: None,
                language: Some("C++".to_string()),
                default_branch: "master".to_string(),
                stars: 1,
                is_private: false,
                commit_sha: "0000000000000000000000000000000000000001".to_string(),
            },
            compensation: "100".to_string(),
            currency: "ATP Credits".to_string(),
            scope: vec!["Dependency risk".to_string()],
            requester_agent_id: local_agent.clone(),
            created_at: now_millis(),
        };
        let envelope = create_signed_envelope(
            &keypair,
            AtpVerb::Discover,
            job.id.clone(),
            None,
            None,
            serde_json::to_value(AuditEventBody::Announce { job }).unwrap(),
        )
        .unwrap();

        let first = store
            .commit_envelope(&envelope, &local_agent, None)
            .unwrap();
        assert!(first.accepted);
        assert!(!first.duplicate);
        let second = store
            .commit_envelope(&envelope, &local_agent, None)
            .unwrap();
        assert!(second.duplicate);
        assert_eq!(store.list_jobs().unwrap().len(), 1);
        let campaign_id = campaign_id_for_transaction("audit-1");
        let snapshot = store.campaign_report_snapshot(&campaign_id).unwrap();
        assert_eq!(snapshot.campaign.repository.full_name, "bitcoin/bitcoin");
        assert_eq!(snapshot.work_units.len(), 7);
        assert_eq!(snapshot.contributions.len(), 0);
    }

    #[test]
    fn requires_worker_offer_then_requester_selection() {
        let store = test_store();
        let requester = libp2p::identity::Keypair::generate_ed25519();
        let worker = libp2p::identity::Keypair::generate_ed25519();
        let requester_agent = agent_id(&requester.public());
        let worker_agent = agent_id(&worker.public());
        let job = AuditJobPayload {
            id: "audit-2".to_string(),
            repository: RepositorySummary {
                full_name: "cyphes/example".to_string(),
                url: "https://github.com/cyphes/example".to_string(),
                description: None,
                language: Some("Rust".to_string()),
                default_branch: "main".to_string(),
                stars: 0,
                is_private: false,
                commit_sha: "0000000000000000000000000000000000000002".to_string(),
            },
            compensation: "100".to_string(),
            currency: "ATP Credits".to_string(),
            scope: vec!["Repository audit".to_string()],
            requester_agent_id: requester_agent.clone(),
            created_at: now_millis(),
        };
        let discover = create_signed_envelope(
            &requester,
            AtpVerb::Discover,
            job.id.clone(),
            None,
            None,
            serde_json::to_value(AuditEventBody::Announce { job }).unwrap(),
        )
        .unwrap();
        let discover_ack = store
            .commit_envelope(&discover, &requester_agent, None)
            .unwrap();

        let expiry =
            (Utc::now() + Duration::minutes(30)).to_rfc3339_opts(SecondsFormat::Millis, true);
        let contract = AuditContract::repository_audit(
            "audit-2".to_string(),
            requester_agent.clone(),
            worker_agent.clone(),
            RepositoryTarget {
                full_name: "cyphes/example".to_string(),
                url: "https://github.com/cyphes/example".to_string(),
                commit_sha: "0000000000000000000000000000000000000002".to_string(),
            },
            vec!["Repository audit".to_string()],
            "100".to_string(),
            expiry.clone(),
        );
        let offered_contract_hash = contract_hash(&contract).unwrap();
        let mut altered_contract = contract.clone();
        altered_contract.proposed_compensation.amount = "999".to_string();
        let altered_offer = create_signed_envelope_with_expiry(
            &worker,
            AtpVerb::Negotiate,
            "audit-2".to_string(),
            Some(requester_agent.clone()),
            Some(discover_ack.event_hash.clone()),
            serde_json::to_value(AuditEventBody::WorkerOffer {
                job_id: "audit-2".to_string(),
                worker_agent_id: worker_agent.clone(),
                contract: altered_contract,
            })
            .unwrap(),
            Some(expiry.clone()),
        )
        .unwrap();
        let error = store
            .commit_envelope(&altered_offer, &requester_agent, None)
            .unwrap_err();
        assert!(error.contains("AUDIT_CONTRACT_REQUEST_MISMATCH"));

        let offer = create_signed_envelope_with_expiry(
            &worker,
            AtpVerb::Negotiate,
            "audit-2".to_string(),
            Some(requester_agent.clone()),
            Some(discover_ack.event_hash),
            serde_json::to_value(AuditEventBody::WorkerOffer {
                job_id: "audit-2".to_string(),
                worker_agent_id: worker_agent.clone(),
                contract,
            })
            .unwrap(),
            Some(expiry),
        )
        .unwrap();
        let offer_ack = store
            .commit_envelope(&offer, &requester_agent, None)
            .unwrap();
        assert_eq!(offer_ack.state.as_deref(), Some("negotiating"));
        let offered = store.get_job("audit-2").unwrap();
        assert_eq!(offered.last_event_hash, offer_ack.event_hash);
        assert_eq!(
            offered.contract_hash.as_deref(),
            Some(offered_contract_hash.as_str())
        );

        let selection = create_signed_envelope(
            &requester,
            AtpVerb::Negotiate,
            "audit-2".to_string(),
            Some(worker_agent.clone()),
            Some(offer_ack.event_hash),
            serde_json::to_value(AuditEventBody::WorkerSelected {
                job_id: "audit-2".to_string(),
                worker_agent_id: worker_agent.clone(),
                contract_hash: offered_contract_hash.clone(),
            })
            .unwrap(),
        )
        .unwrap();
        let selection_ack = store
            .commit_envelope(&selection, &requester_agent, None)
            .unwrap();

        assert_eq!(selection_ack.state.as_deref(), Some("negotiated"));
        let committed = store.get_job("audit-2").unwrap();
        assert_eq!(committed.status, "negotiated");
        assert_eq!(
            committed.worker_agent_id.as_deref(),
            Some(worker_agent.as_str())
        );
        assert_eq!(
            committed.contract_hash.as_deref(),
            Some(offered_contract_hash.as_str())
        );
    }

    #[test]
    fn campaign_contributions_verifications_and_credits_are_local_and_receipt_backed() {
        let store = test_store();
        let requester = libp2p::identity::Keypair::generate_ed25519();
        let worker = libp2p::identity::Keypair::generate_ed25519();
        let verifier = libp2p::identity::Keypair::generate_ed25519();
        let campaign = labor_campaign(agent_id(&requester.public()));
        store.create_protocol_campaign(&campaign).unwrap();
        let work_units = store.list_work_units(&campaign.campaign_id).unwrap();
        assert!(work_units
            .iter()
            .any(|unit| unit.kind == "defi-exploit-class-pass"));

        let accepted_contribution = signed_contribution(
            &worker,
            campaign.campaign_id.clone(),
            work_units[3].work_unit_id.clone(),
            RuntimeDescriptor::deterministic_fixture(),
            "Mapped DeFi exploit-class applicability with no code execution.".to_string(),
            vec![AuditFinding {
                id: "AAVE-COVERAGE-001".to_string(),
                title: "No reportable bug in fixture pass".to_string(),
                severity: "informational".to_string(),
                status: "non_reportable".to_string(),
                impact: None,
                evidence: vec!["coverage-notes.md".to_string()],
                reportable: false,
            }],
            vec![labor_artifact("coverage-notes.md")],
            vec![CoverageItem {
                area: "oracle mocks".to_string(),
                status: "considered".to_string(),
                evidence: vec!["Oracle mock assumptions recorded.".to_string()],
            }],
            vec!["no repository code execution".to_string()],
        )
        .unwrap();
        store.record_contribution(&accepted_contribution).unwrap();
        let submitted_units = store.list_work_units(&campaign.campaign_id).unwrap();
        assert_eq!(
            submitted_units
                .iter()
                .find(|unit| unit.work_unit_id == accepted_contribution.work_unit_id)
                .unwrap()
                .status,
            "submitted"
        );
        let accepted_verification = signed_verification(
            &verifier,
            campaign.campaign_id.clone(),
            accepted_contribution.contribution_id.clone(),
            "accepted".to_string(),
            "COVERAGE_ACCEPTED".to_string(),
            "Coverage is useful and bounded.".to_string(),
            vec![VerificationEvidence {
                label: "receipt".to_string(),
                reference: accepted_contribution.receipt_hash.clone(),
            }],
            vec![labor_artifact("verification.md")],
        )
        .unwrap();
        let allocations = store.record_verification(&accepted_verification).unwrap();
        assert_eq!(allocations.len(), 2);
        let accepted_units = store.list_work_units(&campaign.campaign_id).unwrap();
        assert_eq!(
            accepted_units
                .iter()
                .find(|unit| unit.work_unit_id == accepted_contribution.work_unit_id)
                .unwrap()
                .status,
            "accepted"
        );
        assert!(allocations
            .iter()
            .all(|allocation| allocation.contribution_receipt_hash
                == accepted_contribution.receipt_hash));

        let rejected_contribution = signed_contribution(
            &worker,
            campaign.campaign_id.clone(),
            work_units[4].work_unit_id.clone(),
            RuntimeDescriptor::deterministic_fixture(),
            "Lead matched prior public report.".to_string(),
            vec![AuditFinding {
                id: "AAVE-DUP-001".to_string(),
                title: "Duplicate lead".to_string(),
                severity: "high".to_string(),
                status: "duplicate".to_string(),
                impact: Some("Principal theft".to_string()),
                evidence: vec!["prior-audit.md".to_string()],
                reportable: true,
            }],
            vec![labor_artifact("duplicate.md")],
            vec![CoverageItem {
                area: "known issue search".to_string(),
                status: "failed".to_string(),
                evidence: vec!["Duplicate discovered.".to_string()],
            }],
            vec![],
        )
        .unwrap();
        store.record_contribution(&rejected_contribution).unwrap();
        let rejected_verification = signed_verification(
            &verifier,
            campaign.campaign_id.clone(),
            rejected_contribution.contribution_id.clone(),
            "rejected".to_string(),
            "DUPLICATE_KNOWN_ISSUE".to_string(),
            "Lead is duplicate and appendix-only.".to_string(),
            vec![],
            vec![labor_artifact("rejection.md")],
        )
        .unwrap();
        assert!(store
            .record_verification(&rejected_verification)
            .unwrap()
            .is_empty());
        let rejected_units = store.list_work_units(&campaign.campaign_id).unwrap();
        assert_eq!(
            rejected_units
                .iter()
                .find(|unit| unit.work_unit_id == rejected_contribution.work_unit_id)
                .unwrap()
                .status,
            "rejected"
        );

        let snapshot = store
            .campaign_report_snapshot(&campaign.campaign_id)
            .unwrap();
        assert_eq!(snapshot.contributions.len(), 2);
        assert_eq!(snapshot.credits.len(), 2);
        let report_root =
            std::env::temp_dir().join(format!("cyphes-report-{}", campaign.campaign_id));
        let bundle =
            export_campaign_report_bundle_to(&store, &campaign.campaign_id, &report_root).unwrap();
        assert!(bundle.join("report.md").is_file());
        assert!(bundle.join("findings.json").is_file());
        assert!(bundle.join("receipts/README.md").is_file());
        let report = fs::read_to_string(bundle.join("report.md")).unwrap();
        let findings_section = report
            .split("## Non-reportable, Rejected, Or Duplicate Leads")
            .next()
            .unwrap();
        assert!(report.contains("## Document Control"));
        assert!(report.contains("## Audit Pass Matrix"));
        assert!(report.contains("## Evidence Arbitration"));
        assert!(report.contains("## Runtime And Receipt Appendix"));
        assert!(!findings_section.contains("Duplicate lead"));
        assert!(report.contains("Duplicate lead"));
    }

    #[tokio::test]
    #[ignore = "downloads a pinned GitHub archive and exports a real receipt bundle"]
    async fn completes_a_real_atp_l1_repository_transaction() {
        let store = test_store();
        let requester = libp2p::identity::Keypair::generate_ed25519();
        let worker = libp2p::identity::Keypair::generate_ed25519();
        let requester_agent = agent_id(&requester.public());
        let worker_agent = agent_id(&worker.public());
        let transaction_id = format!("audit-e2e-{}", uuid::Uuid::new_v4());
        let repository = RepositorySummary {
            full_name: "octocat/Hello-World".to_string(),
            url: "https://github.com/octocat/Hello-World".to_string(),
            description: Some("ATP-L1 integration fixture".to_string()),
            language: None,
            default_branch: "master".to_string(),
            stars: 0,
            is_private: false,
            commit_sha: "7fd1a60b01f91b314f59955a4e4d4e80d8edf11d".to_string(),
        };
        let scope = vec!["Deterministic repository security posture".to_string()];
        let discover = create_signed_envelope(
            &requester,
            AtpVerb::Discover,
            transaction_id.clone(),
            None,
            None,
            serde_json::to_value(AuditEventBody::Announce {
                job: AuditJobPayload {
                    id: transaction_id.clone(),
                    repository: repository.clone(),
                    compensation: "100".to_string(),
                    currency: "ATP Credits".to_string(),
                    scope: scope.clone(),
                    requester_agent_id: requester_agent.clone(),
                    created_at: now_millis(),
                },
            })
            .unwrap(),
        )
        .unwrap();
        let discover_ack = store
            .commit_envelope(&discover, &requester_agent, None)
            .unwrap();

        let expiry =
            (Utc::now() + Duration::minutes(30)).to_rfc3339_opts(SecondsFormat::Millis, true);
        let contract = AuditContract::repository_audit(
            transaction_id.clone(),
            requester_agent.clone(),
            worker_agent.clone(),
            RepositoryTarget {
                full_name: repository.full_name.clone(),
                url: repository.url.clone(),
                commit_sha: repository.commit_sha.clone(),
            },
            scope,
            "100".to_string(),
            expiry.clone(),
        );
        let expected_contract_hash = contract_hash(&contract).unwrap();
        let offer = create_signed_envelope_with_expiry(
            &worker,
            AtpVerb::Negotiate,
            transaction_id.clone(),
            Some(requester_agent.clone()),
            Some(discover_ack.event_hash),
            serde_json::to_value(AuditEventBody::WorkerOffer {
                job_id: transaction_id.clone(),
                worker_agent_id: worker_agent.clone(),
                contract: contract.clone(),
            })
            .unwrap(),
            Some(expiry),
        )
        .unwrap();
        let offer_ack = store
            .commit_envelope(&offer, &requester_agent, None)
            .unwrap();

        let selection = create_signed_envelope(
            &requester,
            AtpVerb::Negotiate,
            transaction_id.clone(),
            Some(worker_agent.clone()),
            Some(offer_ack.event_hash),
            serde_json::to_value(AuditEventBody::WorkerSelected {
                job_id: transaction_id.clone(),
                worker_agent_id: worker_agent.clone(),
                contract_hash: expected_contract_hash.clone(),
            })
            .unwrap(),
        )
        .unwrap();
        let selection_ack = store
            .commit_envelope(&selection, &requester_agent, None)
            .unwrap();

        let leases = create_repository_leases(&requester, &contract).unwrap();
        let route = create_signed_envelope(
            &requester,
            AtpVerb::Route,
            transaction_id.clone(),
            Some(worker_agent.clone()),
            Some(selection_ack.event_hash),
            serde_json::to_value(AuditEventBody::RouteAudit {
                job_id: transaction_id.clone(),
                contract_hash: expected_contract_hash.clone(),
                leases: leases.clone(),
            })
            .unwrap(),
        )
        .unwrap();
        let route_ack = store.commit_envelope(&route, &worker_agent, None).unwrap();
        assert_eq!(route_ack.state.as_deref(), Some("routed"));

        let work_root = std::env::temp_dir().join(format!("cyphes-{transaction_id}"));
        let result = execute_repository_audit(
            &worker,
            &contract,
            &expected_contract_hash,
            &leases,
            &work_root,
        )
        .await
        .unwrap();
        store.save_execution_result(&result).unwrap();

        let approval = ReceiptApproval {
            by: requester_agent.clone(),
            method: "requester-verified-result".to_string(),
            time: crate::atp::now_rfc3339(),
        };
        let settle = create_signed_envelope(
            &requester,
            AtpVerb::Settle,
            transaction_id.clone(),
            Some(worker_agent.clone()),
            Some(route_ack.event_hash),
            serde_json::to_value(AuditEventBody::SettlementApproved {
                job_id: transaction_id.clone(),
                contract_hash: expected_contract_hash,
                result_hash: result.result_hash.clone(),
                approved: approval.clone(),
            })
            .unwrap(),
        )
        .unwrap();
        let settle_ack = store.commit_envelope(&settle, &worker_agent, None).unwrap();

        let receipt = store
            .build_worker_receipt(&transaction_id, &settle_ack.event_hash, approval, &worker)
            .unwrap();
        let attest = create_signed_envelope(
            &worker,
            AtpVerb::Attest,
            transaction_id.clone(),
            Some(requester_agent),
            Some(settle_ack.event_hash),
            serde_json::to_value(receipt).unwrap(),
        )
        .unwrap();
        let attest_ack = store.commit_envelope(&attest, &worker_agent, None).unwrap();
        assert_eq!(attest_ack.state.as_deref(), Some("attested"));

        let receipt_root = work_root.join("verified-receipts");
        let bundle = export_receipt_bundle_to(&store, &transaction_id, &receipt_root).unwrap();
        assert!(bundle.join("receipt.json").is_file());
        assert!(bundle.join("artifacts/audit-report.md").is_file());
        assert_eq!(
            store.transaction_envelopes(&transaction_id).unwrap().len(),
            6
        );
        println!("ATP_E2E_BUNDLE={}", bundle.display());
    }
}
