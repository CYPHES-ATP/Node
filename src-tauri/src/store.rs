use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};

use crate::{
    atp::{event_hash, now_rfc3339, transition, verify_envelope, AtpAck, AtpEnvelope, AtpVerb},
    audit_profile::{contract_hash, validate_contract, AuditContract, RepositoryTarget},
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
    pub acknowledged_peers: u64,
    pub origin: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
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
}

#[derive(Clone)]
pub struct AtpStore {
    connection: Arc<Mutex<Connection>>,
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
        if envelope.prev != current.last_event_hash {
            return Err("ATP event does not extend the committed transaction head".to_string());
        }

        let next_state = transition(current.state.as_deref(), envelope.verb)?;
        let body: AuditEventBody =
            serde_json::from_value(envelope.body.clone()).map_err(|error| error.to_string())?;
        validate_body(&body, envelope, &current)?;
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

        apply_audit_event(
            &transaction,
            &body,
            envelope,
            &hash,
            next_state,
            receiver_agent_id,
        )?;
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
                    (event_hash, transaction_id, peer_id, accepted, duplicate, reason_code, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(event_hash, peer_id) DO UPDATE SET
                    accepted = excluded.accepted,
                    duplicate = excluded.duplicate,
                    reason_code = excluded.reason_code,
                    updated_at = excluded.updated_at",
                params![
                    ack.event_hash,
                    ack.transaction_id,
                    peer_id,
                    ack.accepted,
                    ack.duplicate,
                    ack.reason_code,
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

                 CREATE TABLE IF NOT EXISTS deliveries (
                    event_hash TEXT NOT NULL,
                    transaction_id TEXT NOT NULL,
                    peer_id TEXT NOT NULL,
                    accepted INTEGER NOT NULL,
                    duplicate INTEGER NOT NULL,
                    reason_code TEXT,
                    updated_at INTEGER NOT NULL,
                    PRIMARY KEY(event_hash, peer_id)
                 );",
            )
            .map_err(|error| error.to_string())
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
                     WHERE c.transaction_id = audit_jobs.transaction_id)
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
    }
    Ok(())
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
    }
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
        origin: row.get(14)?,
        acknowledged_peers: row.get::<_, i64>(15)? as u64,
    })
}

pub fn now_millis() -> u64 {
    chrono::Utc::now().timestamp_millis() as u64
}

fn database_path() -> Result<PathBuf, String> {
    if let Ok(data_dir) = std::env::var("CYPHES_DATA_DIR") {
        return Ok(PathBuf::from(data_dir).join("atp.sqlite3"));
    }
    let home = dirs::home_dir().ok_or_else(|| "Could not resolve home directory".to_string())?;
    Ok(home.join(".cyphes").join("atp.sqlite3"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        atp::{agent_id, create_signed_envelope, create_signed_envelope_with_expiry},
        audit_profile::{contract_hash, AuditContract, RepositoryTarget},
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
            currency: "USDC".to_string(),
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
            currency: "USDC".to_string(),
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
}
