use std::{
    fs,
    path::{Path, PathBuf},
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
        allocate_credits, allocate_provisional_credits, default_work_units, validate_campaign,
        verify_signed_contribution, verify_signed_verification, verify_signed_work_unit_claim,
        AuditWorkUnit, AuditWorkUnitClaim, CampaignReportSnapshot, CreditAllocation, CreditSummary,
        NodeContribution, ProtocolAuditCampaign, VerificationResult, WORK_UNIT_CLAIM_TTL_MS,
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
    #[serde(default)]
    pub audit_brief_text: Option<String>,
    #[serde(default)]
    pub attachment_text: Option<String>,
    #[serde(default)]
    pub custom_skill_text: Option<String>,
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditLaborInventory {
    pub campaign_ids: Vec<String>,
    pub claim_ids: Vec<String>,
    pub contribution_ids: Vec<String>,
    pub verification_ids: Vec<String>,
    pub needs_verifier_contribution_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaborObjectPreflight {
    New,
    Duplicate(&'static str),
    Superseded(&'static str),
}

impl LaborObjectPreflight {
    pub fn skip_reason(&self) -> Option<&'static str> {
        match self {
            Self::New => None,
            Self::Duplicate(reason) | Self::Superseded(reason) => Some(reason),
        }
    }
}

#[derive(Debug, Clone)]
pub struct StaleContributionRepair {
    pub campaign: ProtocolAuditCampaign,
    pub claim: Option<AuditWorkUnitClaim>,
    pub contribution: NodeContribution,
}

#[derive(Debug, Clone)]
pub struct PendingLaborObject {
    pub object_kind: String,
    pub object_id: String,
    pub object_json: String,
}

pub const ATP_STORE_TESTNET_ID: &str = "cyphes-dev-v0.7.7";
pub const MAX_PENDING_CONTRIBUTIONS_PER_WORKER: usize = 25;
const STORE_META_TESTNET_ID_KEY: &str = "testnet_id";
const STORE_META_APP_VERSION_KEY: &str = "app_version";
const STORE_META_SCHEMA_KEY: &str = "schema";
const STORE_SCHEMA_VERSION: &str = "audit-labor-v1";
const CLAIM_CONTRIBUTION_CLOCK_SKEW_MS: i64 = 60_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContributionIngestPolicy {
    LocalWorker,
    PeerSync,
}

const LEDGER_TABLES: &[&str] = &[
    "atp_events",
    "audit_jobs",
    "protocol_audit_campaigns",
    "audit_work_units",
    "audit_work_unit_claims",
    "audit_contributions",
    "audit_verifications",
    "credit_allocations",
    "audit_labor_pending_objects",
    "audit_labor_events",
];

pub fn campaign_id_for_transaction(transaction_id: &str) -> String {
    format!("campaign_{transaction_id}")
}

impl AtpStore {
    pub fn open_default() -> Result<Self, String> {
        let path = database_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        prepare_database_path_for_current_testnet(&path)?;

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
        if let Some(existing) = equivalent_campaign_in_transaction(&transaction, campaign)? {
            transaction.commit().map_err(|error| error.to_string())?;
            return Ok(existing);
        }
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

    pub fn upsert_protocol_campaign(
        &self,
        campaign: &ProtocolAuditCampaign,
    ) -> Result<ProtocolAuditCampaign, String> {
        validate_campaign(campaign)?;
        let work_units = default_work_units(campaign);
        let mut connection = self.connection.lock().map_err(|error| error.to_string())?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        if let Some(existing) = equivalent_campaign_in_transaction(&transaction, campaign)? {
            transaction.commit().map_err(|error| error.to_string())?;
            return Ok(existing);
        }
        let exists = transaction
            .query_row(
                "SELECT 1 FROM protocol_audit_campaigns WHERE campaign_id = ?1",
                params![campaign.campaign_id],
                |_| Ok(()),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .is_some();
        if !exists {
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

    pub fn record_work_unit_claim(
        &self,
        claim: &AuditWorkUnitClaim,
    ) -> Result<AuditWorkUnitClaim, String> {
        verify_signed_work_unit_claim(claim)?;
        let now = now_millis() as i64;
        if work_unit_claim_is_expired(claim, now)? {
            return Err("work unit claim has expired".to_string());
        }
        let mut connection = self.connection.lock().map_err(|error| error.to_string())?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        expire_stale_claims_in_transaction(&transaction, claim_expiry_cutoff(now), now)?;
        let campaign = campaign_in_transaction(&transaction, &claim.campaign_id)?;
        if campaign.requester_agent_id != claim.requester_agent_id {
            return Err("claim requester does not match campaign requester".to_string());
        }
        let work_unit =
            work_unit_in_transaction(&transaction, &claim.campaign_id, &claim.work_unit_id)?;
        if matches!(
            work_unit.status.as_str(),
            "submitted" | "accepted" | "rejected" | "challenged" | "revision_requested"
        ) {
            return Err("work unit already has submitted or reviewed work".to_string());
        }
        let existing = transaction
            .query_row(
                "SELECT claim_json FROM audit_work_unit_claims
                 WHERE campaign_id = ?1 AND work_unit_id = ?2 AND status = 'claimed'",
                params![claim.campaign_id, claim.work_unit_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if let Some(json) = existing {
            let existing_claim: AuditWorkUnitClaim =
                serde_json::from_str(&json).map_err(|error| error.to_string())?;
            if existing_claim.claim_id == claim.claim_id {
                return Ok(existing_claim);
            }
            return Err("work unit is already claimed by another node".to_string());
        }
        transaction
            .execute(
                "INSERT INTO audit_work_unit_claims
                    (claim_id, campaign_id, work_unit_id, worker_agent_id, requester_agent_id,
                     status, claim_json, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    claim.claim_id,
                    claim.campaign_id,
                    claim.work_unit_id,
                    claim.worker_agent_id,
                    claim.requester_agent_id,
                    claim.status,
                    serde_json::to_string(claim).map_err(|error| error.to_string())?,
                    millis_from_rfc3339(&claim.created_at)?,
                    now_millis() as i64,
                ],
            )
            .map_err(|error| error.to_string())?;
        update_work_unit_claim_status(
            &transaction,
            &claim.campaign_id,
            &claim.work_unit_id,
            claim,
        )?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(claim.clone())
    }

    pub fn record_historical_work_unit_claim(
        &self,
        claim: &AuditWorkUnitClaim,
    ) -> Result<AuditWorkUnitClaim, String> {
        verify_signed_work_unit_claim(claim)?;
        let now = now_millis() as i64;
        let mut connection = self.connection.lock().map_err(|error| error.to_string())?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        expire_stale_claims_in_transaction(&transaction, claim_expiry_cutoff(now), now)?;
        let campaign = campaign_in_transaction(&transaction, &claim.campaign_id)?;
        if campaign.requester_agent_id != claim.requester_agent_id {
            return Err("claim requester does not match campaign requester".to_string());
        }
        let _work_unit =
            work_unit_in_transaction(&transaction, &claim.campaign_id, &claim.work_unit_id)?;
        let claim_json = serde_json::to_string(claim).map_err(|error| error.to_string())?;
        let existing = transaction
            .query_row(
                "SELECT claim_json FROM audit_work_unit_claims WHERE claim_id = ?1",
                params![claim.claim_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if let Some(existing_json) = existing {
            if existing_json == claim_json {
                return Ok(claim.clone());
            }
            return Err("claim id already exists with different signed content".to_string());
        }
        transaction
            .execute(
                "INSERT INTO audit_work_unit_claims
                    (claim_id, campaign_id, work_unit_id, worker_agent_id, requester_agent_id,
                     status, claim_json, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'expired', ?6, ?7, ?8)",
                params![
                    claim.claim_id,
                    claim.campaign_id,
                    claim.work_unit_id,
                    claim.worker_agent_id,
                    claim.requester_agent_id,
                    claim_json,
                    millis_from_rfc3339(&claim.created_at)?,
                    now,
                ],
            )
            .map_err(|error| error.to_string())?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(claim.clone())
    }

    pub fn record_contribution(
        &self,
        contribution: &NodeContribution,
    ) -> Result<NodeContribution, String> {
        self.record_contribution_with_policy(contribution, ContributionIngestPolicy::LocalWorker)
    }

    pub fn record_network_contribution(
        &self,
        contribution: &NodeContribution,
    ) -> Result<NodeContribution, String> {
        self.record_contribution_with_policy(contribution, ContributionIngestPolicy::PeerSync)
    }

    pub fn contribution_preflight_status(
        &self,
        contribution: &NodeContribution,
    ) -> Result<LaborObjectPreflight, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        let existing_by_id = connection
            .query_row(
                "SELECT receipt_hash FROM audit_contributions WHERE contribution_id = ?1",
                params![contribution.contribution_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if let Some(existing_receipt_hash) = existing_by_id {
            return Ok(if existing_receipt_hash == contribution.receipt_hash {
                LaborObjectPreflight::Duplicate("contribution already known")
            } else {
                LaborObjectPreflight::Superseded(
                    "contribution id already exists with different receipt hash",
                )
            });
        }
        let existing_by_receipt = connection
            .query_row(
                "SELECT contribution_id FROM audit_contributions WHERE receipt_hash = ?1",
                params![contribution.receipt_hash],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if existing_by_receipt.is_some() {
            return Ok(LaborObjectPreflight::Duplicate(
                "contribution receipt hash already known",
            ));
        }
        let existing_for_worker_unit = connection
            .query_row(
                "SELECT contribution_id FROM audit_contributions
                 WHERE campaign_id = ?1
                   AND work_unit_id = ?2
                   AND worker_agent_id = ?3
                 ORDER BY created_at, contribution_id
                 LIMIT 1",
                params![
                    contribution.campaign_id,
                    contribution.work_unit_id,
                    contribution.worker_agent_id
                ],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if existing_for_worker_unit.is_some() {
            return Ok(LaborObjectPreflight::Superseded(
                "worker already submitted a contribution for this work unit",
            ));
        }
        let work_unit_status = connection
            .query_row(
                "SELECT status FROM audit_work_units
                 WHERE campaign_id = ?1 AND work_unit_id = ?2",
                params![contribution.campaign_id, contribution.work_unit_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if work_unit_status
            .as_deref()
            .is_some_and(is_reviewed_terminal_work_unit_status)
        {
            return Ok(LaborObjectPreflight::Superseded(
                "work unit already has reviewed work",
            ));
        }
        Ok(LaborObjectPreflight::New)
    }

    fn record_contribution_with_policy(
        &self,
        contribution: &NodeContribution,
        policy: ContributionIngestPolicy,
    ) -> Result<NodeContribution, String> {
        verify_signed_contribution(contribution)?;
        let mut connection = self.connection.lock().map_err(|error| error.to_string())?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        let existing = transaction
            .query_row(
                "SELECT contribution_json FROM audit_contributions WHERE contribution_id = ?1",
                params![contribution.contribution_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if let Some(json) = existing {
            let existing_contribution: NodeContribution =
                serde_json::from_str(&json).map_err(|error| error.to_string())?;
            if existing_contribution == *contribution {
                return Ok(existing_contribution);
            }
            return Err("contribution id already exists with different signed content".to_string());
        }
        let _campaign = campaign_in_transaction(&transaction, &contribution.campaign_id)
            .map_err(|_| "contribution campaign is not known locally".to_string())?;
        let work_unit = work_unit_in_transaction(
            &transaction,
            &contribution.campaign_id,
            &contribution.work_unit_id,
        )
        .map_err(|_| "contribution work unit is not known locally".to_string())?;
        let existing_for_worker_unit = transaction
            .query_row(
                "SELECT contribution_json FROM audit_contributions
                 WHERE campaign_id = ?1
                   AND work_unit_id = ?2
                   AND worker_agent_id = ?3
                 ORDER BY created_at, contribution_id
                 LIMIT 1",
                params![
                    contribution.campaign_id,
                    contribution.work_unit_id,
                    contribution.worker_agent_id
                ],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if existing_for_worker_unit.is_some() {
            return Err("worker already submitted a contribution for this work unit".to_string());
        }
        if policy == ContributionIngestPolicy::LocalWorker
            && is_reviewed_terminal_work_unit_status(&work_unit.status)
        {
            return Err("work unit already has reviewed work".to_string());
        }
        let claim = claim_for_contribution_in_connection(&transaction, contribution)?;
        if claim.is_none() {
            return Err("work unit must be claimed by this worker before submission".to_string());
        }
        if policy == ContributionIngestPolicy::LocalWorker {
            if let Some(claimed_by) = work_unit.claimed_by_agent_id.as_deref() {
                if claimed_by != contribution.worker_agent_id && work_unit.status != "claimed" {
                    return Err("work unit is claimed by another worker".to_string());
                }
            }
            let worker_pending = transaction
                .query_row(
                    "SELECT COUNT(*) FROM audit_contributions c
                     WHERE c.worker_agent_id = ?1
                       AND NOT EXISTS (
                        SELECT 1 FROM audit_verifications v
                        WHERE v.target_contribution_id = c.contribution_id
                     )",
                    params![contribution.worker_agent_id],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(|error| error.to_string())?;
            if worker_pending.max(0) as usize >= MAX_PENDING_CONTRIBUTIONS_PER_WORKER {
                return Err(format!(
                    "worker backpressure active: {worker_pending} receipt{} from this worker await independent verification; pause new audit work until the network clears below {MAX_PENDING_CONTRIBUTIONS_PER_WORKER}",
                    if worker_pending == 1 { "" } else { "s" }
                ));
            }
        }
        let inserted = transaction
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
        expect_one_row(inserted, "contribution insert")?;
        if !is_reviewed_terminal_work_unit_status(&work_unit.status) {
            update_work_unit_status(
                &transaction,
                &contribution.campaign_id,
                &contribution.work_unit_id,
                "submitted",
                Some(contribution.worker_agent_id.as_str()),
            )?;
        }
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(contribution.clone())
    }

    pub fn record_verification(
        &self,
        verification: &VerificationResult,
    ) -> Result<Vec<CreditAllocation>, String> {
        verify_signed_verification(verification)?;
        let contribution = self.get_contribution(&verification.target_contribution_id)?;
        let allocations = credit_allocations_for_verification(&contribution, verification)?;
        let mut connection = self.connection.lock().map_err(|error| error.to_string())?;
        if let Some((_, existing_allocations)) = verification_bundle_for_contribution_in_connection(
            &connection,
            &verification.target_contribution_id,
        )? {
            return Ok(existing_allocations);
        }
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
            None,
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

    pub fn record_verification_bundle(
        &self,
        verification: &VerificationResult,
        allocations: &[CreditAllocation],
    ) -> Result<Vec<CreditAllocation>, String> {
        verify_signed_verification(verification)?;
        let contribution = self.get_contribution(&verification.target_contribution_id)?;
        if verification.campaign_id != contribution.campaign_id {
            return Err("verification campaign does not match contribution".to_string());
        }
        validate_credit_allocation_bundle(&contribution, verification, allocations)?;

        let mut connection = self.connection.lock().map_err(|error| error.to_string())?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        let verification_json =
            serde_json::to_string(verification).map_err(|error| error.to_string())?;
        let existing_verification_json = transaction
            .query_row(
                "SELECT verification_json FROM audit_verifications WHERE verification_id = ?1",
                params![verification.verification_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if let Some(existing_json) = existing_verification_json {
            if existing_json != verification_json {
                return Err(
                    "verification id already exists with different signed content".to_string(),
                );
            }
            let existing_allocations = credit_allocations_for_verification_id_in_connection(
                &transaction,
                &verification.verification_id,
            )?;
            if !credit_allocation_terms_match_set(&existing_allocations, allocations) {
                return Err(
                    "verification id already exists with different credit allocations".to_string(),
                );
            }
            return Ok(existing_allocations);
        }
        if let Some((existing_verification, _)) =
            verification_bundle_for_contribution_in_connection(
                &transaction,
                &verification.target_contribution_id,
            )?
        {
            if existing_verification.verification_id != verification.verification_id {
                return Err("contribution already has a different verification bundle".to_string());
            }
        }
        let inserted = transaction
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
                    verification_json,
                    millis_from_rfc3339(&verification.created_at)?,
                ],
            )
            .map_err(|error| error.to_string())?;
        expect_one_row(inserted, "verification insert")?;
        let contribution_status = match verification.decision.as_str() {
            "accepted" | "reproduced" => "accepted",
            "rejected" => "rejected",
            "challenged" => "challenged",
            "revision_requested" => "revision_requested",
            _ => "reviewed",
        };
        let updated = transaction
            .execute(
                "UPDATE audit_contributions
                 SET status = ?2
                 WHERE contribution_id = ?1",
                params![verification.target_contribution_id, contribution_status],
            )
            .map_err(|error| error.to_string())?;
        expect_one_row(updated, "verified contribution status update")?;
        update_work_unit_status(
            &transaction,
            &contribution.campaign_id,
            &contribution.work_unit_id,
            contribution_status,
            None,
        )?;
        for allocation in allocations {
            let inserted = transaction
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
            expect_one_row(inserted, "credit allocation insert")?;
        }
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(allocations.to_vec())
    }

    pub fn verification_bundle_preflight_status(
        &self,
        verification: &VerificationResult,
    ) -> Result<LaborObjectPreflight, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        let existing_target_for_id = connection
            .query_row(
                "SELECT target_contribution_id FROM audit_verifications
                 WHERE verification_id = ?1",
                params![verification.verification_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if let Some(existing_target) = existing_target_for_id {
            return Ok(if existing_target == verification.target_contribution_id {
                LaborObjectPreflight::Duplicate("verification already known")
            } else {
                LaborObjectPreflight::Superseded(
                    "verification id already exists for a different contribution",
                )
            });
        }
        let existing_verification_for_target = connection
            .query_row(
                "SELECT verification_id FROM audit_verifications
                 WHERE target_contribution_id = ?1
                 ORDER BY created_at, verification_id
                 LIMIT 1",
                params![verification.target_contribution_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if existing_verification_for_target.is_some() {
            return Ok(LaborObjectPreflight::Superseded(
                "contribution already has a verification bundle",
            ));
        }
        Ok(LaborObjectPreflight::New)
    }

    pub fn get_contribution(&self, contribution_id: &str) -> Result<NodeContribution, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        contribution_in_connection(&connection, contribution_id)
    }

    pub fn verification_bundle_for_contribution(
        &self,
        contribution_id: &str,
    ) -> Result<Option<(VerificationResult, Vec<CreditAllocation>)>, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        verification_bundle_for_contribution_in_connection(&connection, contribution_id)
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
        let candidate_allocations = rows
            .map(|row| {
                let json = row.map_err(|error| error.to_string())?;
                serde_json::from_str(&json).map_err(|error| error.to_string())
            })
            .collect::<Result<Vec<CreditAllocation>, String>>()?;
        let mut allocations = Vec::new();
        let mut provisional_allocations = Vec::new();
        for allocation in candidate_allocations {
            match credit_allocation_trust(&connection, &allocation)? {
                CreditAllocationTrust::Verified => allocations.push(allocation),
                CreditAllocationTrust::Provisional => provisional_allocations.push(allocation),
                CreditAllocationTrust::Invalid => {}
            }
        }
        let total = allocations.iter().map(|allocation| allocation.total).sum();
        let provisional_total = provisional_allocations
            .iter()
            .map(|allocation| allocation.total)
            .sum();
        Ok(CreditSummary {
            total,
            allocations,
            provisional_total,
            provisional_allocations,
        })
    }

    #[cfg(test)]
    pub fn verification_bundles_for_worker(
        &self,
        worker_agent_id: &str,
    ) -> Result<Vec<(VerificationResult, Vec<CreditAllocation>)>, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        let verifications = {
            let mut statement = connection
                .prepare(
                    "SELECT v.verification_json
                     FROM audit_verifications v
                     INNER JOIN audit_contributions c
                        ON c.contribution_id = v.target_contribution_id
                     WHERE c.worker_agent_id = ?1
                     ORDER BY v.created_at, v.verification_id",
                )
                .map_err(|error| error.to_string())?;
            let rows = statement
                .query_map(params![worker_agent_id], |row| row.get::<_, String>(0))
                .map_err(|error| error.to_string())?;
            rows.map(|row| {
                let json = row.map_err(|error| error.to_string())?;
                serde_json::from_str(&json).map_err(|error| error.to_string())
            })
            .collect::<Result<Vec<VerificationResult>, String>>()?
        };

        let mut bundles = Vec::new();
        for verification in verifications {
            let allocations = {
                let mut statement = connection
                    .prepare(
                        "SELECT allocation_json FROM credit_allocations
                         WHERE verification_id = ?1
                         ORDER BY issued_at, allocation_id",
                    )
                    .map_err(|error| error.to_string())?;
                let rows = statement
                    .query_map(params![verification.verification_id], |row| {
                        row.get::<_, String>(0)
                    })
                    .map_err(|error| error.to_string())?;
                rows.map(|row| {
                    let json = row.map_err(|error| error.to_string())?;
                    serde_json::from_str(&json).map_err(|error| error.to_string())
                })
                .collect::<Result<Vec<CreditAllocation>, String>>()?
            };
            bundles.push((verification, allocations));
        }
        Ok(bundles)
    }

    #[cfg(test)]
    pub fn verification_bundles_for_network(
        &self,
        limit: usize,
    ) -> Result<Vec<(VerificationResult, Vec<CreditAllocation>)>, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        let mut statement = connection
            .prepare(
                "SELECT verification_json FROM audit_verifications
                 ORDER BY created_at DESC, verification_id DESC
                 LIMIT ?1",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(params![limit as i64], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?;
        let verifications = rows
            .map(|row| {
                let json = row.map_err(|error| error.to_string())?;
                serde_json::from_str(&json).map_err(|error| error.to_string())
            })
            .collect::<Result<Vec<VerificationResult>, String>>()?;

        let mut bundles = Vec::new();
        for verification in verifications {
            let allocations = credit_allocations_for_verification_id_in_connection(
                &connection,
                &verification.verification_id,
            )?;
            bundles.push((verification, allocations));
        }
        Ok(bundles)
    }

    #[cfg(test)]
    pub fn work_unit_claims_for_requester(
        &self,
        requester_agent_id: &str,
    ) -> Result<Vec<AuditWorkUnitClaim>, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        let mut statement = connection
            .prepare(
                "SELECT claim_json FROM audit_work_unit_claims
                 WHERE requester_agent_id = ?1 AND status = 'claimed'
                 ORDER BY created_at, claim_id",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(params![requester_agent_id], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?;
        rows.map(|row| {
            let json = row.map_err(|error| error.to_string())?;
            serde_json::from_str(&json).map_err(|error| error.to_string())
        })
        .collect()
    }

    #[cfg(test)]
    pub fn work_unit_claims_for_network(
        &self,
        limit: usize,
    ) -> Result<Vec<AuditWorkUnitClaim>, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        let mut statement = connection
            .prepare(
                "SELECT claim_json FROM audit_work_unit_claims
                 WHERE status = 'claimed'
                 ORDER BY created_at DESC, claim_id DESC
                 LIMIT ?1",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(params![limit as i64], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?;
        rows.map(|row| {
            let json = row.map_err(|error| error.to_string())?;
            serde_json::from_str(&json).map_err(|error| error.to_string())
        })
        .collect()
    }

    #[cfg(test)]
    pub fn contributions_for_requester(
        &self,
        requester_agent_id: &str,
    ) -> Result<Vec<NodeContribution>, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        let mut statement = connection
            .prepare(
                "SELECT contribution_json FROM audit_contributions c
                 INNER JOIN protocol_audit_campaigns p
                    ON p.campaign_id = c.campaign_id
                 WHERE p.requester_agent_id = ?1
                 ORDER BY c.created_at, c.contribution_id",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(params![requester_agent_id], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?;
        rows.map(|row| {
            let json = row.map_err(|error| error.to_string())?;
            serde_json::from_str(&json).map_err(|error| error.to_string())
        })
        .collect()
    }

    #[cfg(test)]
    pub fn unverified_contributions_for_network(
        &self,
        limit: usize,
    ) -> Result<Vec<NodeContribution>, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        let mut statement = connection
            .prepare(
                "SELECT c.contribution_json FROM audit_contributions c
                 WHERE NOT EXISTS (
                    SELECT 1 FROM audit_verifications v
                    WHERE v.target_contribution_id = c.contribution_id
                 )
                 ORDER BY c.created_at DESC, c.contribution_id DESC
                 LIMIT ?1",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(params![limit as i64], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?;
        rows.map(|row| {
            let json = row.map_err(|error| error.to_string())?;
            serde_json::from_str(&json).map_err(|error| error.to_string())
        })
        .collect()
    }

    pub fn network_verification_candidates(
        &self,
        verifier_agent_id: &str,
        limit: usize,
    ) -> Result<Vec<NodeContribution>, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        let mut statement = connection
            .prepare(
                "SELECT c.contribution_json FROM audit_contributions c
                 WHERE c.worker_agent_id != ?1
                   AND NOT EXISTS (
                    SELECT 1 FROM audit_verifications v
                    WHERE v.target_contribution_id = c.contribution_id
                   )
                 ORDER BY c.created_at, c.contribution_id
                 LIMIT ?2",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(params![verifier_agent_id, limit as i64], |row| {
                row.get::<_, String>(0)
            })
            .map_err(|error| error.to_string())?;
        rows.map(|row| {
            let json = row.map_err(|error| error.to_string())?;
            serde_json::from_str(&json).map_err(|error| error.to_string())
        })
        .collect()
    }

    pub fn pending_network_verification_count_for_verifier(
        &self,
        verifier_agent_id: &str,
    ) -> Result<usize, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        let count = connection
            .query_row(
                "SELECT COUNT(*) FROM audit_contributions c
                 WHERE c.worker_agent_id != ?1
                   AND NOT EXISTS (
                    SELECT 1 FROM audit_verifications v
                    WHERE v.target_contribution_id = c.contribution_id
                 )",
                params![verifier_agent_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| error.to_string())?;
        Ok(count.max(0) as usize)
    }

    pub fn pending_contribution_count_for_worker(
        &self,
        worker_agent_id: &str,
    ) -> Result<usize, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        let count = connection
            .query_row(
                "SELECT COUNT(*) FROM audit_contributions c
                 WHERE c.worker_agent_id = ?1
                   AND NOT EXISTS (
                    SELECT 1 FROM audit_verifications v
                    WHERE v.target_contribution_id = c.contribution_id
                 )",
                params![worker_agent_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| error.to_string())?;
        Ok(count.max(0) as usize)
    }

    pub fn oldest_pending_contribution_time_for_worker(
        &self,
        worker_agent_id: &str,
    ) -> Result<Option<u64>, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        let timestamp = connection
            .query_row(
                "SELECT MIN(c.created_at) FROM audit_contributions c
                 WHERE c.worker_agent_id = ?1
                   AND NOT EXISTS (
                    SELECT 1 FROM audit_verifications v
                    WHERE v.target_contribution_id = c.contribution_id
                 )",
                params![worker_agent_id],
                |row| row.get::<_, Option<i64>>(0),
            )
            .map_err(|error| error.to_string())?;
        Ok(timestamp.map(|value| value.max(0) as u64))
    }

    pub fn latest_independent_verification_time_for_worker(
        &self,
        worker_agent_id: &str,
    ) -> Result<Option<u64>, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        let timestamp = connection
            .query_row(
                "SELECT MAX(v.created_at)
                 FROM audit_verifications v
                 JOIN audit_contributions c
                   ON c.contribution_id = v.target_contribution_id
                 WHERE c.worker_agent_id = ?1
                   AND v.verifier_agent_id != ?1",
                params![worker_agent_id],
                |row| row.get::<_, Option<i64>>(0),
            )
            .map_err(|error| error.to_string())?;
        Ok(timestamp.map(|value| value.max(0) as u64))
    }

    pub fn audit_labor_inventory(
        &self,
        local_agent_id: &str,
        limit: usize,
    ) -> Result<AuditLaborInventory, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        let mut campaign_statement = connection
            .prepare(
                "SELECT campaign_id FROM protocol_audit_campaigns
                 ORDER BY created_at DESC, campaign_id DESC
                 LIMIT ?1",
            )
            .map_err(|error| error.to_string())?;
        let campaign_ids = campaign_statement
            .query_map(params![limit as i64], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?
            .map(|row| row.map_err(|error| error.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        drop(campaign_statement);

        let mut claim_statement = connection
            .prepare(
                "SELECT claim_id FROM audit_work_unit_claims
                 WHERE status = 'claimed'
                 ORDER BY created_at DESC, claim_id DESC
                 LIMIT ?1",
            )
            .map_err(|error| error.to_string())?;
        let claim_ids = claim_statement
            .query_map(params![limit as i64], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?
            .map(|row| row.map_err(|error| error.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        drop(claim_statement);

        let mut contribution_statement = connection
            .prepare(
                "SELECT c.contribution_id FROM audit_contributions c
                 WHERE NOT EXISTS (
                    SELECT 1 FROM audit_verifications v
                    WHERE v.target_contribution_id = c.contribution_id
                 )
                 ORDER BY c.created_at DESC, c.contribution_id DESC
                 LIMIT ?1",
            )
            .map_err(|error| error.to_string())?;
        let contribution_ids = contribution_statement
            .query_map(params![limit as i64], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?
            .map(|row| row.map_err(|error| error.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        drop(contribution_statement);

        let mut verification_statement = connection
            .prepare(
                "SELECT verification_id FROM audit_verifications
                 ORDER BY created_at DESC, verification_id DESC
                 LIMIT ?1",
            )
            .map_err(|error| error.to_string())?;
        let verification_ids = verification_statement
            .query_map(params![limit as i64], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?
            .map(|row| row.map_err(|error| error.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        drop(verification_statement);

        let mut needs_statement = connection
            .prepare(
                "SELECT c.contribution_id FROM audit_contributions c
                 WHERE c.worker_agent_id = ?1
                   AND NOT EXISTS (
                    SELECT 1 FROM audit_verifications v
                    WHERE v.target_contribution_id = c.contribution_id
                   )
                 ORDER BY c.created_at, c.contribution_id
                 LIMIT ?2",
            )
            .map_err(|error| error.to_string())?;
        let needs_verifier_contribution_ids = needs_statement
            .query_map(params![local_agent_id, limit as i64], |row| {
                row.get::<_, String>(0)
            })
            .map_err(|error| error.to_string())?
            .map(|row| row.map_err(|error| error.to_string()))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(AuditLaborInventory {
            campaign_ids,
            claim_ids,
            contribution_ids,
            verification_ids,
            needs_verifier_contribution_ids,
        })
    }

    pub fn campaigns_by_ids(
        &self,
        campaign_ids: &[String],
    ) -> Result<Vec<ProtocolAuditCampaign>, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        let mut campaigns = Vec::new();
        for campaign_id in campaign_ids {
            let campaign_json = connection
                .query_row(
                    "SELECT campaign_json FROM protocol_audit_campaigns WHERE campaign_id = ?1",
                    params![campaign_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(|error| error.to_string())?;
            if let Some(json) = campaign_json {
                campaigns.push(serde_json::from_str(&json).map_err(|error| error.to_string())?);
            }
        }
        Ok(campaigns)
    }

    pub fn work_unit_claims_by_ids(
        &self,
        claim_ids: &[String],
    ) -> Result<Vec<AuditWorkUnitClaim>, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        let mut claims = Vec::new();
        for claim_id in claim_ids {
            let claim_json = connection
                .query_row(
                    "SELECT claim_json FROM audit_work_unit_claims
                     WHERE claim_id = ?1 AND status = 'claimed'",
                    params![claim_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(|error| error.to_string())?;
            if let Some(json) = claim_json {
                claims.push(serde_json::from_str(&json).map_err(|error| error.to_string())?);
            }
        }
        Ok(claims)
    }

    pub fn claims_for_contributions(
        &self,
        contributions: &[NodeContribution],
    ) -> Result<Vec<AuditWorkUnitClaim>, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        let mut claims = Vec::new();
        for contribution in contributions {
            if let Some(claim) = claim_for_contribution_in_connection(&connection, contribution)? {
                if !claims
                    .iter()
                    .any(|existing: &AuditWorkUnitClaim| existing.claim_id == claim.claim_id)
                {
                    claims.push(claim);
                }
            }
        }
        Ok(claims)
    }

    pub fn contributions_by_ids(
        &self,
        contribution_ids: &[String],
    ) -> Result<Vec<NodeContribution>, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        let mut contributions = Vec::new();
        for contribution_id in contribution_ids {
            let contribution_json = connection
                .query_row(
                    "SELECT contribution_json FROM audit_contributions WHERE contribution_id = ?1",
                    params![contribution_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(|error| error.to_string())?;
            if let Some(json) = contribution_json {
                contributions.push(serde_json::from_str(&json).map_err(|error| error.to_string())?);
            }
        }
        Ok(contributions)
    }

    pub fn contributions_for_verifications(
        &self,
        verification_ids: &[String],
    ) -> Result<Vec<NodeContribution>, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        let mut contributions = Vec::new();
        for verification_id in verification_ids {
            let contribution_json = connection
                .query_row(
                    "SELECT c.contribution_json
                     FROM audit_verifications v
                     INNER JOIN audit_contributions c
                        ON c.contribution_id = v.target_contribution_id
                     WHERE v.verification_id = ?1",
                    params![verification_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(|error| error.to_string())?;
            if let Some(json) = contribution_json {
                let contribution: NodeContribution =
                    serde_json::from_str(&json).map_err(|error| error.to_string())?;
                if !contributions.iter().any(|existing: &NodeContribution| {
                    existing.contribution_id == contribution.contribution_id
                }) {
                    contributions.push(contribution);
                }
            }
        }
        Ok(contributions)
    }

    pub fn verification_bundles_by_ids(
        &self,
        verification_ids: &[String],
    ) -> Result<Vec<(VerificationResult, Vec<CreditAllocation>)>, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        let mut bundles = Vec::new();
        for verification_id in verification_ids {
            let verification_json = connection
                .query_row(
                    "SELECT verification_json FROM audit_verifications WHERE verification_id = ?1",
                    params![verification_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(|error| error.to_string())?;
            if let Some(json) = verification_json {
                let verification: VerificationResult =
                    serde_json::from_str(&json).map_err(|error| error.to_string())?;
                let allocations = credit_allocations_for_verification_id_in_connection(
                    &connection,
                    &verification.verification_id,
                )?;
                bundles.push((verification, allocations));
            }
        }
        Ok(bundles)
    }

    pub fn stale_unverified_contributions_with_claims(
        &self,
        age_ms: u64,
        limit: usize,
    ) -> Result<Vec<StaleContributionRepair>, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        let cutoff = (now_millis() as i64).saturating_sub(ttl_ms_to_i64(age_ms));
        let contributions = {
            let mut statement = connection
                .prepare(
                    "SELECT c.contribution_json FROM audit_contributions c
                     WHERE c.created_at <= ?1
                       AND NOT EXISTS (
                        SELECT 1 FROM audit_verifications v
                        WHERE v.target_contribution_id = c.contribution_id
                       )
                     ORDER BY c.created_at, c.contribution_id
                     LIMIT ?2",
                )
                .map_err(|error| error.to_string())?;
            let rows = statement
                .query_map(params![cutoff, limit as i64], |row| row.get::<_, String>(0))
                .map_err(|error| error.to_string())?;
            rows.map(|row| {
                let json = row.map_err(|error| error.to_string())?;
                serde_json::from_str(&json).map_err(|error| error.to_string())
            })
            .collect::<Result<Vec<NodeContribution>, String>>()?
        };
        let mut repairs = Vec::new();
        for contribution in contributions {
            let campaign = campaign_in_connection(&connection, &contribution.campaign_id)?;
            let claim = claim_for_contribution_in_connection(&connection, &contribution)?.or(
                active_claim_for_worker_in_connection(
                    &connection,
                    &contribution.campaign_id,
                    &contribution.work_unit_id,
                    &contribution.worker_agent_id,
                )?,
            );
            repairs.push(StaleContributionRepair {
                campaign,
                claim,
                contribution,
            });
        }
        Ok(repairs)
    }

    pub fn queue_pending_labor_object(
        &self,
        object_kind: &str,
        object_id: &str,
        object_json: &str,
        reason: &str,
    ) -> Result<(), String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        let now = now_millis() as i64;
        connection
            .execute(
                "INSERT INTO audit_labor_pending_objects
                    (object_kind, object_id, object_json, status, reason, attempts, created_at, updated_at)
                 VALUES (?1, ?2, ?3, 'needs_dependency', ?4, 0, ?5, ?5)
                 ON CONFLICT(object_kind, object_id) DO UPDATE SET
                    object_json = excluded.object_json,
                    status = 'needs_dependency',
                    reason = excluded.reason,
                    updated_at = excluded.updated_at",
                params![object_kind, object_id, object_json, reason, now],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn pending_labor_objects(&self, limit: usize) -> Result<Vec<PendingLaborObject>, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        let mut statement = connection
            .prepare(
                "SELECT object_kind, object_id, object_json
                 FROM audit_labor_pending_objects
                 WHERE status = 'needs_dependency'
                 ORDER BY created_at, object_kind, object_id
                 LIMIT ?1",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(params![limit as i64], |row| {
                Ok(PendingLaborObject {
                    object_kind: row.get(0)?,
                    object_id: row.get(1)?,
                    object_json: row.get(2)?,
                })
            })
            .map_err(|error| error.to_string())?;
        rows.map(|row| row.map_err(|error| error.to_string()))
            .collect()
    }

    pub fn mark_pending_labor_object_settled(
        &self,
        object_kind: &str,
        object_id: &str,
    ) -> Result<(), String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        connection
            .execute(
                "UPDATE audit_labor_pending_objects
                 SET status = 'settled', reason = NULL, updated_at = ?3
                 WHERE object_kind = ?1 AND object_id = ?2",
                params![object_kind, object_id, now_millis() as i64],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn mark_pending_labor_object_rejected(
        &self,
        object_kind: &str,
        object_id: &str,
        reason: &str,
    ) -> Result<(), String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        connection
            .execute(
                "UPDATE audit_labor_pending_objects
                 SET status = 'rejected', reason = ?3, updated_at = ?4
                 WHERE object_kind = ?1 AND object_id = ?2",
                params![object_kind, object_id, reason, now_millis() as i64],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn refresh_pending_labor_object(
        &self,
        object_kind: &str,
        object_id: &str,
        reason: &str,
    ) -> Result<(), String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        connection
            .execute(
                "UPDATE audit_labor_pending_objects
                 SET reason = ?3, attempts = attempts + 1, updated_at = ?4
                 WHERE object_kind = ?1 AND object_id = ?2",
                params![object_kind, object_id, reason, now_millis() as i64],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn record_labor_event(
        &self,
        event_kind: &str,
        peer_id: Option<&str>,
        object_kind: Option<&str>,
        object_id: Option<&str>,
        accepted: bool,
        reason: Option<&str>,
        payload: &serde_json::Value,
    ) -> Result<(), String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        let event_id = format!("labor_event_{}", uuid::Uuid::new_v4().simple());
        connection
            .execute(
                "INSERT INTO audit_labor_events
                    (event_id, event_kind, peer_id, object_kind, object_id, accepted, reason,
                     payload_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    event_id,
                    event_kind,
                    peer_id,
                    object_kind,
                    object_id,
                    if accepted { 1 } else { 0 },
                    reason,
                    serde_json::to_string(payload).map_err(|error| error.to_string())?,
                    now_millis() as i64,
                ],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn expire_stale_claims(&self, claim_ttl_ms: u64) -> Result<usize, String> {
        let mut connection = self.connection.lock().map_err(|error| error.to_string())?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        let now = now_millis() as i64;
        let expired = expire_stale_claims_in_transaction(
            &transaction,
            now.saturating_sub(ttl_ms_to_i64(claim_ttl_ms)),
            now,
        )?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(expired)
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
            claims: claims_in_connection(&connection, campaign_id)?,
            contributions: contributions_in_connection(&connection, campaign_id)?,
            verifications: verifications_in_connection(&connection, campaign_id)?,
            credits: trusted_credits_in_connection(&connection, campaign_id)?,
        })
    }

    pub fn campaign_live_snapshot(
        &self,
        campaign_id: &str,
    ) -> Result<CampaignReportSnapshot, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        let campaign = campaign_in_connection(&connection, campaign_id)?;
        Ok(CampaignReportSnapshot {
            campaign,
            work_units: work_units_in_connection(&connection, campaign_id)?,
            claims: claims_in_connection(&connection, campaign_id)?,
            contributions: contributions_in_connection(&connection, campaign_id)?,
            verifications: verifications_in_connection(&connection, campaign_id)?,
            credits: Vec::new(),
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
                "PRAGMA foreign_keys = ON;

                 CREATE TABLE IF NOT EXISTS atp_store_meta (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL,
                    updated_at INTEGER NOT NULL
                 );

                 CREATE TABLE IF NOT EXISTS atp_events (
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
                    updated_at INTEGER NOT NULL,
                    FOREIGN KEY(campaign_id)
                        REFERENCES protocol_audit_campaigns(campaign_id)
                        ON DELETE CASCADE
                 );
                 CREATE INDEX IF NOT EXISTS audit_work_units_campaign
                    ON audit_work_units(campaign_id, created_at);
                 CREATE INDEX IF NOT EXISTS audit_work_units_campaign_status
                    ON audit_work_units(campaign_id, status, created_at, work_unit_id);

                 CREATE TABLE IF NOT EXISTS audit_work_unit_claims (
                    claim_id TEXT PRIMARY KEY,
                    campaign_id TEXT NOT NULL,
                    work_unit_id TEXT NOT NULL,
                    worker_agent_id TEXT NOT NULL,
                    requester_agent_id TEXT NOT NULL,
                    status TEXT NOT NULL,
                    claim_json TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    FOREIGN KEY(campaign_id)
                        REFERENCES protocol_audit_campaigns(campaign_id)
                        ON DELETE CASCADE,
                    FOREIGN KEY(work_unit_id)
                        REFERENCES audit_work_units(work_unit_id)
                        ON DELETE CASCADE
                 );
                 CREATE UNIQUE INDEX IF NOT EXISTS audit_work_unit_claims_active_unit
                    ON audit_work_unit_claims(campaign_id, work_unit_id)
                    WHERE status = 'claimed';
                 CREATE INDEX IF NOT EXISTS audit_work_unit_claims_campaign
                    ON audit_work_unit_claims(campaign_id, created_at);
                 CREATE INDEX IF NOT EXISTS audit_work_unit_claims_requester
                    ON audit_work_unit_claims(requester_agent_id, created_at, claim_id);
                 CREATE INDEX IF NOT EXISTS audit_work_unit_claims_status
                    ON audit_work_unit_claims(status, created_at, claim_id);
                 CREATE INDEX IF NOT EXISTS audit_work_unit_claims_worker_unit_status
                    ON audit_work_unit_claims(campaign_id, work_unit_id, worker_agent_id, status);

                 CREATE TABLE IF NOT EXISTS audit_contributions (
                    contribution_id TEXT PRIMARY KEY,
                    campaign_id TEXT NOT NULL,
                    work_unit_id TEXT NOT NULL,
                    worker_agent_id TEXT NOT NULL,
                    receipt_hash TEXT NOT NULL UNIQUE,
                    contribution_json TEXT NOT NULL,
                    status TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    FOREIGN KEY(campaign_id)
                        REFERENCES protocol_audit_campaigns(campaign_id)
                        ON DELETE CASCADE,
                    FOREIGN KEY(work_unit_id)
                        REFERENCES audit_work_units(work_unit_id)
                        ON DELETE CASCADE
                 );
                 CREATE INDEX IF NOT EXISTS audit_contributions_campaign
                    ON audit_contributions(campaign_id, created_at);
                 CREATE INDEX IF NOT EXISTS audit_contributions_created
                    ON audit_contributions(created_at, contribution_id);
                 CREATE INDEX IF NOT EXISTS audit_contributions_worker
                    ON audit_contributions(worker_agent_id, created_at, contribution_id);
                 CREATE INDEX IF NOT EXISTS audit_contributions_work_unit
                    ON audit_contributions(campaign_id, work_unit_id, created_at, contribution_id);

                 CREATE TABLE IF NOT EXISTS audit_verifications (
                    verification_id TEXT PRIMARY KEY,
                    campaign_id TEXT NOT NULL,
                    target_contribution_id TEXT NOT NULL,
                    verifier_agent_id TEXT NOT NULL,
                    decision TEXT NOT NULL,
                    verification_json TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    FOREIGN KEY(campaign_id)
                        REFERENCES protocol_audit_campaigns(campaign_id)
                        ON DELETE CASCADE,
                    FOREIGN KEY(target_contribution_id)
                        REFERENCES audit_contributions(contribution_id)
                        ON DELETE CASCADE
                 );
                 CREATE INDEX IF NOT EXISTS audit_verifications_campaign
                    ON audit_verifications(campaign_id, created_at);
                 CREATE UNIQUE INDEX IF NOT EXISTS audit_verifications_target_contribution
                    ON audit_verifications(target_contribution_id);
                 CREATE INDEX IF NOT EXISTS audit_verifications_verifier
                    ON audit_verifications(verifier_agent_id, created_at, verification_id);

                 CREATE TABLE IF NOT EXISTS credit_allocations (
                    allocation_id TEXT PRIMARY KEY,
                    campaign_id TEXT NOT NULL,
                    contribution_id TEXT NOT NULL,
                    verification_id TEXT NOT NULL,
                    receiver_agent_id TEXT NOT NULL,
                    total INTEGER NOT NULL,
                    allocation_json TEXT NOT NULL,
                    issued_at INTEGER NOT NULL,
                    FOREIGN KEY(campaign_id)
                        REFERENCES protocol_audit_campaigns(campaign_id)
                        ON DELETE CASCADE,
                    FOREIGN KEY(contribution_id)
                        REFERENCES audit_contributions(contribution_id)
                        ON DELETE CASCADE,
                    FOREIGN KEY(verification_id)
                        REFERENCES audit_verifications(verification_id)
                        ON DELETE CASCADE
                 );
                 CREATE INDEX IF NOT EXISTS credit_allocations_campaign_issued
                    ON credit_allocations(campaign_id, issued_at, allocation_id);
                 CREATE INDEX IF NOT EXISTS credit_allocations_verification_issued
                    ON credit_allocations(verification_id, issued_at, allocation_id);
                 CREATE INDEX IF NOT EXISTS credit_allocations_contribution
                    ON credit_allocations(contribution_id, issued_at, allocation_id);
                 CREATE TABLE IF NOT EXISTS audit_labor_pending_objects (
                    object_kind TEXT NOT NULL,
                    object_id TEXT NOT NULL,
                    object_json TEXT NOT NULL,
                    status TEXT NOT NULL,
                    reason TEXT,
                    attempts INTEGER NOT NULL DEFAULT 0,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    PRIMARY KEY(object_kind, object_id)
                 );
                 CREATE INDEX IF NOT EXISTS audit_labor_pending_objects_status
                    ON audit_labor_pending_objects(status, updated_at, object_kind, object_id);
                 CREATE TABLE IF NOT EXISTS audit_labor_events (
                    event_id TEXT PRIMARY KEY,
                    event_kind TEXT NOT NULL,
                    peer_id TEXT,
                    object_kind TEXT,
                    object_id TEXT,
                    accepted INTEGER NOT NULL,
                    reason TEXT,
                    payload_json TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS audit_labor_events_kind_created
                    ON audit_labor_events(event_kind, created_at);
                 CREATE INDEX IF NOT EXISTS audit_labor_events_peer_created
                    ON audit_labor_events(peer_id, created_at);
                 CREATE INDEX IF NOT EXISTS deliveries_transaction_updated
                    ON deliveries(transaction_id, updated_at);
                 CREATE INDEX IF NOT EXISTS protocol_audit_campaigns_requester_created
                    ON protocol_audit_campaigns(requester_agent_id, created_at);",
            )
            .map_err(|error| error.to_string())?;

        let updated_at = now_millis() as i64;
        for (key, value) in [
            (STORE_META_TESTNET_ID_KEY, ATP_STORE_TESTNET_ID),
            (STORE_META_APP_VERSION_KEY, env!("CARGO_PKG_VERSION")),
            (STORE_META_SCHEMA_KEY, STORE_SCHEMA_VERSION),
        ] {
            connection
                .execute(
                    "INSERT INTO atp_store_meta (key, value, updated_at)
                     VALUES (?1, ?2, ?3)
                     ON CONFLICT(key) DO UPDATE SET
                        value = excluded.value,
                        updated_at = excluded.updated_at",
                    params![key, value, updated_at],
                )
                .map_err(|error| error.to_string())?;
        }

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
    let mut attachments = Vec::new();
    if let Some(text) = job
        .attachment_text
        .as_ref()
        .filter(|text| !text.trim().is_empty())
    {
        attachments.push(crate::audit_labor::CampaignAttachment::from_text(
            "Requester attachment".to_string(),
            text.clone(),
        )?);
    }
    let brief = [
        Some(format!(
            "ATP transaction: {}. ATP Credits budget: {} {}.",
            job.id, job.compensation, job.currency
        )),
        job.audit_brief_text.clone(),
    ]
    .into_iter()
    .flatten()
    .filter(|text| !text.trim().is_empty())
    .collect::<Vec<_>>()
    .join("\n\n");
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
        Some(brief),
        None,
        attachments,
        job.custom_skill_text.clone(),
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
    worker_agent_id: Option<&str>,
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
    if let Some(worker_agent_id) = worker_agent_id {
        if work_unit.claimed_by_agent_id.is_none() {
            work_unit.claimed_by_agent_id = Some(worker_agent_id.to_string());
        }
    }
    let updated = connection
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
    expect_one_row(updated, "work unit status update")?;
    Ok(())
}

fn update_work_unit_claim_status(
    connection: &Connection,
    campaign_id: &str,
    work_unit_id: &str,
    claim: &AuditWorkUnitClaim,
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
    work_unit.status = claim.status.clone();
    work_unit.claimed_by_agent_id = Some(claim.worker_agent_id.clone());
    work_unit.claim_id = Some(claim.claim_id.clone());
    work_unit.claimed_at = Some(claim.created_at.clone());
    connection
        .execute(
            "UPDATE audit_work_units
             SET status = ?3, work_unit_json = ?4, updated_at = ?5
             WHERE campaign_id = ?1 AND work_unit_id = ?2",
            params![
                campaign_id,
                work_unit_id,
                claim.status,
                serde_json::to_string(&work_unit).map_err(|error| error.to_string())?,
                now_millis() as i64,
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn expire_stale_claims_in_transaction(
    transaction: &Transaction<'_>,
    cutoff: i64,
    now: i64,
) -> Result<usize, String> {
    let stale_claims = {
        let mut statement = transaction
            .prepare(
                "SELECT c.claim_id, c.campaign_id, c.work_unit_id
                 FROM audit_work_unit_claims c
                 WHERE c.status = 'claimed'
                   AND c.created_at <= ?1
                   AND NOT EXISTS (
                    SELECT 1 FROM audit_contributions n
                    WHERE n.campaign_id = c.campaign_id
                      AND n.work_unit_id = c.work_unit_id
                   )
                 ORDER BY c.created_at, c.claim_id",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(params![cutoff], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|error| error.to_string())?;
        rows.map(|row| row.map_err(|error| error.to_string()))
            .collect::<Result<Vec<_>, _>>()?
    };

    let mut expired = 0;
    for (claim_id, campaign_id, work_unit_id) in stale_claims {
        let updated = transaction
            .execute(
                "UPDATE audit_work_unit_claims
                 SET status = 'expired', updated_at = ?2
                 WHERE claim_id = ?1 AND status = 'claimed'",
                params![claim_id, now],
            )
            .map_err(|error| error.to_string())?;
        if updated == 1 {
            reset_work_unit_after_claim_expiry(
                transaction,
                &campaign_id,
                &work_unit_id,
                &claim_id,
                now,
            )?;
            expired += 1;
        }
    }
    Ok(expired)
}

fn reset_work_unit_after_claim_expiry(
    connection: &Connection,
    campaign_id: &str,
    work_unit_id: &str,
    claim_id: &str,
    now: i64,
) -> Result<(), String> {
    let row = connection
        .query_row(
            "SELECT status, work_unit_json FROM audit_work_units
             WHERE campaign_id = ?1 AND work_unit_id = ?2",
            params![campaign_id, work_unit_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let Some((status, json)) = row else {
        return Ok(());
    };
    if is_submitted_or_terminal_work_unit_status(&status) {
        return Ok(());
    }

    let mut work_unit: AuditWorkUnit =
        serde_json::from_str(&json).map_err(|error| error.to_string())?;
    if work_unit
        .claim_id
        .as_deref()
        .is_some_and(|active_claim_id| active_claim_id != claim_id)
    {
        return Ok(());
    }
    work_unit.status = "open".to_string();
    work_unit.claimed_by_agent_id = None;
    work_unit.claim_id = None;
    work_unit.claimed_at = None;

    connection
        .execute(
            "UPDATE audit_work_units
             SET status = 'open', work_unit_json = ?3, updated_at = ?4
             WHERE campaign_id = ?1
               AND work_unit_id = ?2
               AND status = 'claimed'",
            params![
                campaign_id,
                work_unit_id,
                serde_json::to_string(&work_unit).map_err(|error| error.to_string())?,
                now,
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

fn campaign_in_transaction(
    transaction: &Transaction<'_>,
    campaign_id: &str,
) -> Result<ProtocolAuditCampaign, String> {
    let json = transaction
        .query_row(
            "SELECT campaign_json FROM protocol_audit_campaigns WHERE campaign_id = ?1",
            params![campaign_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(|error| error.to_string())?;
    serde_json::from_str(&json).map_err(|error| error.to_string())
}

fn equivalent_campaign_in_transaction(
    transaction: &Transaction<'_>,
    campaign: &ProtocolAuditCampaign,
) -> Result<Option<ProtocolAuditCampaign>, String> {
    let mut statement = transaction
        .prepare(
            "SELECT campaign_json FROM protocol_audit_campaigns
             WHERE requester_agent_id = ?1
             ORDER BY created_at",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![campaign.requester_agent_id], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|error| error.to_string())?;
    for row in rows {
        let json = row.map_err(|error| error.to_string())?;
        let existing: ProtocolAuditCampaign =
            serde_json::from_str(&json).map_err(|error| error.to_string())?;
        if existing
            .repository
            .full_name
            .eq_ignore_ascii_case(&campaign.repository.full_name)
            && existing
                .repository
                .commit_sha
                .eq_ignore_ascii_case(&campaign.repository.commit_sha)
            && existing.scope_text.trim() == campaign.scope_text.trim()
        {
            return Ok(Some(existing));
        }
    }
    Ok(None)
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

fn work_unit_in_transaction(
    transaction: &Transaction<'_>,
    campaign_id: &str,
    work_unit_id: &str,
) -> Result<AuditWorkUnit, String> {
    let json = transaction
        .query_row(
            "SELECT work_unit_json FROM audit_work_units
             WHERE campaign_id = ?1 AND work_unit_id = ?2",
            params![campaign_id, work_unit_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(|error| error.to_string())?;
    serde_json::from_str(&json).map_err(|error| error.to_string())
}

fn claims_in_connection(
    connection: &Connection,
    campaign_id: &str,
) -> Result<Vec<AuditWorkUnitClaim>, String> {
    let mut statement = connection
        .prepare(
            "SELECT claim_json, status FROM audit_work_unit_claims
             WHERE campaign_id = ?1 ORDER BY created_at, claim_id",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![campaign_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| error.to_string())?;
    rows.map(|row| {
        let (json, status) = row.map_err(|error| error.to_string())?;
        claim_from_json_with_status(&json, &status)
    })
    .collect()
}

fn claim_from_json_with_status(json: &str, status: &str) -> Result<AuditWorkUnitClaim, String> {
    let mut claim: AuditWorkUnitClaim =
        serde_json::from_str(json).map_err(|error| error.to_string())?;
    if claim.status != status {
        claim.status = status.to_string();
    }
    Ok(claim)
}

fn active_claim_for_worker_in_connection(
    connection: &Connection,
    campaign_id: &str,
    work_unit_id: &str,
    worker_agent_id: &str,
) -> Result<Option<AuditWorkUnitClaim>, String> {
    let claim_json = connection
        .query_row(
            "SELECT claim_json FROM audit_work_unit_claims
             WHERE campaign_id = ?1
               AND work_unit_id = ?2
               AND worker_agent_id = ?3
               AND status = 'claimed'",
            params![campaign_id, work_unit_id, worker_agent_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let Some(claim_json) = claim_json else {
        return Ok(None);
    };
    let claim: AuditWorkUnitClaim =
        serde_json::from_str(&claim_json).map_err(|error| error.to_string())?;
    verify_signed_work_unit_claim(&claim)?;
    Ok(Some(claim))
}

fn claim_for_contribution_in_connection(
    connection: &Connection,
    contribution: &NodeContribution,
) -> Result<Option<AuditWorkUnitClaim>, String> {
    let contribution_created_at = millis_from_rfc3339(&contribution.created_at)?;
    let mut statement = connection
        .prepare(
            "SELECT claim_json, created_at FROM audit_work_unit_claims
             WHERE campaign_id = ?1
               AND work_unit_id = ?2
               AND worker_agent_id = ?3
             ORDER BY created_at DESC, claim_id DESC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(
            params![
                contribution.campaign_id,
                contribution.work_unit_id,
                contribution.worker_agent_id
            ],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .map_err(|error| error.to_string())?;
    for row in rows {
        let (claim_json, stored_claim_created_at) = row.map_err(|error| error.to_string())?;
        let claim: AuditWorkUnitClaim =
            serde_json::from_str(&claim_json).map_err(|error| error.to_string())?;
        verify_signed_work_unit_claim(&claim)?;
        if contribution_is_within_claim_window(
            &claim,
            stored_claim_created_at,
            contribution_created_at,
        )? {
            return Ok(Some(claim));
        }
    }
    Ok(None)
}

fn contribution_is_within_claim_window(
    claim: &AuditWorkUnitClaim,
    stored_claim_created_at: i64,
    contribution_created_at: i64,
) -> Result<bool, String> {
    let signed_claim_created_at = millis_from_rfc3339(&claim.created_at)?;
    let claim_started_at = stored_claim_created_at.min(signed_claim_created_at);
    let signed_expires_at = claim
        .expires_at
        .as_deref()
        .map(millis_from_rfc3339)
        .transpose()?
        .unwrap_or_else(|| {
            signed_claim_created_at.saturating_add(ttl_ms_to_i64(WORK_UNIT_CLAIM_TTL_MS))
        });
    let ttl_expires_at =
        stored_claim_created_at.saturating_add(ttl_ms_to_i64(WORK_UNIT_CLAIM_TTL_MS));
    let claim_expires_at = signed_expires_at.min(ttl_expires_at);
    Ok(
        contribution_created_at.saturating_add(CLAIM_CONTRIBUTION_CLOCK_SKEW_MS)
            >= claim_started_at
            && contribution_created_at <= claim_expires_at,
    )
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

fn trusted_credits_in_connection(
    connection: &Connection,
    campaign_id: &str,
) -> Result<Vec<CreditAllocation>, String> {
    let mut credits = Vec::new();
    for allocation in credits_in_connection(connection, campaign_id)? {
        if credit_allocation_trust(connection, &allocation)? == CreditAllocationTrust::Verified {
            credits.push(allocation);
        }
    }
    Ok(credits)
}

fn verification_bundle_for_contribution_in_connection(
    connection: &Connection,
    contribution_id: &str,
) -> Result<Option<(VerificationResult, Vec<CreditAllocation>)>, String> {
    let verification_json = connection
        .query_row(
            "SELECT verification_json FROM audit_verifications
             WHERE target_contribution_id = ?1
             ORDER BY created_at, verification_id
             LIMIT 1",
            params![contribution_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let Some(verification_json) = verification_json else {
        return Ok(None);
    };
    let verification: VerificationResult =
        serde_json::from_str(&verification_json).map_err(|error| error.to_string())?;
    let allocations = credit_allocations_for_verification_id_in_connection(
        connection,
        &verification.verification_id,
    )?;
    Ok(Some((verification, allocations)))
}

fn credit_allocations_for_verification_id_in_connection(
    connection: &Connection,
    verification_id: &str,
) -> Result<Vec<CreditAllocation>, String> {
    let mut statement = connection
        .prepare(
            "SELECT allocation_json FROM credit_allocations
             WHERE verification_id = ?1
             ORDER BY issued_at, allocation_id",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![verification_id], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?;
    rows.map(|row| {
        let json = row.map_err(|error| error.to_string())?;
        serde_json::from_str(&json).map_err(|error| error.to_string())
    })
    .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CreditAllocationTrust {
    Verified,
    Provisional,
    Invalid,
}

fn credit_allocations_for_verification(
    contribution: &NodeContribution,
    verification: &VerificationResult,
) -> Result<Vec<CreditAllocation>, String> {
    if verification.decision != "accepted" {
        return Ok(Vec::new());
    }
    if verification.verifier_agent_id == contribution.worker_agent_id {
        return Ok(Vec::new());
    }
    allocate_credits(contribution, verification)
}

fn validate_credit_allocation_bundle(
    contribution: &NodeContribution,
    verification: &VerificationResult,
    allocations: &[CreditAllocation],
) -> Result<(), String> {
    if verification.decision != "accepted" && !allocations.is_empty() {
        return Err("non-accepted verification cannot carry credit allocations".to_string());
    }
    if verification.decision == "accepted"
        && verification.verifier_agent_id == contribution.worker_agent_id
    {
        if allocations.is_empty() {
            return Ok(());
        }
        return Err("self-verification cannot carry earned ATP allocations".to_string());
    }
    let expected = credit_allocations_for_verification(contribution, verification)?;
    if !credit_allocation_terms_match_set(allocations, &expected) {
        return Err("credit allocation does not match verification bundle".to_string());
    }
    Ok(())
}

fn credit_allocation_trust(
    connection: &Connection,
    allocation: &CreditAllocation,
) -> Result<CreditAllocationTrust, String> {
    let contribution_json = connection
        .query_row(
            "SELECT contribution_json FROM audit_contributions WHERE contribution_id = ?1",
            params![allocation.contribution_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let Some(contribution_json) = contribution_json else {
        return Ok(CreditAllocationTrust::Invalid);
    };
    let contribution: NodeContribution =
        serde_json::from_str(&contribution_json).map_err(|error| error.to_string())?;

    let verification_json = connection
        .query_row(
            "SELECT verification_json FROM audit_verifications WHERE verification_id = ?1",
            params![allocation.verification_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let Some(verification_json) = verification_json else {
        return Ok(CreditAllocationTrust::Invalid);
    };
    let verification: VerificationResult =
        serde_json::from_str(&verification_json).map_err(|error| error.to_string())?;

    if verification.target_contribution_id != contribution.contribution_id
        || verification.campaign_id != contribution.campaign_id
        || allocation.campaign_id != contribution.campaign_id
        || allocation.contribution_receipt_hash != contribution.receipt_hash
    {
        return Ok(CreditAllocationTrust::Invalid);
    }
    if verification.verifier_agent_id == contribution.worker_agent_id {
        let expected = match allocate_provisional_credits(&contribution, &verification) {
            Ok(expected) => expected,
            Err(_) => return Ok(CreditAllocationTrust::Invalid),
        };
        return if expected.iter().any(|expected_allocation| {
            credit_allocation_terms_match(allocation, expected_allocation)
        }) {
            Ok(CreditAllocationTrust::Provisional)
        } else {
            Ok(CreditAllocationTrust::Invalid)
        };
    }
    let expected = match allocate_credits(&contribution, &verification) {
        Ok(expected) => expected,
        Err(_) => return Ok(CreditAllocationTrust::Invalid),
    };
    if expected
        .iter()
        .any(|expected_allocation| credit_allocation_terms_match(allocation, expected_allocation))
    {
        Ok(CreditAllocationTrust::Verified)
    } else {
        Ok(CreditAllocationTrust::Invalid)
    }
}

fn credit_allocation_terms_match_set(
    actual: &[CreditAllocation],
    expected: &[CreditAllocation],
) -> bool {
    if actual.len() != expected.len() {
        return false;
    }
    let mut matched = vec![false; expected.len()];
    actual.iter().all(|actual_allocation| {
        if let Some((index, _)) =
            expected
                .iter()
                .enumerate()
                .find(|(index, expected_allocation)| {
                    !matched[*index]
                        && credit_allocation_terms_match(actual_allocation, expected_allocation)
                })
        {
            matched[index] = true;
            true
        } else {
            false
        }
    })
}

fn credit_allocation_terms_match(actual: &CreditAllocation, expected: &CreditAllocation) -> bool {
    actual.profile == expected.profile
        && actual.profile_version == expected.profile_version
        && actual.campaign_id == expected.campaign_id
        && actual.contribution_id == expected.contribution_id
        && actual.verification_id == expected.verification_id
        && actual.receiver_agent_id == expected.receiver_agent_id
        && actual.contribution_receipt_hash == expected.contribution_receipt_hash
        && actual.buckets == expected.buckets
        && actual.total == expected.total
        && actual.formula == expected.formula
}

fn is_submitted_or_terminal_work_unit_status(status: &str) -> bool {
    matches!(
        status,
        "submitted" | "accepted" | "rejected" | "challenged" | "revision_requested"
    )
}

fn is_reviewed_terminal_work_unit_status(status: &str) -> bool {
    matches!(
        status,
        "accepted" | "rejected" | "challenged" | "revision_requested"
    )
}

fn ttl_ms_to_i64(ttl_ms: u64) -> i64 {
    i64::try_from(ttl_ms).unwrap_or(i64::MAX)
}

fn claim_expiry_cutoff(now: i64) -> i64 {
    now.saturating_sub(ttl_ms_to_i64(WORK_UNIT_CLAIM_TTL_MS))
}

fn work_unit_claim_is_expired(claim: &AuditWorkUnitClaim, now: i64) -> Result<bool, String> {
    if let Some(expires_at) = claim.expires_at.as_deref() {
        if millis_from_rfc3339(expires_at)? <= now {
            return Ok(true);
        }
    }
    let created_at = millis_from_rfc3339(&claim.created_at)?;
    Ok(created_at.saturating_add(ttl_ms_to_i64(WORK_UNIT_CLAIM_TTL_MS)) <= now)
}

fn expect_one_row(rows: usize, context: &str) -> Result<(), String> {
    if rows == 1 {
        Ok(())
    } else {
        Err(format!("{context} affected {rows} rows"))
    }
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
    Ok(data_dir()?.join(format!("{ATP_STORE_TESTNET_ID}.sqlite3")))
}

fn prepare_database_path_for_current_testnet(path: &Path) -> Result<(), String> {
    if !path.exists() || database_matches_current_testnet(path)? {
        return Ok(());
    }
    archive_database_files(path)
}

fn database_matches_current_testnet(path: &Path) -> Result<bool, String> {
    let connection = Connection::open(path).map_err(|error| error.to_string())?;
    if sqlite_table_exists(&connection, "atp_store_meta")? {
        let testnet_id = connection
            .query_row(
                "SELECT value FROM atp_store_meta WHERE key = ?1",
                params![STORE_META_TESTNET_ID_KEY],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        return Ok(testnet_id.as_deref() == Some(ATP_STORE_TESTNET_ID));
    }
    Ok(!database_has_ledger_rows(&connection)?)
}

fn sqlite_table_exists(connection: &Connection, table_name: &str) -> Result<bool, String> {
    let exists = connection
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            params![table_name],
            |_| Ok(()),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .is_some();
    Ok(exists)
}

fn database_has_ledger_rows(connection: &Connection) -> Result<bool, String> {
    for table in LEDGER_TABLES {
        if !sqlite_table_exists(connection, table)? {
            continue;
        }
        let sql = format!("SELECT COUNT(*) FROM {table}");
        let count = connection
            .query_row(&sql, [], |row| row.get::<_, i64>(0))
            .map_err(|error| error.to_string())?;
        if count > 0 {
            return Ok(true);
        }
    }
    Ok(false)
}

fn archive_database_files(path: &Path) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "database path has no parent directory".to_string())?;
    let archive_dir = parent.join("backups").join(format!(
        "{}-incompatible-{}",
        Utc::now().format("%Y-%m-%d-%H%M%S"),
        ATP_STORE_TESTNET_ID
    ));
    fs::create_dir_all(&archive_dir).map_err(|error| error.to_string())?;

    for source in [
        path.to_path_buf(),
        path_with_suffix(path, "-wal"),
        path_with_suffix(path, "-shm"),
    ] {
        if source.exists() {
            let file_name = source
                .file_name()
                .ok_or_else(|| "database file has no filename".to_string())?;
            fs::rename(&source, archive_dir.join(file_name)).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn path_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        atp::{agent_id, create_signed_envelope, create_signed_envelope_with_expiry, AtpVerb},
        audit_labor::{
            allocate_credits, sha256_ref, signed_contribution, signed_verification,
            signed_work_unit_claim, AuditFinding, ContributionArtifact, CoverageItem,
            CreditBuckets, RuntimeDescriptor, VerificationEvidence, VerificationResult,
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

    #[test]
    fn store_initialization_records_current_testnet_marker() {
        let store = test_store();
        let connection = store.connection.lock().unwrap();
        let marker = connection
            .query_row(
                "SELECT value FROM atp_store_meta WHERE key = ?1",
                params![STORE_META_TESTNET_ID_KEY],
                |row| row.get::<_, String>(0),
            )
            .unwrap();
        assert_eq!(marker, ATP_STORE_TESTNET_ID);
    }

    #[test]
    fn database_path_is_testnet_specific() {
        let filename = database_path()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert_eq!(filename, format!("{ATP_STORE_TESTNET_ID}.sqlite3"));
    }

    #[test]
    fn unmarked_database_with_ledger_rows_is_archived() {
        let root = std::env::temp_dir().join(format!("cyphes-store-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join(format!("{ATP_STORE_TESTNET_ID}.sqlite3"));
        {
            let connection = Connection::open(&path).unwrap();
            connection
                .execute_batch(
                    "CREATE TABLE protocol_audit_campaigns (campaign_id TEXT PRIMARY KEY);
                     INSERT INTO protocol_audit_campaigns (campaign_id) VALUES ('old-campaign');",
                )
                .unwrap();
        }
        prepare_database_path_for_current_testnet(&path).unwrap();

        assert!(!path.exists());
        let backup_root = root.join("backups");
        let backup_dir = fs::read_dir(&backup_root)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert!(backup_dir
            .join(format!("{ATP_STORE_TESTNET_ID}.sqlite3"))
            .is_file());

        fs::remove_dir_all(root).unwrap();
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
            None,
            Vec::new(),
            None,
            requester_agent,
        )
        .unwrap()
    }

    fn claim_work_unit(
        store: &AtpStore,
        worker: &libp2p::identity::Keypair,
        campaign: &ProtocolAuditCampaign,
        work_unit: &AuditWorkUnit,
    ) {
        let claim = signed_work_unit_claim(worker, campaign, work_unit).unwrap();
        store.record_work_unit_claim(&claim).unwrap();
    }

    fn resign_verification(
        keypair: &libp2p::identity::Keypair,
        verification: &mut VerificationResult,
    ) {
        let mut value = serde_json::to_value(&*verification).unwrap();
        let object = value.as_object_mut().unwrap();
        object.remove("verificationHash");
        object.remove("signature");
        verification.verification_hash = sha256_ref(&serde_jcs::to_vec(&value).unwrap());
        verification.signature = crate::atp::sign_canonical(keypair, &value).unwrap();
    }

    fn resign_contribution(
        keypair: &libp2p::identity::Keypair,
        contribution: &mut NodeContribution,
    ) {
        let mut value = serde_json::to_value(&*contribution).unwrap();
        let object = value.as_object_mut().unwrap();
        object.remove("contributionHash");
        object.remove("receiptHash");
        object.remove("signature");
        contribution.contribution_hash = sha256_ref(&serde_jcs::to_vec(&value).unwrap());
        let receipt_value = serde_json::json!({
            "profile": contribution.profile.clone(),
            "receiptType": "NodeContributionReceipt",
            "campaignId": contribution.campaign_id.clone(),
            "workUnitId": contribution.work_unit_id.clone(),
            "contributionId": contribution.contribution_id.clone(),
            "workerAgentId": contribution.worker_agent_id.clone(),
            "contributionHash": contribution.contribution_hash.clone(),
            "artifacts": contribution.artifacts.clone(),
            "createdAt": contribution.created_at.clone(),
        });
        contribution.receipt_hash = sha256_ref(&serde_jcs::to_vec(&receipt_value).unwrap());
        contribution.signature = crate::atp::sign_canonical(keypair, &value).unwrap();
    }

    fn resign_claim(keypair: &libp2p::identity::Keypair, claim: &mut AuditWorkUnitClaim) {
        let mut value = serde_json::to_value(&*claim).unwrap();
        let object = value.as_object_mut().unwrap();
        object.remove("claimHash");
        object.remove("signature");
        claim.claim_hash = sha256_ref(&serde_jcs::to_vec(&value).unwrap());
        claim.signature = crate::atp::sign_canonical(keypair, &value).unwrap();
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
            audit_brief_text: None,
            attachment_text: None,
            custom_skill_text: None,
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
            audit_brief_text: None,
            attachment_text: None,
            custom_skill_text: None,
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
    fn duplicate_protocol_campaign_returns_existing_campaign() {
        let store = test_store();
        let requester = libp2p::identity::Keypair::generate_ed25519();
        let campaign = labor_campaign(agent_id(&requester.public()));
        let first = store.create_protocol_campaign(&campaign).unwrap();
        let duplicate = ProtocolAuditCampaign::new(
            campaign.protocol_name.clone(),
            campaign.repository.clone(),
            campaign.scope_text.clone(),
            campaign.bounty_url.clone(),
            campaign.impacts_in_scope.clone(),
            campaign.out_of_scope.clone(),
            campaign.audit_brief_text.clone(),
            None,
            Vec::new(),
            None,
            campaign.requester_agent_id.clone(),
        )
        .unwrap();
        assert_ne!(first.campaign_id, duplicate.campaign_id);
        let second = store.create_protocol_campaign(&duplicate).unwrap();
        assert_eq!(second.campaign_id, first.campaign_id);
        assert_eq!(store.list_protocol_campaigns().unwrap().len(), 1);
        assert_eq!(store.list_work_units(&first.campaign_id).unwrap().len(), 7);
    }

    #[test]
    fn record_contribution_enforces_worker_pending_cap_across_entrypoints() {
        let store = test_store();
        let requester = libp2p::identity::Keypair::generate_ed25519();
        let worker = libp2p::identity::Keypair::generate_ed25519();
        let requester_agent = agent_id(&requester.public());
        let mut submitted = 0usize;

        let campaign_count = (MAX_PENDING_CONTRIBUTIONS_PER_WORKER / 7) + 2;
        for campaign_index in 0..campaign_count {
            let campaign = ProtocolAuditCampaign::new(
                format!("Backpressure Fixture {campaign_index}"),
                RepositoryTarget {
                    full_name: format!("fixture/repo-{campaign_index}"),
                    url: format!("https://github.com/fixture/repo-{campaign_index}"),
                    commit_sha: "fd1fbd9150426ca8ace9cee45b4acf912ae84f5b".to_string(),
                },
                format!("Backpressure fixture scope {campaign_index}."),
                None,
                vec!["Principal theft".to_string()],
                vec!["Best practice notes".to_string()],
                Some("Backpressure fixture".to_string()),
                None,
                Vec::new(),
                None,
                requester_agent.clone(),
            )
            .unwrap();
            store.create_protocol_campaign(&campaign).unwrap();
            let work_units = store.list_work_units(&campaign.campaign_id).unwrap();
            for work_unit in work_units {
                let contribution = signed_contribution(
                    &worker,
                    campaign.campaign_id.clone(),
                    work_unit.work_unit_id.clone(),
                    RuntimeDescriptor::deterministic_fixture(),
                    format!("Backpressure fixture contribution {submitted}."),
                    vec![],
                    vec![labor_artifact(&format!("backpressure-{submitted}.md"))],
                    vec![CoverageItem {
                        area: "backpressure".to_string(),
                        status: "completed".to_string(),
                        evidence: vec!["Pending cap exercised.".to_string()],
                    }],
                    vec!["no repository code execution".to_string()],
                )
                .unwrap();
                claim_work_unit(&store, &worker, &campaign, &work_unit);
                if submitted < MAX_PENDING_CONTRIBUTIONS_PER_WORKER {
                    store.record_contribution(&contribution).unwrap();
                    submitted += 1;
                    continue;
                }

                let error = store.record_contribution(&contribution).unwrap_err();
                assert!(error.contains("worker backpressure active"));
                assert_eq!(
                    store
                        .pending_contribution_count_for_worker(&agent_id(&worker.public()))
                        .unwrap(),
                    MAX_PENDING_CONTRIBUTIONS_PER_WORKER
                );
                return;
            }
        }

        panic!("expected pending contribution cap to be reached");
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
        claim_work_unit(&store, &worker, &campaign, &work_units[3]);
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
        assert_eq!(
            submitted_units
                .iter()
                .find(|unit| unit.work_unit_id == accepted_contribution.work_unit_id)
                .unwrap()
                .claimed_by_agent_id
                .as_deref(),
            Some(agent_id(&worker.public()).as_str())
        );
        store.record_contribution(&accepted_contribution).unwrap();
        let replay_snapshot = store
            .campaign_report_snapshot(&campaign.campaign_id)
            .unwrap();
        assert_eq!(replay_snapshot.contributions.len(), 1);
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
        let duplicate_verification = signed_verification(
            &verifier,
            campaign.campaign_id.clone(),
            accepted_contribution.contribution_id.clone(),
            "accepted".to_string(),
            "COVERAGE_ACCEPTED_RETRY".to_string(),
            "Retry should reuse the existing verification bundle.".to_string(),
            vec![VerificationEvidence {
                label: "receipt".to_string(),
                reference: accepted_contribution.receipt_hash.clone(),
            }],
            vec![labor_artifact("verification-retry.md")],
        )
        .unwrap();
        let duplicate_allocations = store.record_verification(&duplicate_verification).unwrap();
        let mut duplicate_ids = duplicate_allocations
            .iter()
            .map(|allocation| allocation.allocation_id.clone())
            .collect::<Vec<_>>();
        let mut allocation_ids = allocations
            .iter()
            .map(|allocation| allocation.allocation_id.clone())
            .collect::<Vec<_>>();
        duplicate_ids.sort();
        allocation_ids.sort();
        assert_eq!(duplicate_ids, allocation_ids);
        let replay_snapshot = store
            .campaign_report_snapshot(&campaign.campaign_id)
            .unwrap();
        assert_eq!(replay_snapshot.verifications.len(), 1);
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
        claim_work_unit(&store, &worker, &campaign, &work_units[4]);
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

    #[test]
    fn network_verification_candidates_exclude_self_and_clear_after_verification() {
        let store = test_store();
        let requester = libp2p::identity::Keypair::generate_ed25519();
        let worker = libp2p::identity::Keypair::generate_ed25519();
        let verifier = libp2p::identity::Keypair::generate_ed25519();
        let worker_agent = agent_id(&worker.public());
        let verifier_agent = agent_id(&verifier.public());
        let campaign = labor_campaign(agent_id(&requester.public()));
        store.create_protocol_campaign(&campaign).unwrap();
        let work_units = store.list_work_units(&campaign.campaign_id).unwrap();
        let contribution = signed_contribution(
            &worker,
            campaign.campaign_id.clone(),
            work_units[0].work_unit_id.clone(),
            RuntimeDescriptor::deterministic_fixture(),
            "Mapped signed network verification candidate.".to_string(),
            vec![AuditFinding {
                id: "NETWORK-COVERAGE-001".to_string(),
                title: "Network verification candidate".to_string(),
                severity: "informational".to_string(),
                status: "non_reportable".to_string(),
                impact: None,
                evidence: vec!["network-candidate.md".to_string()],
                reportable: false,
            }],
            vec![labor_artifact("network-candidate.md")],
            vec![CoverageItem {
                area: "network verification queue".to_string(),
                status: "covered".to_string(),
                evidence: vec!["Signed contribution is verifier-claimable.".to_string()],
            }],
            vec!["no repository code execution".to_string()],
        )
        .unwrap();
        claim_work_unit(&store, &worker, &campaign, &work_units[0]);
        store.record_contribution(&contribution).unwrap();
        assert_eq!(
            store
                .pending_network_verification_count_for_verifier(&worker_agent)
                .unwrap(),
            0
        );
        assert_eq!(
            store
                .pending_contribution_count_for_worker(&worker_agent)
                .unwrap(),
            1
        );
        assert_eq!(
            store
                .pending_network_verification_count_for_verifier(&verifier_agent)
                .unwrap(),
            1
        );

        assert!(store
            .network_verification_candidates(&worker_agent, 10)
            .unwrap()
            .is_empty());
        let candidates = store
            .network_verification_candidates(&verifier_agent, 10)
            .unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].contribution_id, contribution.contribution_id);
        assert_eq!(
            store
                .unverified_contributions_for_network(10)
                .unwrap()
                .len(),
            1
        );

        let verification = signed_verification(
            &verifier,
            campaign.campaign_id.clone(),
            contribution.contribution_id.clone(),
            "accepted".to_string(),
            "NETWORK_SIGNED_RECEIPT_ACCEPTED".to_string(),
            "Independent verifier accepted the signed contribution.".to_string(),
            vec![VerificationEvidence {
                label: "receipt".to_string(),
                reference: contribution.receipt_hash.clone(),
            }],
            vec![labor_artifact("network-verification.md")],
        )
        .unwrap();
        let allocations = store.record_verification(&verification).unwrap();
        assert_eq!(allocations.len(), 2);
        assert!(store
            .network_verification_candidates(&verifier_agent, 10)
            .unwrap()
            .is_empty());
        assert!(store
            .unverified_contributions_for_network(10)
            .unwrap()
            .is_empty());
        assert_eq!(
            store
                .pending_network_verification_count_for_verifier(&verifier_agent)
                .unwrap(),
            0
        );
        assert_eq!(
            store
                .pending_contribution_count_for_worker(&worker_agent)
                .unwrap(),
            0
        );
        let bundles = store.verification_bundles_for_network(10).unwrap();
        assert_eq!(bundles.len(), 1);
        assert_eq!(bundles[0].0.verification_id, verification.verification_id);
        assert_eq!(bundles[0].1.len(), allocations.len());
    }

    #[test]
    fn peer_sync_accepts_parallel_worker_receipt_for_submitted_unit() {
        let store = test_store();
        let requester = libp2p::identity::Keypair::generate_ed25519();
        let worker_a = libp2p::identity::Keypair::generate_ed25519();
        let worker_b = libp2p::identity::Keypair::generate_ed25519();
        let worker_a_agent = agent_id(&worker_a.public());
        let worker_b_agent = agent_id(&worker_b.public());
        let campaign = labor_campaign(agent_id(&requester.public()));
        store.create_protocol_campaign(&campaign).unwrap();
        let work_unit = store
            .list_work_units(&campaign.campaign_id)
            .unwrap()
            .into_iter()
            .find(|unit| unit.kind == "repo-inventory")
            .unwrap();

        let claim_a = signed_work_unit_claim(&worker_a, &campaign, &work_unit).unwrap();
        store.record_work_unit_claim(&claim_a).unwrap();
        let contribution_a = signed_contribution(
            &worker_a,
            campaign.campaign_id.clone(),
            work_unit.work_unit_id.clone(),
            RuntimeDescriptor::deterministic_fixture(),
            "Worker A submitted a signed receipt first.".to_string(),
            vec![],
            vec![labor_artifact("worker-a.md")],
            vec![CoverageItem {
                area: "parallel worker recovery".to_string(),
                status: "completed".to_string(),
                evidence: vec!["Worker A produced local work.".to_string()],
            }],
            vec!["no repository code execution".to_string()],
        )
        .unwrap();
        store.record_contribution(&contribution_a).unwrap();

        let claim_b = signed_work_unit_claim(&worker_b, &campaign, &work_unit).unwrap();
        let claim_error = store.record_work_unit_claim(&claim_b).unwrap_err();
        assert!(claim_error.contains("submitted or reviewed"));
        store.record_historical_work_unit_claim(&claim_b).unwrap();
        let contribution_b = signed_contribution(
            &worker_b,
            campaign.campaign_id.clone(),
            work_unit.work_unit_id.clone(),
            RuntimeDescriptor::deterministic_fixture(),
            "Worker B submitted a raced signed receipt through peer sync.".to_string(),
            vec![],
            vec![labor_artifact("worker-b.md")],
            vec![CoverageItem {
                area: "parallel worker recovery".to_string(),
                status: "completed".to_string(),
                evidence: vec!["Worker B receipt remains verifiable.".to_string()],
            }],
            vec!["no repository code execution".to_string()],
        )
        .unwrap();
        store.record_network_contribution(&contribution_b).unwrap();

        let candidates_for_a = store
            .network_verification_candidates(&worker_a_agent, 10)
            .unwrap();
        assert_eq!(candidates_for_a.len(), 1);
        assert_eq!(
            candidates_for_a[0].contribution_id,
            contribution_b.contribution_id
        );
        let candidates_for_b = store
            .network_verification_candidates(&worker_b_agent, 10)
            .unwrap();
        assert_eq!(candidates_for_b.len(), 1);
        assert_eq!(
            candidates_for_b[0].contribution_id,
            contribution_a.contribution_id
        );
        let snapshot = store
            .campaign_report_snapshot(&campaign.campaign_id)
            .unwrap();
        assert_eq!(snapshot.contributions.len(), 2);
    }

    #[test]
    fn unclaimed_worker_contributions_are_rejected() {
        let store = test_store();
        let requester = libp2p::identity::Keypair::generate_ed25519();
        let worker = libp2p::identity::Keypair::generate_ed25519();
        let campaign = labor_campaign(agent_id(&requester.public()));
        store.create_protocol_campaign(&campaign).unwrap();
        let work_unit = store
            .list_work_units(&campaign.campaign_id)
            .unwrap()
            .into_iter()
            .find(|unit| unit.kind == "repo-inventory")
            .unwrap();

        let contribution = signed_contribution(
            &worker,
            campaign.campaign_id.clone(),
            work_unit.work_unit_id.clone(),
            RuntimeDescriptor::deterministic_fixture(),
            "Tried to submit without a signed work-unit claim.".to_string(),
            vec![],
            vec![labor_artifact("unclaimed.md")],
            vec![CoverageItem {
                area: "claim enforcement".to_string(),
                status: "attempted".to_string(),
                evidence: vec!["Unclaimed submission should be rejected.".to_string()],
            }],
            vec!["no code execution".to_string()],
        )
        .unwrap();

        let error = store.record_contribution(&contribution).unwrap_err();
        assert!(error.contains("claimed by this worker"));
        let snapshot = store
            .campaign_report_snapshot(&campaign.campaign_id)
            .unwrap();
        assert!(snapshot.contributions.is_empty());
        assert_eq!(
            snapshot
                .work_units
                .iter()
                .find(|unit| unit.work_unit_id == work_unit.work_unit_id)
                .unwrap()
                .status,
            "open"
        );
    }

    #[test]
    fn work_unit_claims_are_signed_and_prevent_conflicting_worker_submissions() {
        let store = test_store();
        let requester = libp2p::identity::Keypair::generate_ed25519();
        let worker = libp2p::identity::Keypair::generate_ed25519();
        let other_worker = libp2p::identity::Keypair::generate_ed25519();
        let campaign = labor_campaign(agent_id(&requester.public()));
        store.create_protocol_campaign(&campaign).unwrap();
        let work_unit = store
            .list_work_units(&campaign.campaign_id)
            .unwrap()
            .into_iter()
            .find(|unit| unit.kind == "repo-inventory")
            .unwrap();

        let claim = signed_work_unit_claim(&worker, &campaign, &work_unit).unwrap();
        store.record_work_unit_claim(&claim).unwrap();
        let claimed = store
            .list_work_units(&campaign.campaign_id)
            .unwrap()
            .into_iter()
            .find(|unit| unit.work_unit_id == work_unit.work_unit_id)
            .unwrap();
        assert_eq!(claimed.status, "claimed");
        assert_eq!(
            claimed.claimed_by_agent_id.as_deref(),
            Some(agent_id(&worker.public()).as_str())
        );

        let conflicting_claim =
            signed_work_unit_claim(&other_worker, &campaign, &work_unit).unwrap();
        assert!(store.record_work_unit_claim(&conflicting_claim).is_err());

        let wrong_worker_contribution = signed_contribution(
            &other_worker,
            campaign.campaign_id.clone(),
            work_unit.work_unit_id.clone(),
            RuntimeDescriptor::deterministic_fixture(),
            "Attempted to submit work for another node's claim.".to_string(),
            vec![],
            vec![labor_artifact("wrong-worker.md")],
            vec![CoverageItem {
                area: "claim enforcement".to_string(),
                status: "attempted".to_string(),
                evidence: vec!["Submission should be rejected.".to_string()],
            }],
            vec!["no code execution".to_string()],
        )
        .unwrap();
        assert!(store
            .record_contribution(&wrong_worker_contribution)
            .is_err());

        let claimed_worker_contribution = signed_contribution(
            &worker,
            campaign.campaign_id.clone(),
            work_unit.work_unit_id.clone(),
            RuntimeDescriptor::deterministic_fixture(),
            "Submitted repo inventory for the claimed work unit.".to_string(),
            vec![],
            vec![labor_artifact("repo-map.md")],
            vec![CoverageItem {
                area: "repository inventory".to_string(),
                status: "completed".to_string(),
                evidence: vec!["Pinned tree inventoried.".to_string()],
            }],
            vec!["no code execution".to_string()],
        )
        .unwrap();
        store
            .record_contribution(&claimed_worker_contribution)
            .unwrap();
        let duplicate_submission = signed_contribution(
            &worker,
            campaign.campaign_id.clone(),
            claimed_worker_contribution.work_unit_id.clone(),
            RuntimeDescriptor::deterministic_fixture(),
            "Attempted to submit a second receipt for the claimed unit.".to_string(),
            vec![],
            vec![labor_artifact("repo-map-duplicate.md")],
            vec![CoverageItem {
                area: "repository inventory".to_string(),
                status: "duplicate".to_string(),
                evidence: vec!["Second submission should be rejected.".to_string()],
            }],
            vec!["no code execution".to_string()],
        )
        .unwrap();
        assert!(store.record_contribution(&duplicate_submission).is_err());
        let snapshot = store
            .campaign_report_snapshot(&campaign.campaign_id)
            .unwrap();
        assert_eq!(snapshot.claims.len(), 1);
        assert_eq!(snapshot.contributions.len(), 1);
        assert_eq!(
            store
                .work_unit_claims_for_requester(&agent_id(&requester.public()))
                .unwrap()
                .len(),
            1
        );
        let replayable_claims = store.work_unit_claims_for_network(10).unwrap();
        assert_eq!(replayable_claims.len(), 1);
        assert_eq!(replayable_claims[0].claim_id, claim.claim_id);
        assert_eq!(
            store
                .pending_contribution_count_for_worker(&agent_id(&worker.public()))
                .unwrap(),
            1
        );
        let replayable_contributions = store
            .contributions_for_requester(&agent_id(&requester.public()))
            .unwrap();
        assert_eq!(replayable_contributions.len(), 1);
        assert_eq!(
            replayable_contributions[0].contribution_id,
            claimed_worker_contribution.contribution_id
        );
    }

    #[test]
    fn stale_claim_expiry_reopens_work_and_blocks_late_submission() {
        let store = test_store();
        let requester = libp2p::identity::Keypair::generate_ed25519();
        let worker = libp2p::identity::Keypair::generate_ed25519();
        let campaign = labor_campaign(agent_id(&requester.public()));
        store.create_protocol_campaign(&campaign).unwrap();
        let work_unit = store
            .list_work_units(&campaign.campaign_id)
            .unwrap()
            .into_iter()
            .find(|unit| unit.kind == "repo-inventory")
            .unwrap();

        let claim = signed_work_unit_claim(&worker, &campaign, &work_unit).unwrap();
        store.record_work_unit_claim(&claim).unwrap();
        let stale_created_at = now_millis() as i64 - ttl_ms_to_i64(WORK_UNIT_CLAIM_TTL_MS) - 1_000;
        {
            let connection = store.connection.lock().unwrap();
            connection
                .execute(
                    "UPDATE audit_work_unit_claims
                     SET created_at = ?1, updated_at = ?1
                     WHERE claim_id = ?2",
                    params![stale_created_at, claim.claim_id],
                )
                .unwrap();
        }

        let expired = store.expire_stale_claims(WORK_UNIT_CLAIM_TTL_MS).unwrap();
        assert_eq!(expired, 1);
        let reopened = store
            .list_work_units(&campaign.campaign_id)
            .unwrap()
            .into_iter()
            .find(|unit| unit.work_unit_id == work_unit.work_unit_id)
            .unwrap();
        assert_eq!(reopened.status, "open");
        assert!(reopened.claimed_by_agent_id.is_none());
        assert!(reopened.claim_id.is_none());
        assert!(reopened.claimed_at.is_none());
        assert!(store.work_unit_claims_for_network(10).unwrap().is_empty());

        let contribution = signed_contribution(
            &worker,
            campaign.campaign_id.clone(),
            work_unit.work_unit_id.clone(),
            RuntimeDescriptor::deterministic_fixture(),
            "Late submission after a stale claim expired.".to_string(),
            vec![],
            vec![labor_artifact("late.md")],
            vec![CoverageItem {
                area: "stale claim".to_string(),
                status: "expired".to_string(),
                evidence: vec!["Late submissions must reclaim first.".to_string()],
            }],
            vec!["no code execution".to_string()],
        )
        .unwrap();
        let error = store.record_contribution(&contribution).unwrap_err();
        assert!(error.contains("claimed by this worker"));
        let snapshot = store
            .campaign_report_snapshot(&campaign.campaign_id)
            .unwrap();
        assert_eq!(snapshot.claims.len(), 1);
        assert_eq!(snapshot.claims[0].status, "expired");
    }

    #[test]
    fn delayed_contribution_replay_accepts_receipt_created_inside_claim_window() {
        let store = test_store();
        let requester = libp2p::identity::Keypair::generate_ed25519();
        let worker = libp2p::identity::Keypair::generate_ed25519();
        let campaign = labor_campaign(agent_id(&requester.public()));
        store.create_protocol_campaign(&campaign).unwrap();
        let work_unit = store
            .list_work_units(&campaign.campaign_id)
            .unwrap()
            .into_iter()
            .find(|unit| unit.kind == "repo-inventory")
            .unwrap();

        let mut claim = signed_work_unit_claim(&worker, &campaign, &work_unit).unwrap();
        store.record_work_unit_claim(&claim).unwrap();

        let historical_claim_created_at =
            now_millis() as i64 - ttl_ms_to_i64(WORK_UNIT_CLAIM_TTL_MS) - 60_000;
        let contribution_created_at =
            historical_claim_created_at + ttl_ms_to_i64(WORK_UNIT_CLAIM_TTL_MS) / 2;
        claim.created_at = rfc3339_from_millis(historical_claim_created_at as u64);
        claim.expires_at = Some(rfc3339_from_millis(
            (historical_claim_created_at + ttl_ms_to_i64(WORK_UNIT_CLAIM_TTL_MS)) as u64,
        ));
        resign_claim(&worker, &mut claim);
        {
            let connection = store.connection.lock().unwrap();
            connection
                .execute(
                    "UPDATE audit_work_unit_claims
                     SET claim_json = ?1, created_at = ?2, updated_at = ?2
                     WHERE claim_id = ?3",
                    params![
                        serde_json::to_string(&claim).unwrap(),
                        historical_claim_created_at,
                        claim.claim_id
                    ],
                )
                .unwrap();
        }

        let expired = store.expire_stale_claims(WORK_UNIT_CLAIM_TTL_MS).unwrap();
        assert_eq!(expired, 1);

        let mut contribution = signed_contribution(
            &worker,
            campaign.campaign_id.clone(),
            work_unit.work_unit_id.clone(),
            RuntimeDescriptor::deterministic_fixture(),
            "Receipt was produced while the claim was valid but delivered after expiry."
                .to_string(),
            vec![],
            vec![labor_artifact("delayed-replay.md")],
            vec![CoverageItem {
                area: "delayed replay".to_string(),
                status: "completed".to_string(),
                evidence: vec!["Receipt timestamp falls inside signed claim window.".to_string()],
            }],
            vec!["no code execution".to_string()],
        )
        .unwrap();
        contribution.created_at = rfc3339_from_millis(contribution_created_at as u64);
        resign_contribution(&worker, &mut contribution);

        store.record_contribution(&contribution).unwrap();
        let snapshot = store
            .campaign_report_snapshot(&campaign.campaign_id)
            .unwrap();
        assert_eq!(snapshot.contributions.len(), 1);
        assert_eq!(
            snapshot.contributions[0].contribution_id,
            contribution.contribution_id
        );
        assert_eq!(
            snapshot
                .work_units
                .iter()
                .find(|unit| unit.work_unit_id == work_unit.work_unit_id)
                .unwrap()
                .status,
            "submitted"
        );
    }

    #[test]
    fn expired_claim_replay_is_stored_as_historical_evidence() {
        let store = test_store();
        let requester = libp2p::identity::Keypair::generate_ed25519();
        let worker = libp2p::identity::Keypair::generate_ed25519();
        let campaign = labor_campaign(agent_id(&requester.public()));
        store.create_protocol_campaign(&campaign).unwrap();
        let work_unit = store
            .list_work_units(&campaign.campaign_id)
            .unwrap()
            .into_iter()
            .find(|unit| unit.kind == "repo-inventory")
            .unwrap();

        let mut claim = signed_work_unit_claim(&worker, &campaign, &work_unit).unwrap();
        let historical_claim_created_at =
            now_millis() as i64 - ttl_ms_to_i64(WORK_UNIT_CLAIM_TTL_MS) - 60_000;
        let contribution_created_at =
            historical_claim_created_at + ttl_ms_to_i64(WORK_UNIT_CLAIM_TTL_MS) / 2;
        claim.created_at = rfc3339_from_millis(historical_claim_created_at as u64);
        claim.expires_at = Some(rfc3339_from_millis(
            (historical_claim_created_at + ttl_ms_to_i64(WORK_UNIT_CLAIM_TTL_MS)) as u64,
        ));
        resign_claim(&worker, &mut claim);

        let active_error = store.record_work_unit_claim(&claim).unwrap_err();
        assert!(active_error.contains("claim has expired"));
        store.record_historical_work_unit_claim(&claim).unwrap();

        let snapshot = store
            .campaign_report_snapshot(&campaign.campaign_id)
            .unwrap();
        let stored_claim = snapshot
            .claims
            .iter()
            .find(|candidate| candidate.claim_id == claim.claim_id)
            .unwrap();
        assert_eq!(stored_claim.status, "expired");
        let reopened_unit = snapshot
            .work_units
            .iter()
            .find(|unit| unit.work_unit_id == work_unit.work_unit_id)
            .unwrap();
        assert_eq!(reopened_unit.status, "open");

        let mut contribution = signed_contribution(
            &worker,
            campaign.campaign_id.clone(),
            work_unit.work_unit_id.clone(),
            RuntimeDescriptor::deterministic_fixture(),
            "Receipt arrived after the claim expired on the verifier.".to_string(),
            vec![],
            vec![labor_artifact("historical-claim-replay.md")],
            vec![CoverageItem {
                area: "historical claim replay".to_string(),
                status: "completed".to_string(),
                evidence: vec!["Claim evidence was retained after expiry.".to_string()],
            }],
            vec!["no code execution".to_string()],
        )
        .unwrap();
        contribution.created_at = rfc3339_from_millis(contribution_created_at as u64);
        resign_contribution(&worker, &mut contribution);

        store.record_contribution(&contribution).unwrap();
    }

    #[test]
    fn contribution_replay_repairs_submitted_work_unit_shell() {
        let store = test_store();
        let requester = libp2p::identity::Keypair::generate_ed25519();
        let worker = libp2p::identity::Keypair::generate_ed25519();
        let campaign = labor_campaign(agent_id(&requester.public()));
        store.create_protocol_campaign(&campaign).unwrap();
        let work_unit = store
            .list_work_units(&campaign.campaign_id)
            .unwrap()
            .into_iter()
            .find(|unit| unit.kind == "repo-inventory")
            .unwrap();
        let claim = signed_work_unit_claim(&worker, &campaign, &work_unit).unwrap();
        store.record_work_unit_claim(&claim).unwrap();
        {
            let connection = store.connection.lock().unwrap();
            update_work_unit_status(
                &connection,
                &campaign.campaign_id,
                &work_unit.work_unit_id,
                "submitted",
                Some(claim.worker_agent_id.as_str()),
            )
            .unwrap();
        }

        let contribution = signed_contribution(
            &worker,
            campaign.campaign_id.clone(),
            work_unit.work_unit_id.clone(),
            RuntimeDescriptor::deterministic_fixture(),
            "Contribution row repairs a submitted work-unit shell.".to_string(),
            vec![],
            vec![labor_artifact("submitted-shell-repair.md")],
            vec![CoverageItem {
                area: "submitted shell replay".to_string(),
                status: "completed".to_string(),
                evidence: vec!["Signed contribution row was missing locally.".to_string()],
            }],
            vec!["no code execution".to_string()],
        )
        .unwrap();

        store.record_contribution(&contribution).unwrap();
        assert_eq!(
            store
                .network_verification_candidates(&agent_id(&requester.public()), 10)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn stale_receipt_repair_returns_claim_before_pending_contribution() {
        let store = test_store();
        let requester = libp2p::identity::Keypair::generate_ed25519();
        let worker = libp2p::identity::Keypair::generate_ed25519();
        let campaign = labor_campaign(agent_id(&requester.public()));
        store.create_protocol_campaign(&campaign).unwrap();
        let work_unit = store
            .list_work_units(&campaign.campaign_id)
            .unwrap()
            .into_iter()
            .find(|unit| unit.kind == "dependency-config-review")
            .unwrap();
        let claim = signed_work_unit_claim(&worker, &campaign, &work_unit).unwrap();
        store.record_work_unit_claim(&claim).unwrap();
        let contribution = signed_contribution(
            &worker,
            campaign.campaign_id.clone(),
            work_unit.work_unit_id.clone(),
            RuntimeDescriptor::deterministic_fixture(),
            "Pending receipt should be repair-rebroadcast with its claim.".to_string(),
            vec![],
            vec![labor_artifact("dependency-review.md")],
            vec![CoverageItem {
                area: "dependency and config review".to_string(),
                status: "completed".to_string(),
                evidence: vec!["Repair includes claim before contribution.".to_string()],
            }],
            vec!["no code execution".to_string()],
        )
        .unwrap();
        store.record_contribution(&contribution).unwrap();
        {
            let connection = store.connection.lock().unwrap();
            connection
                .execute(
                    "UPDATE audit_contributions
                     SET created_at = ?1
                     WHERE contribution_id = ?2",
                    params![now_millis() as i64 - 60_000, contribution.contribution_id],
                )
                .unwrap();
        }

        let repairs = store
            .stale_unverified_contributions_with_claims(1_000, 10)
            .unwrap();
        assert_eq!(repairs.len(), 1);
        assert_eq!(
            repairs[0]
                .claim
                .as_ref()
                .map(|claim| claim.claim_id.as_str()),
            Some(claim.claim_id.as_str())
        );
        assert_eq!(
            repairs[0].contribution.contribution_id,
            contribution.contribution_id
        );
    }

    #[test]
    fn labor_inventory_lists_and_fetches_repair_objects() {
        let store = test_store();
        let requester = libp2p::identity::Keypair::generate_ed25519();
        let worker = libp2p::identity::Keypair::generate_ed25519();
        let verifier = libp2p::identity::Keypair::generate_ed25519();
        let campaign = labor_campaign(agent_id(&requester.public()));
        store.create_protocol_campaign(&campaign).unwrap();
        let work_unit = store
            .list_work_units(&campaign.campaign_id)
            .unwrap()
            .into_iter()
            .find(|unit| unit.kind == "repo-inventory")
            .unwrap();
        let claim = signed_work_unit_claim(&worker, &campaign, &work_unit).unwrap();
        store.record_work_unit_claim(&claim).unwrap();
        let contribution = signed_contribution(
            &worker,
            campaign.campaign_id.clone(),
            work_unit.work_unit_id.clone(),
            RuntimeDescriptor::deterministic_fixture(),
            "Inventory should advertise this pending contribution.".to_string(),
            vec![],
            vec![labor_artifact("inventory.md")],
            vec![CoverageItem {
                area: "repository inventory".to_string(),
                status: "completed".to_string(),
                evidence: vec!["Inventory advertises unverified work.".to_string()],
            }],
            vec!["no code execution".to_string()],
        )
        .unwrap();
        store.record_contribution(&contribution).unwrap();

        let worker_agent = agent_id(&worker.public());
        let inventory = store.audit_labor_inventory(&worker_agent, 10).unwrap();
        assert!(inventory.claim_ids.contains(&claim.claim_id));
        assert!(inventory
            .contribution_ids
            .contains(&contribution.contribution_id));
        assert!(inventory
            .needs_verifier_contribution_ids
            .contains(&contribution.contribution_id));
        assert!(inventory.verification_ids.is_empty());
        assert_eq!(
            store
                .work_unit_claims_by_ids(&[claim.claim_id.clone(), "missing".to_string()])
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            store
                .contributions_by_ids(&[
                    contribution.contribution_id.clone(),
                    "missing".to_string()
                ])
                .unwrap()
                .len(),
            1
        );

        let verification = signed_verification(
            &verifier,
            campaign.campaign_id.clone(),
            contribution.contribution_id.clone(),
            "accepted".to_string(),
            "INVENTORY_FETCH_ACCEPTED".to_string(),
            "Independent verifier accepted the inventory fixture.".to_string(),
            vec![VerificationEvidence {
                label: "receipt".to_string(),
                reference: contribution.receipt_hash.clone(),
            }],
            vec![labor_artifact("verification.md")],
        )
        .unwrap();
        let allocations = allocate_credits(&contribution, &verification).unwrap();
        store
            .record_verification_bundle(&verification, &allocations)
            .unwrap();
        let settled_inventory = store.audit_labor_inventory(&worker_agent, 10).unwrap();
        assert!(!settled_inventory
            .contribution_ids
            .contains(&contribution.contribution_id));
        assert!(settled_inventory
            .verification_ids
            .contains(&verification.verification_id));
        let bundles = store
            .verification_bundles_by_ids(&[
                verification.verification_id.clone(),
                "missing".to_string(),
            ])
            .unwrap();
        assert_eq!(bundles.len(), 1);
        assert_eq!(bundles[0].0.verification_id, verification.verification_id);
        assert_eq!(bundles[0].1.len(), allocations.len());
    }

    #[test]
    fn verification_bundles_credit_the_worker_and_are_idempotent() {
        let store = test_store();
        let requester = libp2p::identity::Keypair::generate_ed25519();
        let worker = libp2p::identity::Keypair::generate_ed25519();
        let campaign = labor_campaign(agent_id(&requester.public()));
        store.create_protocol_campaign(&campaign).unwrap();
        let work_unit = store
            .list_work_units(&campaign.campaign_id)
            .unwrap()
            .into_iter()
            .find(|unit| unit.kind == "dependency-config-review")
            .unwrap();
        let contribution = signed_contribution(
            &worker,
            campaign.campaign_id.clone(),
            work_unit.work_unit_id.clone(),
            RuntimeDescriptor::deterministic_fixture(),
            "Reviewed dependency and configuration posture with bounded evidence.".to_string(),
            vec![],
            vec![labor_artifact("dependency-review.md")],
            vec![CoverageItem {
                area: "dependency and config review".to_string(),
                status: "completed".to_string(),
                evidence: vec!["No repository code execution.".to_string()],
            }],
            vec!["no code execution".to_string()],
        )
        .unwrap();
        claim_work_unit(&store, &worker, &campaign, &work_unit);
        store.record_contribution(&contribution).unwrap();
        let verification = signed_verification(
            &requester,
            campaign.campaign_id.clone(),
            contribution.contribution_id.clone(),
            "accepted".to_string(),
            "COVERAGE_ACCEPTED".to_string(),
            "Contribution is bounded, signed, and useful.".to_string(),
            vec![VerificationEvidence {
                label: "receipt".to_string(),
                reference: contribution.receipt_hash.clone(),
            }],
            vec![labor_artifact("verification.md")],
        )
        .unwrap();
        let allocations = allocate_credits(&contribution, &verification).unwrap();
        let worker_agent = agent_id(&worker.public());
        let worker_total = allocations
            .iter()
            .filter(|allocation| allocation.receiver_agent_id == worker_agent)
            .map(|allocation| allocation.total)
            .sum::<u32>();

        let first = store
            .record_verification_bundle(&verification, &allocations)
            .unwrap();
        assert_eq!(first.len(), allocations.len());
        let bundles = store
            .verification_bundles_for_worker(&worker_agent)
            .unwrap();
        assert_eq!(bundles.len(), 1);
        assert_eq!(bundles[0].0.verification_id, verification.verification_id);
        assert_eq!(bundles[0].1.len(), allocations.len());
        let summary = store.credit_summary(&worker_agent).unwrap();
        assert_eq!(summary.total, worker_total);

        store
            .record_verification_bundle(&verification, &allocations)
            .unwrap();
        let duplicate_summary = store.credit_summary(&worker_agent).unwrap();
        assert_eq!(duplicate_summary.total, worker_total);
    }

    #[test]
    fn labor_object_preflight_skips_known_contributions_and_verifications() {
        let store = test_store();
        let requester = libp2p::identity::Keypair::generate_ed25519();
        let worker = libp2p::identity::Keypair::generate_ed25519();
        let campaign = labor_campaign(agent_id(&requester.public()));
        store.create_protocol_campaign(&campaign).unwrap();
        let work_unit = store
            .list_work_units(&campaign.campaign_id)
            .unwrap()
            .into_iter()
            .find(|unit| unit.kind == "dependency-config-review")
            .unwrap();
        let contribution = signed_contribution(
            &worker,
            campaign.campaign_id.clone(),
            work_unit.work_unit_id.clone(),
            RuntimeDescriptor::deterministic_fixture(),
            "Reviewed dependency and configuration posture with bounded evidence.".to_string(),
            vec![],
            vec![labor_artifact("dependency-review.md")],
            vec![CoverageItem {
                area: "dependency and config review".to_string(),
                status: "completed".to_string(),
                evidence: vec!["No repository code execution.".to_string()],
            }],
            vec!["no code execution".to_string()],
        )
        .unwrap();
        claim_work_unit(&store, &worker, &campaign, &work_unit);
        assert_eq!(
            store.contribution_preflight_status(&contribution).unwrap(),
            LaborObjectPreflight::New
        );
        store.record_contribution(&contribution).unwrap();
        assert_eq!(
            store.contribution_preflight_status(&contribution).unwrap(),
            LaborObjectPreflight::Duplicate("contribution already known")
        );
        let mut same_receipt = contribution.clone();
        same_receipt.contribution_id = "contribution_same_receipt_replay".to_string();
        assert_eq!(
            store.contribution_preflight_status(&same_receipt).unwrap(),
            LaborObjectPreflight::Duplicate("contribution receipt hash already known")
        );

        let verification = signed_verification(
            &requester,
            campaign.campaign_id.clone(),
            contribution.contribution_id.clone(),
            "accepted".to_string(),
            "COVERAGE_ACCEPTED".to_string(),
            "Contribution is bounded, signed, and useful.".to_string(),
            vec![VerificationEvidence {
                label: "receipt".to_string(),
                reference: contribution.receipt_hash.clone(),
            }],
            vec![labor_artifact("verification.md")],
        )
        .unwrap();
        let allocations = allocate_credits(&contribution, &verification).unwrap();
        assert_eq!(
            store
                .verification_bundle_preflight_status(&verification)
                .unwrap(),
            LaborObjectPreflight::New
        );
        store
            .record_verification_bundle(&verification, &allocations)
            .unwrap();
        assert_eq!(
            store
                .verification_bundle_preflight_status(&verification)
                .unwrap(),
            LaborObjectPreflight::Duplicate("verification already known")
        );
        let mut second_verification = verification.clone();
        second_verification.verification_id = "verification_duplicate_target".to_string();
        assert_eq!(
            store
                .verification_bundle_preflight_status(&second_verification)
                .unwrap(),
            LaborObjectPreflight::Superseded("contribution already has a verification bundle")
        );
    }

    #[test]
    fn verification_bundle_rejects_verification_id_collision_for_different_target() {
        let store = test_store();
        let requester = libp2p::identity::Keypair::generate_ed25519();
        let worker = libp2p::identity::Keypair::generate_ed25519();
        let second_worker = libp2p::identity::Keypair::generate_ed25519();
        let campaign = labor_campaign(agent_id(&requester.public()));
        store.create_protocol_campaign(&campaign).unwrap();
        let work_units = store.list_work_units(&campaign.campaign_id).unwrap();
        let first_unit = work_units
            .iter()
            .find(|unit| unit.kind == "repo-inventory")
            .unwrap()
            .clone();
        let second_unit = work_units
            .iter()
            .find(|unit| unit.kind == "dependency-config-review")
            .unwrap()
            .clone();

        let first_contribution = signed_contribution(
            &worker,
            campaign.campaign_id.clone(),
            first_unit.work_unit_id.clone(),
            RuntimeDescriptor::deterministic_fixture(),
            "Mapped repository inventory for collision fixture.".to_string(),
            vec![],
            vec![labor_artifact("inventory.md")],
            vec![CoverageItem {
                area: "repository inventory".to_string(),
                status: "completed".to_string(),
                evidence: vec!["Inventory was bounded.".to_string()],
            }],
            vec!["no code execution".to_string()],
        )
        .unwrap();
        claim_work_unit(&store, &worker, &campaign, &first_unit);
        store.record_contribution(&first_contribution).unwrap();

        let second_contribution = signed_contribution(
            &second_worker,
            campaign.campaign_id.clone(),
            second_unit.work_unit_id.clone(),
            RuntimeDescriptor::deterministic_fixture(),
            "Mapped dependency posture for collision fixture.".to_string(),
            vec![],
            vec![labor_artifact("dependency-review.md")],
            vec![CoverageItem {
                area: "dependency and config review".to_string(),
                status: "completed".to_string(),
                evidence: vec!["Dependency posture was bounded.".to_string()],
            }],
            vec!["no code execution".to_string()],
        )
        .unwrap();
        claim_work_unit(&store, &second_worker, &campaign, &second_unit);
        store.record_contribution(&second_contribution).unwrap();

        let first_verification = signed_verification(
            &requester,
            campaign.campaign_id.clone(),
            first_contribution.contribution_id.clone(),
            "accepted".to_string(),
            "COLLISION_FIXTURE_ACCEPTED".to_string(),
            "First contribution is accepted.".to_string(),
            vec![VerificationEvidence {
                label: "receipt".to_string(),
                reference: first_contribution.receipt_hash.clone(),
            }],
            vec![labor_artifact("verification-first.md")],
        )
        .unwrap();
        let first_allocations = allocate_credits(&first_contribution, &first_verification).unwrap();
        store
            .record_verification_bundle(&first_verification, &first_allocations)
            .unwrap();

        let mut colliding_verification = signed_verification(
            &requester,
            campaign.campaign_id.clone(),
            second_contribution.contribution_id.clone(),
            "accepted".to_string(),
            "COLLIDING_VERIFICATION_ID".to_string(),
            "Second contribution tries to reuse the first verification id.".to_string(),
            vec![VerificationEvidence {
                label: "receipt".to_string(),
                reference: second_contribution.receipt_hash.clone(),
            }],
            vec![labor_artifact("verification-second.md")],
        )
        .unwrap();
        colliding_verification.verification_id = first_verification.verification_id.clone();
        resign_verification(&requester, &mut colliding_verification);
        let colliding_allocations =
            allocate_credits(&second_contribution, &colliding_verification).unwrap();

        let error = store
            .record_verification_bundle(&colliding_verification, &colliding_allocations)
            .unwrap_err();
        assert!(error.contains("verification id already exists"));
        assert!(store
            .verification_bundle_for_contribution(&second_contribution.contribution_id)
            .unwrap()
            .is_none());
        let snapshot = store
            .campaign_report_snapshot(&campaign.campaign_id)
            .unwrap();
        assert_eq!(snapshot.verifications.len(), 1);
        assert_eq!(
            snapshot
                .work_units
                .iter()
                .find(|unit| unit.work_unit_id == second_contribution.work_unit_id)
                .unwrap()
                .status,
            "submitted"
        );
    }

    #[test]
    fn self_verification_records_no_earned_credit() {
        let store = test_store();
        let requester = libp2p::identity::Keypair::generate_ed25519();
        let campaign = labor_campaign(agent_id(&requester.public()));
        store.create_protocol_campaign(&campaign).unwrap();
        let work_unit = store
            .list_work_units(&campaign.campaign_id)
            .unwrap()
            .into_iter()
            .find(|unit| unit.kind == "repo-inventory")
            .unwrap();
        claim_work_unit(&store, &requester, &campaign, &work_unit);
        let contribution = signed_contribution(
            &requester,
            campaign.campaign_id.clone(),
            work_unit.work_unit_id.clone(),
            RuntimeDescriptor::deterministic_fixture(),
            "Mapped repository inventory locally.".to_string(),
            vec![],
            vec![labor_artifact("inventory.md")],
            vec![CoverageItem {
                area: "repository inventory".to_string(),
                status: "completed".to_string(),
                evidence: vec!["No repository code execution.".to_string()],
            }],
            vec!["no code execution".to_string()],
        )
        .unwrap();
        store.record_contribution(&contribution).unwrap();
        let verification = signed_verification(
            &requester,
            campaign.campaign_id.clone(),
            contribution.contribution_id.clone(),
            "accepted".to_string(),
            "SELF_PREVIEW_ACCEPTED".to_string(),
            "Self-verification is a local preview and cannot mint earned ATP.".to_string(),
            vec![],
            vec![labor_artifact("verification.md")],
        )
        .unwrap();

        let allocations = store.record_verification(&verification).unwrap();
        assert!(allocations.is_empty());
        let summary = store
            .credit_summary(&agent_id(&requester.public()))
            .unwrap();
        assert_eq!(summary.total, 0);
        assert_eq!(summary.provisional_total, 0);

        let forged = CreditAllocation {
            profile: "cyphes.credit-ledger".to_string(),
            profile_version: "0.1".to_string(),
            allocation_id: "credit_forged_self_preview".to_string(),
            campaign_id: campaign.campaign_id.clone(),
            contribution_id: contribution.contribution_id.clone(),
            verification_id: verification.verification_id.clone(),
            receiver_agent_id: agent_id(&requester.public()),
            contribution_receipt_hash: contribution.receipt_hash.clone(),
            buckets: CreditBuckets {
                participation: 1_000_000,
                verification: 0,
                coverage: 0,
                finding: 0,
                bonus_allocation_placeholder: 0,
            },
            total: 1_000_000,
            formula: "forged local sqlite edit".to_string(),
            issued_at: now_rfc3339(),
        };
        let connection = store.connection.lock().unwrap();
        connection
            .execute(
                "INSERT INTO credit_allocations
                    (allocation_id, campaign_id, contribution_id, verification_id,
                     receiver_agent_id, total, allocation_json, issued_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    forged.allocation_id,
                    forged.campaign_id,
                    forged.contribution_id,
                    forged.verification_id,
                    forged.receiver_agent_id,
                    forged.total as i64,
                    serde_json::to_string(&forged).unwrap(),
                    millis_from_rfc3339(&forged.issued_at).unwrap(),
                ],
            )
            .unwrap();
        drop(connection);

        let summary = store
            .credit_summary(&agent_id(&requester.public()))
            .unwrap();
        assert_eq!(summary.total, 0);
        assert_eq!(summary.provisional_total, 0);
    }

    #[test]
    fn credit_summary_ignores_tampered_sqlite_allocations() {
        let store = test_store();
        let requester = libp2p::identity::Keypair::generate_ed25519();
        let worker = libp2p::identity::Keypair::generate_ed25519();
        let campaign = labor_campaign(agent_id(&requester.public()));
        store.create_protocol_campaign(&campaign).unwrap();
        let work_unit = store
            .list_work_units(&campaign.campaign_id)
            .unwrap()
            .into_iter()
            .find(|unit| unit.kind == "dependency-config-review")
            .unwrap();
        let contribution = signed_contribution(
            &worker,
            campaign.campaign_id.clone(),
            work_unit.work_unit_id.clone(),
            RuntimeDescriptor::deterministic_fixture(),
            "Reviewed dependency and configuration posture with bounded evidence.".to_string(),
            vec![],
            vec![labor_artifact("dependency-review.md")],
            vec![CoverageItem {
                area: "dependency and config review".to_string(),
                status: "completed".to_string(),
                evidence: vec!["No repository code execution.".to_string()],
            }],
            vec!["no code execution".to_string()],
        )
        .unwrap();
        claim_work_unit(&store, &worker, &campaign, &work_unit);
        store.record_contribution(&contribution).unwrap();
        let verification = signed_verification(
            &requester,
            campaign.campaign_id.clone(),
            contribution.contribution_id.clone(),
            "accepted".to_string(),
            "COVERAGE_ACCEPTED".to_string(),
            "Contribution is bounded, signed, and useful.".to_string(),
            vec![VerificationEvidence {
                label: "receipt".to_string(),
                reference: contribution.receipt_hash.clone(),
            }],
            vec![labor_artifact("verification.md")],
        )
        .unwrap();
        let allocations = store.record_verification(&verification).unwrap();
        let worker_agent = agent_id(&worker.public());
        let verified_total = store.credit_summary(&worker_agent).unwrap().total;
        assert!(verified_total > 0);

        let mut forged = allocations
            .iter()
            .find(|allocation| allocation.receiver_agent_id == worker_agent)
            .unwrap()
            .clone();
        forged.allocation_id = "credit_forged_local_sqlite_row".to_string();
        forged.total = 1_000_000;
        let connection = store.connection.lock().unwrap();
        connection
            .execute(
                "INSERT INTO credit_allocations
                    (allocation_id, campaign_id, contribution_id, verification_id,
                     receiver_agent_id, total, allocation_json, issued_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    forged.allocation_id,
                    forged.campaign_id,
                    forged.contribution_id,
                    forged.verification_id,
                    forged.receiver_agent_id,
                    forged.total as i64,
                    serde_json::to_string(&forged).unwrap(),
                    millis_from_rfc3339(&forged.issued_at).unwrap(),
                ],
            )
            .unwrap();
        drop(connection);

        let summary = store.credit_summary(&worker_agent).unwrap();
        assert_eq!(summary.total, verified_total);
        assert!(summary.total < 1_000_000);
        let snapshot = store
            .campaign_report_snapshot(&campaign.campaign_id)
            .unwrap();
        let snapshot_worker_total = snapshot
            .credits
            .iter()
            .filter(|allocation| allocation.receiver_agent_id == worker_agent)
            .map(|allocation| allocation.total)
            .sum::<u32>();
        assert_eq!(snapshot_worker_total, verified_total);
        assert!(!snapshot
            .credits
            .iter()
            .any(|allocation| allocation.allocation_id == "credit_forged_local_sqlite_row"));
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
                    audit_brief_text: None,
                    attachment_text: None,
                    custom_skill_text: None,
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
