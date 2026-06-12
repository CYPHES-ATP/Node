use chrono::DateTime;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, fmt};

pub const AUDIT_CONTRACT_PROFILE: &str = "cyphes.repository-security-audit/0.1";
pub const AUDIT_RECEIPT_PROFILE: &str = "cyphes.repository-security-audit-receipt/0.1";
pub const AUDIT_PROFILE_VERSION: &str = "0.1";

const REQUIRED_DELIVERABLES: [&str; 5] = [
    "artifacts/audit-report.md",
    "artifacts/findings.json",
    "artifacts/results.sarif",
    "artifacts/checks.json",
    "artifacts/manifest.json",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryTarget {
    pub full_name: String,
    pub url: String,
    pub commit_sha: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuditDeliverable {
    pub path: String,
    pub media_type: String,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuditExecutionPolicy {
    pub repository_access: String,
    pub network_access: String,
    pub max_duration_seconds: u64,
    pub delete_checkout_after_receipt: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProposedCompensation {
    pub amount: String,
    pub asset: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuditSettlement {
    pub rail: String,
    pub amount: String,
    pub asset: String,
    pub condition: String,
    pub proof_of_payment: String,
    pub refund_policy: String,
    pub dispute_policy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuditProofPolicy {
    pub worker_signature: bool,
    pub requester_approval: bool,
    pub artifact_hashes: bool,
    pub event_chain: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuditContract {
    pub profile: String,
    pub profile_version: String,
    pub transaction_id: String,
    pub requester_agent_id: String,
    pub worker_agent_id: String,
    pub repository: RepositoryTarget,
    pub scope: Vec<String>,
    pub exclusions: Vec<String>,
    pub deliverables: Vec<AuditDeliverable>,
    pub execution: AuditExecutionPolicy,
    pub proposed_compensation: ProposedCompensation,
    pub settlement: AuditSettlement,
    pub proof_policy: AuditProofPolicy,
    pub expires_at: String,
}

impl AuditContract {
    pub fn repository_audit(
        transaction_id: String,
        requester_agent_id: String,
        worker_agent_id: String,
        repository: RepositoryTarget,
        scope: Vec<String>,
        proposed_amount: String,
        expires_at: String,
    ) -> Self {
        Self {
            profile: AUDIT_CONTRACT_PROFILE.to_string(),
            profile_version: AUDIT_PROFILE_VERSION.to_string(),
            transaction_id,
            requester_agent_id,
            worker_agent_id,
            repository,
            scope,
            exclusions: vec![
                "No write access to the repository".to_string(),
                "No access outside the pinned repository checkout".to_string(),
                "No credential discovery or secret use".to_string(),
                "No external state changes".to_string(),
            ],
            deliverables: vec![
                AuditDeliverable {
                    path: "artifacts/audit-report.md".to_string(),
                    media_type: "text/markdown".to_string(),
                    required: true,
                },
                AuditDeliverable {
                    path: "artifacts/findings.json".to_string(),
                    media_type: "application/json".to_string(),
                    required: true,
                },
                AuditDeliverable {
                    path: "artifacts/results.sarif".to_string(),
                    media_type: "application/sarif+json".to_string(),
                    required: true,
                },
                AuditDeliverable {
                    path: "artifacts/checks.json".to_string(),
                    media_type: "application/json".to_string(),
                    required: true,
                },
                AuditDeliverable {
                    path: "artifacts/manifest.json".to_string(),
                    media_type: "application/json".to_string(),
                    required: true,
                },
            ],
            execution: AuditExecutionPolicy {
                repository_access: "read-only-pinned-commit".to_string(),
                network_access: "deny-unless-contract-extension".to_string(),
                max_duration_seconds: 3_600,
                delete_checkout_after_receipt: true,
            },
            proposed_compensation: ProposedCompensation {
                amount: proposed_amount,
                asset: "USDC".to_string(),
                status: "non-payable-term".to_string(),
            },
            settlement: AuditSettlement {
                rail: "zero-value".to_string(),
                amount: "0".to_string(),
                asset: "none".to_string(),
                condition: "verified-receipt-and-requester-approval".to_string(),
                proof_of_payment: "waived".to_string(),
                refund_policy: "not-applicable".to_string(),
                dispute_policy: "manual".to_string(),
            },
            proof_policy: AuditProofPolicy {
                worker_signature: true,
                requester_approval: true,
                artifact_hashes: true,
                event_chain: true,
            },
            expires_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuditArtifact {
    pub path: String,
    pub media_type: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptRequested {
    pub contract_hash: String,
    pub repository: RepositoryTarget,
    pub scope: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptAccessed {
    pub leases: Vec<String>,
    pub resources: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptChanged {
    pub artifact_paths: Vec<String>,
    pub external_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptApproval {
    pub by: String,
    pub method: String,
    pub time: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptSignature {
    pub signer: String,
    pub kid: String,
    pub algorithm: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuditReceipt {
    pub receipt_type: String,
    pub atp: String,
    pub profile: String,
    pub profile_version: String,
    pub transaction_id: String,
    pub requested: ReceiptRequested,
    pub accessed: ReceiptAccessed,
    pub changed: ReceiptChanged,
    pub approved: ReceiptApproval,
    pub paid: AuditSettlement,
    pub artifacts: Vec<AuditArtifact>,
    pub event_root: String,
    pub receipt_hash: String,
    pub signatures: Vec<ReceiptSignature>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditProfileError {
    pub atp_code: &'static str,
    pub reason_code: &'static str,
    pub message: String,
}

impl AuditProfileError {
    fn bad_state(reason_code: &'static str, message: impl Into<String>) -> Self {
        Self {
            atp_code: "ATP_BAD_STATE",
            reason_code,
            message: message.into(),
        }
    }

    fn proof_unsatisfied(reason_code: &'static str, message: impl Into<String>) -> Self {
        Self {
            atp_code: "ATP_PROOF_UNSATISFIED",
            reason_code,
            message: message.into(),
        }
    }
}

impl fmt::Display for AuditProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}: {}: {}",
            self.atp_code, self.reason_code, self.message
        )
    }
}

pub fn contract_hash(contract: &AuditContract) -> Result<String, String> {
    canonical_hash(contract)
}

pub fn receipt_hash(receipt: &AuditReceipt) -> Result<String, String> {
    let mut value = serde_json::to_value(receipt).map_err(|error| error.to_string())?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| "ATP receipt must serialize as an object".to_string())?;
    object.remove("receiptHash");
    object.remove("signatures");
    canonical_hash(&value)
}

pub fn validate_contract(contract: &AuditContract) -> Result<(), AuditProfileError> {
    if contract.profile != AUDIT_CONTRACT_PROFILE
        || contract.profile_version != AUDIT_PROFILE_VERSION
    {
        return Err(AuditProfileError::bad_state(
            "AUDIT_CONTRACT_PROFILE_UNSUPPORTED",
            "unsupported repository-audit contract profile",
        ));
    }
    if contract.transaction_id.trim().is_empty()
        || contract.requester_agent_id.trim().is_empty()
        || contract.worker_agent_id.trim().is_empty()
    {
        return Err(AuditProfileError::bad_state(
            "AUDIT_CONTRACT_PARTY_MISSING",
            "transaction and party identifiers are required",
        ));
    }
    validate_repository(&contract.repository)?;
    if contract.scope.is_empty() || contract.scope.iter().any(|item| item.trim().is_empty()) {
        return Err(AuditProfileError::bad_state(
            "AUDIT_CONTRACT_SCOPE_INVALID",
            "the audit scope must contain non-empty entries",
        ));
    }

    let deliverable_paths = contract
        .deliverables
        .iter()
        .map(|deliverable| deliverable.path.as_str())
        .collect::<HashSet<_>>();
    let required_deliverables = contract
        .deliverables
        .iter()
        .filter(|deliverable| deliverable.required)
        .map(|deliverable| deliverable.path.as_str())
        .collect::<HashSet<_>>();
    if deliverable_paths.len() != contract.deliverables.len()
        || REQUIRED_DELIVERABLES
            .iter()
            .any(|required| !required_deliverables.contains(required))
    {
        return Err(AuditProfileError::bad_state(
            "AUDIT_CONTRACT_DELIVERABLE_MISSING",
            "the contract is missing a required audit artifact",
        ));
    }
    if contract.execution.repository_access != "read-only-pinned-commit"
        || contract.execution.max_duration_seconds == 0
        || !contract.execution.delete_checkout_after_receipt
    {
        return Err(AuditProfileError::bad_state(
            "AUDIT_CONTRACT_EXECUTION_UNBOUNDED",
            "execution must be time-bounded, read-only, and delete its checkout",
        ));
    }
    if contract.proposed_compensation.status != "non-payable-term" {
        return Err(AuditProfileError::bad_state(
            "AUDIT_CONTRACT_PAYMENT_MISREPRESENTED",
            "proposed compensation must remain explicitly non-payable",
        ));
    }
    if contract.proposed_compensation.asset != "USDC"
        || contract
            .proposed_compensation
            .amount
            .parse::<f64>()
            .map_or(true, |amount| amount <= 0.0)
    {
        return Err(AuditProfileError::bad_state(
            "AUDIT_CONTRACT_PROPOSED_COMPENSATION_INVALID",
            "proposed compensation must be a positive USDC term",
        ));
    }
    validate_zero_value_settlement(&contract.settlement)?;
    if !contract.proof_policy.worker_signature
        || !contract.proof_policy.requester_approval
        || !contract.proof_policy.artifact_hashes
        || !contract.proof_policy.event_chain
    {
        return Err(AuditProfileError::bad_state(
            "AUDIT_CONTRACT_PROOF_POLICY_WEAK",
            "the repository-audit profile requires signatures, approval, artifact hashes, and event-chain verification",
        ));
    }
    DateTime::parse_from_rfc3339(&contract.expires_at).map_err(|_| {
        AuditProfileError::bad_state(
            "AUDIT_CONTRACT_EXPIRY_INVALID",
            "contract expiry must be RFC3339",
        )
    })?;
    Ok(())
}

pub fn validate_receipt(receipt: &AuditReceipt) -> Result<(), AuditProfileError> {
    if receipt.receipt_type != "ProofOfCognition"
        || receipt.atp != "0.3"
        || receipt.profile != AUDIT_RECEIPT_PROFILE
        || receipt.profile_version != AUDIT_PROFILE_VERSION
    {
        return Err(AuditProfileError::proof_unsatisfied(
            "AUDIT_RECEIPT_PROFILE_UNSUPPORTED",
            "unsupported repository-audit receipt profile",
        ));
    }
    if receipt.transaction_id.trim().is_empty()
        || !is_sha256_ref(&receipt.requested.contract_hash)
        || !is_sha256_ref(&receipt.event_root)
        || receipt.requested.scope.is_empty()
        || receipt.accessed.leases.is_empty()
        || receipt.accessed.resources.is_empty()
        || receipt.changed.external_state != "none"
    {
        return Err(AuditProfileError::proof_unsatisfied(
            "AUDIT_RECEIPT_BINDING_INVALID",
            "receipt must bind its transaction, contract, scope, leases, resources, and event root without external state changes",
        ));
    }
    validate_repository(&receipt.requested.repository)?;
    validate_zero_value_settlement(&receipt.paid)?;
    if DateTime::parse_from_rfc3339(&receipt.approved.time).is_err() {
        return Err(AuditProfileError::proof_unsatisfied(
            "AUDIT_RECEIPT_APPROVAL_INVALID",
            "approval time must be RFC3339",
        ));
    }
    let signers = receipt
        .signatures
        .iter()
        .map(|signature| signature.signer.as_str())
        .collect::<HashSet<_>>();
    if receipt.signatures.len() < 2
        || signers.len() != receipt.signatures.len()
        || !signers.contains(receipt.approved.by.as_str())
        || receipt.signatures.iter().any(|signature| {
            signature.kid.trim().is_empty()
                || signature.algorithm != "EdDSA"
                || signature.signature.trim().is_empty()
        })
    {
        return Err(AuditProfileError::proof_unsatisfied(
            "AUDIT_RECEIPT_SIGNATURE_SET_INVALID",
            "receipt must carry distinct worker and requester EdDSA signature records",
        ));
    }

    let changed_paths = receipt
        .changed
        .artifact_paths
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let mut artifact_paths = HashSet::new();
    for artifact in &receipt.artifacts {
        if !artifact_paths.insert(artifact.path.as_str())
            || !is_sha256_ref(&artifact.sha256)
            || artifact.size == 0
            || artifact.media_type.trim().is_empty()
        {
            return Err(AuditProfileError::proof_unsatisfied(
                "AUDIT_RECEIPT_ARTIFACT_INVALID",
                "artifacts must have unique paths, non-zero sizes, and SHA-256 hashes",
            ));
        }
        if !changed_paths.contains(artifact.path.as_str()) {
            return Err(AuditProfileError::proof_unsatisfied(
                "AUDIT_RECEIPT_ARTIFACT_UNBOUND",
                "every artifact must be named in the changed set",
            ));
        }
    }
    if REQUIRED_DELIVERABLES
        .iter()
        .any(|required| !artifact_paths.contains(required))
    {
        return Err(AuditProfileError::proof_unsatisfied(
            "AUDIT_RECEIPT_ARTIFACT_MISSING",
            "receipt is missing a required audit artifact",
        ));
    }

    let expected_hash = receipt_hash(receipt).map_err(|error| {
        AuditProfileError::proof_unsatisfied("AUDIT_RECEIPT_CANONICALIZATION_FAILED", error)
    })?;
    if receipt.receipt_hash != expected_hash {
        return Err(AuditProfileError::proof_unsatisfied(
            "AUDIT_RECEIPT_HASH_MISMATCH",
            "receiptHash does not match the canonical receipt body",
        ));
    }
    Ok(())
}

pub fn is_git_commit_sha(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validate_repository(repository: &RepositoryTarget) -> Result<(), AuditProfileError> {
    if repository.full_name.split('/').count() != 2
        || repository.url != format!("https://github.com/{}", repository.full_name)
        || !is_git_commit_sha(&repository.commit_sha)
    {
        return Err(AuditProfileError::bad_state(
            "AUDIT_CONTRACT_REPOSITORY_UNPINNED",
            "repository must be a canonical GitHub URL pinned to a commit SHA",
        ));
    }
    Ok(())
}

fn validate_zero_value_settlement(settlement: &AuditSettlement) -> Result<(), AuditProfileError> {
    if settlement.rail != "zero-value"
        || settlement.amount != "0"
        || settlement.asset != "none"
        || settlement.condition != "verified-receipt-and-requester-approval"
        || settlement.proof_of_payment != "waived"
        || settlement.refund_policy != "not-applicable"
        || settlement.dispute_policy != "manual"
    {
        return Err(AuditProfileError::bad_state(
            "AUDIT_CONTRACT_SETTLEMENT_UNSUPPORTED",
            "this profile supports only an explicit zero-value settlement",
        ));
    }
    Ok(())
}

fn is_sha256_ref(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}

fn canonical_hash<T: Serialize>(value: &T) -> Result<String, String> {
    let bytes = serde_jcs::to_vec(value).map_err(|error| error.to_string())?;
    let digest = Sha256::digest(bytes);
    Ok(format!(
        "sha256:{}",
        digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_fixture_is_valid_and_canonical() {
        let contract: AuditContract = serde_json::from_str(include_str!(
            "../../protocol/fixtures/repository-audit-contract.v0.1.json"
        ))
        .unwrap();
        validate_contract(&contract).unwrap();
        println!("contract_hash={}", contract_hash(&contract).unwrap());
    }

    #[test]
    fn receipt_fixture_is_valid_and_canonical() {
        let receipt: AuditReceipt = serde_json::from_str(include_str!(
            "../../protocol/fixtures/repository-audit-receipt.v0.1.json"
        ))
        .unwrap();
        println!("receipt_hash={}", receipt_hash(&receipt).unwrap());
        validate_receipt(&receipt).unwrap();
    }

    #[test]
    fn contract_rejects_a_moving_branch() {
        let mut contract: AuditContract = serde_json::from_str(include_str!(
            "../../protocol/fixtures/repository-audit-contract.v0.1.json"
        ))
        .unwrap();
        contract.repository.commit_sha = "main".to_string();
        let error = validate_contract(&contract).unwrap_err();
        assert_eq!(error.reason_code, "AUDIT_CONTRACT_REPOSITORY_UNPINNED");
    }

    #[test]
    fn receipt_rejects_artifact_substitution() {
        let mut receipt: AuditReceipt = serde_json::from_str(include_str!(
            "../../protocol/fixtures/repository-audit-receipt.v0.1.json"
        ))
        .unwrap();
        receipt.artifacts[0].sha256 =
            "sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff".to_string();
        let error = validate_receipt(&receipt).unwrap_err();
        assert_eq!(error.reason_code, "AUDIT_RECEIPT_HASH_MISMATCH");
    }

    #[test]
    fn receipt_requires_worker_and_requester_signature_records() {
        let mut receipt: AuditReceipt = serde_json::from_str(include_str!(
            "../../protocol/fixtures/repository-audit-receipt.v0.1.json"
        ))
        .unwrap();
        receipt.signatures.clear();
        let error = validate_receipt(&receipt).unwrap_err();
        assert_eq!(error.reason_code, "AUDIT_RECEIPT_SIGNATURE_SET_INVALID");
    }

    #[test]
    fn expiry_parser_accepts_utc_contract_time() {
        let contract: AuditContract = serde_json::from_str(include_str!(
            "../../protocol/fixtures/repository-audit-contract.v0.1.json"
        ))
        .unwrap();
        let parsed = DateTime::parse_from_rfc3339(&contract.expires_at).unwrap();
        assert!(parsed.with_timezone(&chrono::Utc).timestamp() > 0);
    }
}
