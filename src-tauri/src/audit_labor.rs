use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::Utc;
use libp2p::identity;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    atp::{
        agent_id, now_rfc3339, public_key_from_raw_ed25519, raw_ed25519_public_key, sign_canonical,
        verify_canonical,
    },
    audit_profile::{is_git_commit_sha, RepositoryTarget},
};

pub const CAMPAIGN_PROFILE: &str = "cyphes.protocol-audit-campaign/0.1";
pub const WORK_UNIT_PROFILE: &str = "cyphes.audit-work-unit/0.1";
pub const WORK_UNIT_CLAIM_PROFILE: &str = "cyphes.audit-work-unit-claim/0.1";
pub const CONTRIBUTION_PROFILE: &str = "cyphes.audit-contribution/0.1";
pub const VERIFICATION_PROFILE: &str = "cyphes.verification-result/0.1";
pub const CREDIT_PROFILE: &str = "cyphes.credit-ledger/0.1";
pub const FINAL_REPORT_PROFILE: &str = "cyphes.final-audit-report/0.1";
pub const AUDIT_LABOR_PROFILE_VERSION: &str = "0.1";
pub const DEFAULT_SKILL_PACK_ID: &str = "cyphes-audit-skill";
pub const DEFAULT_SKILL_PACK_VERSION: &str = "0.4";
pub const DEFAULT_SKILL_PACK_LABEL: &str = "CYPHES audit methodology v0.4";

const DEFAULT_AUDIT_SKILL_TEXT: &str =
    include_str!("../../protocol/skills/cyphes-audit-skill.v0.4.md");
const PARSER_FALLBACK_CREDIT_MULTIPLIER: f64 = 0.10;
const STANDARD_OUTPUT_MODEL_MULTIPLIER_CAP: f64 = 1.0;
const EXCELLENT_OUTPUT_COVERAGE_THRESHOLD: u32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillPackReference {
    pub skill_pack_id: String,
    pub version: String,
    pub hash: String,
    pub label: String,
}

impl Default for SkillPackReference {
    fn default() -> Self {
        default_skill_pack_reference()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CampaignAttachment {
    pub attachment_id: String,
    pub label: String,
    pub media_type: String,
    pub sha256: String,
    pub size_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

impl CampaignAttachment {
    pub fn from_text(label: String, text: String) -> Result<Self, String> {
        if label.trim().is_empty() || text.trim().is_empty() {
            return Err("campaign attachment requires a label and text".to_string());
        }
        Ok(Self {
            attachment_id: format!("attachment_{}", Uuid::new_v4().simple()),
            label,
            media_type: "text/markdown".to_string(),
            sha256: sha256_ref(text.as_bytes()),
            size_bytes: text.len() as u64,
            text: Some(text),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolAuditCampaign {
    pub profile: String,
    pub profile_version: String,
    pub campaign_id: String,
    pub protocol_name: String,
    pub repository: RepositoryTarget,
    pub scope_text: String,
    pub bounty_url: Option<String>,
    pub impacts_in_scope: Vec<String>,
    pub out_of_scope: Vec<String>,
    pub audit_brief_hash: Option<String>,
    pub audit_brief_text: Option<String>,
    #[serde(default)]
    pub skill_pack: SkillPackReference,
    #[serde(default)]
    pub attachments: Vec<CampaignAttachment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_skill_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_skill_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_skill_text: Option<String>,
    pub requester_agent_id: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

impl ProtocolAuditCampaign {
    pub fn new(
        protocol_name: String,
        repository: RepositoryTarget,
        scope_text: String,
        bounty_url: Option<String>,
        impacts_in_scope: Vec<String>,
        out_of_scope: Vec<String>,
        audit_brief_text: Option<String>,
        skill_pack: Option<SkillPackReference>,
        attachments: Vec<CampaignAttachment>,
        custom_skill_text: Option<String>,
        requester_agent_id: String,
    ) -> Result<Self, String> {
        validate_repository_target(&repository)?;
        if protocol_name.trim().is_empty() || scope_text.trim().is_empty() {
            return Err("protocol name and scope are required".to_string());
        }
        for attachment in &attachments {
            validate_campaign_attachment(attachment)?;
        }
        let custom_skill_hash = custom_skill_text
            .as_ref()
            .filter(|text| !text.trim().is_empty())
            .map(|text| sha256_ref(text.as_bytes()));
        let custom_skill_text = custom_skill_text.filter(|text| !text.trim().is_empty());
        let now = now_rfc3339();
        let campaign_id = format!(
            "campaign_{}_{}",
            Utc::now().timestamp_millis(),
            Uuid::new_v4().simple()
        );
        let audit_brief_hash = audit_brief_text
            .as_ref()
            .map(|text| sha256_ref(text.as_bytes()));
        Ok(Self {
            profile: CAMPAIGN_PROFILE.to_string(),
            profile_version: AUDIT_LABOR_PROFILE_VERSION.to_string(),
            campaign_id,
            protocol_name,
            repository,
            scope_text,
            bounty_url: bounty_url.filter(|value| !value.trim().is_empty()),
            impacts_in_scope,
            out_of_scope,
            audit_brief_hash,
            audit_brief_text,
            skill_pack: skill_pack.unwrap_or_default(),
            attachments,
            custom_skill_hash,
            custom_skill_label: custom_skill_text
                .as_ref()
                .map(|_| "Requester custom SKILL.md overlay".to_string()),
            custom_skill_text,
            requester_agent_id,
            status: "open".to_string(),
            created_at: now.clone(),
            updated_at: now,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuditWorkUnit {
    pub profile: String,
    pub profile_version: String,
    pub work_unit_id: String,
    pub campaign_id: String,
    pub kind: String,
    pub title: String,
    pub instructions: String,
    pub expected_artifacts: Vec<String>,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claimed_by_agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claimed_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuditWorkUnitClaim {
    pub profile: String,
    pub profile_version: String,
    pub claim_id: String,
    pub campaign_id: String,
    pub work_unit_id: String,
    pub requester_agent_id: String,
    pub worker_agent_id: String,
    pub status: String,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    pub public_key_base64_url: String,
    pub claim_hash: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeDescriptor {
    pub operator: String,
    pub adapter: String,
    pub model: String,
    pub model_multiplier: f64,
    pub tool_policy: Vec<String>,
    pub connected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint_class: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_per_second: Option<f64>,
}

impl RuntimeDescriptor {
    pub fn deterministic_fixture() -> Self {
        Self {
            operator: "CYPHES local runtime operator".to_string(),
            adapter: "cyphes-deterministic-fixture".to_string(),
            model: "none".to_string(),
            model_multiplier: 0.2,
            tool_policy: vec![
                "read-only pinned GitHub commit".to_string(),
                "no repository writes".to_string(),
                "no untrusted code execution".to_string(),
            ],
            connected: true,
            endpoint_class: None,
            skill_hash: None,
            input_hash: None,
            output_hash: None,
            tokens_per_second: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ContributionArtifact {
    pub path: String,
    pub media_type: String,
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuditFinding {
    pub id: String,
    pub title: String,
    pub severity: String,
    pub status: String,
    pub impact: Option<String>,
    pub evidence: Vec<String>,
    pub reportable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CoverageItem {
    pub area: String,
    pub status: String,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NodeContribution {
    pub profile: String,
    pub profile_version: String,
    pub contribution_id: String,
    pub campaign_id: String,
    pub work_unit_id: String,
    pub worker_agent_id: String,
    pub runtime: RuntimeDescriptor,
    pub notes_markdown: String,
    pub findings: Vec<AuditFinding>,
    pub artifacts: Vec<ContributionArtifact>,
    pub coverage: Vec<CoverageItem>,
    pub commands: Vec<String>,
    pub created_at: String,
    pub public_key_base64_url: String,
    pub contribution_hash: String,
    pub receipt_hash: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VerificationEvidence {
    pub label: String,
    pub reference: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VerificationResult {
    pub profile: String,
    pub profile_version: String,
    pub verification_id: String,
    pub campaign_id: String,
    pub target_contribution_id: String,
    pub verifier_agent_id: String,
    pub decision: String,
    pub reason_code: String,
    pub reason: String,
    pub reproduction_evidence: Vec<VerificationEvidence>,
    pub artifacts: Vec<ContributionArtifact>,
    pub created_at: String,
    pub public_key_base64_url: String,
    pub verification_hash: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreditBuckets {
    pub participation: u32,
    pub verification: u32,
    pub coverage: u32,
    pub finding: u32,
    pub bonus_allocation_placeholder: u32,
}

impl CreditBuckets {
    pub fn total(&self) -> u32 {
        self.participation
            + self.verification
            + self.coverage
            + self.finding
            + self.bonus_allocation_placeholder
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreditAllocation {
    pub profile: String,
    pub profile_version: String,
    pub allocation_id: String,
    pub campaign_id: String,
    pub contribution_id: String,
    pub verification_id: String,
    pub receiver_agent_id: String,
    pub contribution_receipt_hash: String,
    pub buckets: CreditBuckets,
    pub total: u32,
    pub formula: String,
    pub issued_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CreditSummary {
    pub total: u32,
    pub allocations: Vec<CreditAllocation>,
    #[serde(default)]
    pub provisional_total: u32,
    #[serde(default)]
    pub provisional_allocations: Vec<CreditAllocation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CampaignReportSnapshot {
    pub campaign: ProtocolAuditCampaign,
    pub work_units: Vec<AuditWorkUnit>,
    #[serde(default)]
    pub claims: Vec<AuditWorkUnitClaim>,
    pub contributions: Vec<NodeContribution>,
    pub verifications: Vec<VerificationResult>,
    pub credits: Vec<CreditAllocation>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreditScoreInput {
    pub base_points: f64,
    pub difficulty_multiplier: f64,
    pub verification_multiplier: f64,
    pub model_multiplier: f64,
    pub requester_approval: f64,
    pub penalty_points: f64,
}

pub fn default_work_units(campaign: &ProtocolAuditCampaign) -> Vec<AuditWorkUnit> {
    let templates = [
        (
            "scope-mapping",
            "Scope mapping",
            "Map repository purpose, pinned commit, in-scope assets, bounty rules, out-of-scope clauses, known issues, and residual uncertainty before judging.",
            vec!["notes.md", "scope-map.json"],
        ),
        (
            "repo-inventory",
            "Repository inventory",
            "Inventory key directories, manifests, lockfiles, CI, docs, entry points, and core control/data flow at the pinned commit.",
            vec!["repo-map.md", "inventory.json"],
        ),
        (
            "dependency-config-review",
            "Dependency and config review",
            "Review dependency, build, CI, permission, secret, and configuration posture without executing untrusted repository code.",
            vec!["findings.json", "coverage.json"],
        ),
        (
            "defi-exploit-class-pass",
            "DeFi exploit-class pass",
            "For smart-contract targets, record applicability, files/functions reviewed, tests or traces attempted, finding status, and residual uncertainty for reentrancy, flash loans, price manipulation, mocks, oracle mocks, and MEV.",
            vec!["defi-matrix.json", "notes.md"],
        ),
        (
            "finding-validation",
            "Finding validation",
            "Validate candidate findings against program impact, scope, novelty, permission assumptions, and deterministic reproduction requirements.",
            vec!["findings.json", "validation-notes.md"],
        ),
        (
            "peer-verification",
            "Peer verification",
            "Accept, reject, reproduce, challenge, or request revision for another node's signed contribution with evidence and reason codes.",
            vec!["verification.json", "reproduction-notes.md"],
        ),
        (
            "final-report-section",
            "Final report section",
            "Prepare protocol-facing report sections from accepted work only, with rejected or duplicate leads moved into an appendix.",
            vec!["report-section.md", "appendix.json"],
        ),
    ];

    templates
        .iter()
        .enumerate()
        .map(
            |(index, (kind, title, instructions, artifacts))| AuditWorkUnit {
                profile: WORK_UNIT_PROFILE.to_string(),
                profile_version: AUDIT_LABOR_PROFILE_VERSION.to_string(),
                work_unit_id: format!("{}-{:02}-{}", campaign.campaign_id, index + 1, kind),
                campaign_id: campaign.campaign_id.clone(),
                kind: (*kind).to_string(),
                title: (*title).to_string(),
                instructions: (*instructions).to_string(),
                expected_artifacts: artifacts
                    .iter()
                    .map(|artifact| (*artifact).to_string())
                    .collect(),
                status: "open".to_string(),
                claimed_by_agent_id: None,
                claim_id: None,
                claimed_at: None,
                created_at: now_rfc3339(),
            },
        )
        .collect()
}

pub fn signed_work_unit_claim(
    keypair: &identity::Keypair,
    campaign: &ProtocolAuditCampaign,
    work_unit: &AuditWorkUnit,
) -> Result<AuditWorkUnitClaim, String> {
    validate_campaign(campaign)?;
    validate_work_unit(work_unit)?;
    if work_unit.campaign_id != campaign.campaign_id {
        return Err("work unit does not belong to campaign".to_string());
    }
    let worker_agent_id = agent_id(&keypair.public());
    let public_key = raw_ed25519_public_key(&keypair.public())?;
    let mut claim = AuditWorkUnitClaim {
        profile: WORK_UNIT_CLAIM_PROFILE.to_string(),
        profile_version: AUDIT_LABOR_PROFILE_VERSION.to_string(),
        claim_id: format!("claim_{}", Uuid::new_v4().simple()),
        campaign_id: campaign.campaign_id.clone(),
        work_unit_id: work_unit.work_unit_id.clone(),
        requester_agent_id: campaign.requester_agent_id.clone(),
        worker_agent_id,
        status: "claimed".to_string(),
        created_at: now_rfc3339(),
        expires_at: None,
        public_key_base64_url: URL_SAFE_NO_PAD.encode(public_key),
        claim_hash: String::new(),
        signature: String::new(),
    };
    claim.claim_hash = canonical_hash(&claim_signature_value(&claim)?)?;
    claim.signature = sign_canonical(keypair, &claim_signature_value(&claim)?)?;
    Ok(claim)
}

pub fn signed_contribution(
    keypair: &identity::Keypair,
    campaign_id: String,
    work_unit_id: String,
    runtime: RuntimeDescriptor,
    notes_markdown: String,
    findings: Vec<AuditFinding>,
    artifacts: Vec<ContributionArtifact>,
    coverage: Vec<CoverageItem>,
    commands: Vec<String>,
) -> Result<NodeContribution, String> {
    if notes_markdown.trim().is_empty() || artifacts.is_empty() || coverage.is_empty() {
        return Err("contribution requires notes, artifacts, and coverage evidence".to_string());
    }
    let public_key = raw_ed25519_public_key(&keypair.public())?;
    let mut contribution = NodeContribution {
        profile: CONTRIBUTION_PROFILE.to_string(),
        profile_version: AUDIT_LABOR_PROFILE_VERSION.to_string(),
        contribution_id: format!("contribution_{}", Uuid::new_v4().simple()),
        campaign_id,
        work_unit_id,
        worker_agent_id: agent_id(&keypair.public()),
        runtime,
        notes_markdown,
        findings,
        artifacts,
        coverage,
        commands,
        created_at: now_rfc3339(),
        public_key_base64_url: URL_SAFE_NO_PAD.encode(public_key),
        contribution_hash: String::new(),
        receipt_hash: String::new(),
        signature: String::new(),
    };
    contribution.contribution_hash = canonical_hash(&contribution_signature_value(&contribution)?)?;
    contribution.receipt_hash = contribution_receipt_hash(&contribution)?;
    contribution.signature =
        sign_canonical(keypair, &contribution_signature_value(&contribution)?)?;
    Ok(contribution)
}

pub fn signed_verification(
    keypair: &identity::Keypair,
    campaign_id: String,
    target_contribution_id: String,
    decision: String,
    reason_code: String,
    reason: String,
    reproduction_evidence: Vec<VerificationEvidence>,
    artifacts: Vec<ContributionArtifact>,
) -> Result<VerificationResult, String> {
    validate_verification_decision(&decision)?;
    if reason_code.trim().is_empty() || reason.trim().is_empty() {
        return Err("verification requires a reason code and reason".to_string());
    }
    let public_key = raw_ed25519_public_key(&keypair.public())?;
    let mut verification = VerificationResult {
        profile: VERIFICATION_PROFILE.to_string(),
        profile_version: AUDIT_LABOR_PROFILE_VERSION.to_string(),
        verification_id: format!("verification_{}", Uuid::new_v4().simple()),
        campaign_id,
        target_contribution_id,
        verifier_agent_id: agent_id(&keypair.public()),
        decision,
        reason_code,
        reason,
        reproduction_evidence,
        artifacts,
        created_at: now_rfc3339(),
        public_key_base64_url: URL_SAFE_NO_PAD.encode(public_key),
        verification_hash: String::new(),
        signature: String::new(),
    };
    verification.verification_hash = canonical_hash(&verification_signature_value(&verification)?)?;
    verification.signature =
        sign_canonical(keypair, &verification_signature_value(&verification)?)?;
    Ok(verification)
}

pub fn verify_signed_contribution(contribution: &NodeContribution) -> Result<(), String> {
    validate_contribution(contribution)?;
    let public_bytes = URL_SAFE_NO_PAD
        .decode(&contribution.public_key_base64_url)
        .map_err(|_| "contribution public key is not valid base64url".to_string())?;
    let public_key = public_key_from_raw_ed25519(&public_bytes)?;
    if agent_id(&public_key) != contribution.worker_agent_id {
        return Err("contribution key does not match worker identity".to_string());
    }
    if contribution.contribution_hash
        != canonical_hash(&contribution_signature_value(contribution)?)?
    {
        return Err("contribution hash mismatch".to_string());
    }
    if contribution.receipt_hash != contribution_receipt_hash(contribution)? {
        return Err("contribution receipt hash mismatch".to_string());
    }
    verify_canonical(
        &public_key,
        &contribution_signature_value(contribution)?,
        &contribution.signature,
    )
}

pub fn verify_signed_verification(verification: &VerificationResult) -> Result<(), String> {
    validate_verification(verification)?;
    let public_bytes = URL_SAFE_NO_PAD
        .decode(&verification.public_key_base64_url)
        .map_err(|_| "verification public key is not valid base64url".to_string())?;
    let public_key = public_key_from_raw_ed25519(&public_bytes)?;
    if agent_id(&public_key) != verification.verifier_agent_id {
        return Err("verification key does not match verifier identity".to_string());
    }
    if verification.verification_hash
        != canonical_hash(&verification_signature_value(verification)?)?
    {
        return Err("verification hash mismatch".to_string());
    }
    verify_canonical(
        &public_key,
        &verification_signature_value(verification)?,
        &verification.signature,
    )
}

pub fn verify_signed_work_unit_claim(claim: &AuditWorkUnitClaim) -> Result<(), String> {
    validate_work_unit_claim(claim)?;
    let public_bytes = URL_SAFE_NO_PAD
        .decode(&claim.public_key_base64_url)
        .map_err(|_| "claim public key is not valid base64url".to_string())?;
    let public_key = public_key_from_raw_ed25519(&public_bytes)?;
    if agent_id(&public_key) != claim.worker_agent_id {
        return Err("claim key does not match worker identity".to_string());
    }
    if claim.claim_hash != canonical_hash(&claim_signature_value(claim)?)? {
        return Err("claim hash mismatch".to_string());
    }
    verify_canonical(
        &public_key,
        &claim_signature_value(claim)?,
        &claim.signature,
    )
}

pub fn credit_score(input: CreditScoreInput) -> Result<u32, String> {
    for value in [
        input.base_points,
        input.difficulty_multiplier,
        input.verification_multiplier,
        input.model_multiplier,
        input.requester_approval,
        input.penalty_points,
    ] {
        if !value.is_finite() || value < 0.0 {
            return Err("credit score inputs must be finite non-negative values".to_string());
        }
    }
    let raw = input.base_points
        * input.difficulty_multiplier
        * input.verification_multiplier
        * input.model_multiplier
        * input.requester_approval
        - input.penalty_points;
    Ok(raw.max(0.0).round() as u32)
}

pub fn allocate_credits(
    contribution: &NodeContribution,
    verification: &VerificationResult,
) -> Result<Vec<CreditAllocation>, String> {
    allocate_credits_with_policy(contribution, verification, true)
}

pub fn allocate_provisional_credits(
    contribution: &NodeContribution,
    verification: &VerificationResult,
) -> Result<Vec<CreditAllocation>, String> {
    allocate_credits_with_policy(contribution, verification, false)
}

fn allocate_credits_with_policy(
    contribution: &NodeContribution,
    verification: &VerificationResult,
    require_independent_verifier: bool,
) -> Result<Vec<CreditAllocation>, String> {
    verify_signed_contribution(contribution)?;
    verify_signed_verification(verification)?;
    if verification.target_contribution_id != contribution.contribution_id
        || verification.campaign_id != contribution.campaign_id
    {
        return Err("verification does not target the contribution".to_string());
    }
    if verification.decision != "accepted" {
        return Err("credits require an accepted verification result".to_string());
    }
    if require_independent_verifier
        && verification.verifier_agent_id == contribution.worker_agent_id
    {
        return Err("credits require an independent verifier".to_string());
    }
    if contribution.receipt_hash.trim().is_empty() || !is_sha256_ref(&contribution.receipt_hash) {
        return Err("credits require a signed contribution receipt hash".to_string());
    }

    let reportable_findings = contribution
        .findings
        .iter()
        .filter(|finding| finding.reportable && finding.status == "candidate")
        .count() as u32;
    let high_quality_coverage = contribution
        .coverage
        .iter()
        .filter(|coverage| !coverage.evidence.is_empty())
        .count() as u32;
    let quality_multiplier = contribution_quality_multiplier(contribution);
    let model_multiplier = effective_model_multiplier(
        contribution,
        quality_multiplier,
        reportable_findings,
        high_quality_coverage,
    );
    let participation = credit_score(CreditScoreInput {
        base_points: 100.0,
        difficulty_multiplier: work_unit_difficulty(&contribution.work_unit_id),
        verification_multiplier: 1.0,
        model_multiplier,
        requester_approval: 1.0,
        penalty_points: 0.0,
    })?;
    let coverage =
        scaled_credit_bucket(high_quality_coverage.saturating_mul(15), quality_multiplier);
    let finding = scaled_credit_bucket(reportable_findings.saturating_mul(75), quality_multiplier);
    let formula = if is_parser_fallback_contribution(contribution) {
        "base * difficulty * verification * model * requesterApproval * quality(0.10 parser fallback) - penalties"
    } else if qualifies_for_large_model_bonus(reportable_findings, high_quality_coverage) {
        "base * difficulty * verification * model(excellent-output bonus) * requesterApproval - penalties"
    } else {
        "base * difficulty * verification * min(model, 1.0 standard-output cap) * requesterApproval - penalties"
    };
    let mut allocations = vec![CreditAllocation {
        profile: CREDIT_PROFILE.to_string(),
        profile_version: AUDIT_LABOR_PROFILE_VERSION.to_string(),
        allocation_id: format!("credit_{}", Uuid::new_v4().simple()),
        campaign_id: contribution.campaign_id.clone(),
        contribution_id: contribution.contribution_id.clone(),
        verification_id: verification.verification_id.clone(),
        receiver_agent_id: contribution.worker_agent_id.clone(),
        contribution_receipt_hash: contribution.receipt_hash.clone(),
        buckets: CreditBuckets {
            participation,
            verification: 0,
            coverage,
            finding,
            bonus_allocation_placeholder: if reportable_findings > 0 { 1 } else { 0 },
        },
        total: 0,
        formula: formula.to_string(),
        issued_at: now_rfc3339(),
    }];
    allocations[0].total = allocations[0].buckets.total();

    if verification.verifier_agent_id != contribution.worker_agent_id {
        let verification_credit = scaled_credit_bucket(40, quality_multiplier);
        let mut verifier_allocation = CreditAllocation {
            profile: CREDIT_PROFILE.to_string(),
            profile_version: AUDIT_LABOR_PROFILE_VERSION.to_string(),
            allocation_id: format!("credit_{}", Uuid::new_v4().simple()),
            campaign_id: contribution.campaign_id.clone(),
            contribution_id: contribution.contribution_id.clone(),
            verification_id: verification.verification_id.clone(),
            receiver_agent_id: verification.verifier_agent_id.clone(),
            contribution_receipt_hash: contribution.receipt_hash.clone(),
            buckets: CreditBuckets {
                participation: 0,
                verification: verification_credit,
                coverage: 0,
                finding: 0,
                bonus_allocation_placeholder: 0,
            },
            total: 0,
            formula: if is_parser_fallback_contribution(contribution) {
                "accepted peer verification credit * quality(0.10 parser fallback)".to_string()
            } else {
                "accepted peer verification credit".to_string()
            },
            issued_at: now_rfc3339(),
        };
        verifier_allocation.total = verifier_allocation.buckets.total();
        allocations.push(verifier_allocation);
    }
    Ok(allocations)
}

fn effective_model_multiplier(
    contribution: &NodeContribution,
    quality_multiplier: f64,
    reportable_findings: u32,
    high_quality_coverage: u32,
) -> f64 {
    let model_multiplier = contribution.runtime.model_multiplier;
    if is_parser_fallback_contribution(contribution) {
        model_multiplier * quality_multiplier
    } else if qualifies_for_large_model_bonus(reportable_findings, high_quality_coverage) {
        model_multiplier
    } else {
        model_multiplier.min(STANDARD_OUTPUT_MODEL_MULTIPLIER_CAP)
    }
}

fn qualifies_for_large_model_bonus(reportable_findings: u32, high_quality_coverage: u32) -> bool {
    reportable_findings > 0 || high_quality_coverage >= EXCELLENT_OUTPUT_COVERAGE_THRESHOLD
}

fn contribution_quality_multiplier(contribution: &NodeContribution) -> f64 {
    if is_parser_fallback_contribution(contribution) {
        PARSER_FALLBACK_CREDIT_MULTIPLIER
    } else {
        1.0
    }
}

fn is_parser_fallback_contribution(contribution: &NodeContribution) -> bool {
    contribution.findings.is_empty()
        && (contribution
            .notes_markdown
            .contains("CYPHES parser note: model output was not valid structured JSON")
            || contribution.commands.iter().any(|command| {
                command
                    .to_ascii_lowercase()
                    .contains("structured parse failed")
            })
            || contribution.coverage.iter().any(|coverage| {
                coverage.area.eq_ignore_ascii_case("local model output")
                    && coverage.status.eq_ignore_ascii_case("needs_review")
            }))
}

fn scaled_credit_bucket(points: u32, multiplier: f64) -> u32 {
    ((points as f64) * multiplier).max(0.0).round() as u32
}

pub fn final_report_markdown(snapshot: &CampaignReportSnapshot) -> String {
    let accepted_ids = snapshot
        .verifications
        .iter()
        .filter(|verification| verification.decision == "accepted")
        .map(|verification| verification.target_contribution_id.as_str())
        .collect::<std::collections::HashSet<_>>();
    let rejected_ids = snapshot
        .verifications
        .iter()
        .filter(|verification| verification.decision != "accepted")
        .map(|verification| verification.target_contribution_id.as_str())
        .collect::<std::collections::HashSet<_>>();
    let accepted_contributions = snapshot
        .contributions
        .iter()
        .filter(|contribution| accepted_ids.contains(contribution.contribution_id.as_str()))
        .collect::<Vec<_>>();
    let rejected_contributions = snapshot
        .contributions
        .iter()
        .filter(|contribution| rejected_ids.contains(contribution.contribution_id.as_str()))
        .collect::<Vec<_>>();

    let accepted_findings = accepted_contributions
        .iter()
        .flat_map(|contribution| {
            contribution
                .findings
                .iter()
                .filter(|finding| finding.reportable)
                .map(move |finding| (*contribution, finding))
        })
        .collect::<Vec<_>>();
    let total_credits = snapshot
        .credits
        .iter()
        .map(|credit| credit.total)
        .sum::<u32>();
    let accepted_count = accepted_contributions.len();
    let verification_count = snapshot.verifications.len();

    let mut report = format!(
        "# CYPHES Protocol Audit Report\n\n\
         ## Document Control\n\n\
         | Field | Value |\n\
         | --- | --- |\n\
         | Protocol | {} |\n\
         | Repository | `{}` |\n\
         | Pinned commit | `{}` |\n\
         | Campaign | `{}` |\n\
         | Skill pack | `{}` `{}` |\n\
         | Skill pack hash | `{}` |\n\
         | Custom SKILL overlay | `{}` |\n\
         | Profile | `cyphes.final-audit-report/0.1` |\n\
         | Evidence rule | Accepted CYPHES receipts only |\n\n\
         ## Executive Summary\n\n\
         CYPHES processed **{} signed audit pass{}** for **{}**. \
         **{} contribution{}** ha{} accepted verification and **{} ATP Credits** were issued from receipt-backed allocations.\n\n",
        markdown_table_cell(&snapshot.campaign.protocol_name),
        snapshot.campaign.repository.full_name,
        snapshot.campaign.repository.commit_sha,
        snapshot.campaign.campaign_id,
        snapshot.campaign.skill_pack.skill_pack_id,
        snapshot.campaign.skill_pack.version,
        snapshot.campaign.skill_pack.hash,
        snapshot
            .campaign
            .custom_skill_hash
            .as_deref()
            .unwrap_or("none"),
        snapshot.contributions.len(),
        plural(snapshot.contributions.len()),
        snapshot.campaign.protocol_name,
        accepted_count,
        plural(accepted_count),
        if accepted_count == 1 { "s" } else { "ve" },
        total_credits,
    );
    if accepted_findings.is_empty() {
        report.push_str(
            "No accepted reportable vulnerability is present in this export. The report should be read as verified coverage and negative findings, not as a bounty claim.\n\n",
        );
    } else {
        report.push_str(&format!(
            "{} accepted reportable finding{} appear in the findings register below. Each finding is backed by a signed contribution and verifier decision.\n\n",
            accepted_findings.len(),
            plural(accepted_findings.len())
        ));
    }

    report.push_str("## Scope And Limits\n\n");
    report.push_str(&snapshot.campaign.scope_text);
    report.push_str(
        "\n\nThis is a repository-state audit at the pinned commit. CYPHES does not certify deployed bytecode, private-key custody, off-chain operators, production integrations, or live bounty payment unless those artifacts are explicitly supplied and receipt-backed.\n\n",
    );
    if let Some(brief) = &snapshot.campaign.audit_brief_text {
        report.push_str("### Audit Brief\n\n");
        report.push_str(brief);
        report.push_str("\n\n");
    }
    if !snapshot.campaign.attachments.is_empty() {
        report.push_str("### Requester Attachments\n\n| Label | Media Type | Hash | Bytes |\n| --- | --- | --- | ---: |\n");
        for attachment in &snapshot.campaign.attachments {
            report.push_str(&format!(
                "| {} | `{}` | `{}` | {} |\n",
                markdown_table_cell(&attachment.label),
                attachment.media_type,
                attachment.sha256,
                attachment.size_bytes
            ));
        }
        report.push('\n');
    }

    report.push_str(
        "## Methodology\n\nCYPHES v0.5 decomposes repository review into remotely claimable professional audit passes: scope mapping, repository inventory, dependency/config review, exploit-class analysis, finding validation, final report synthesis, and peer verification. Local model output is only accepted into the final report after it is signed and verified.\n\n",
    );

    report.push_str("## Audit Pass Matrix\n\n| Pass | Status | Contributions | Accepted | Receipt evidence |\n| --- | --- | ---: | ---: | --- |\n");
    for unit in &snapshot.work_units {
        let unit_contributions = snapshot
            .contributions
            .iter()
            .filter(|contribution| contribution.work_unit_id == unit.work_unit_id)
            .collect::<Vec<_>>();
        let unit_accepted = unit_contributions
            .iter()
            .filter(|contribution| accepted_ids.contains(contribution.contribution_id.as_str()))
            .count();
        let receipts = if unit_contributions.is_empty() {
            "none".to_string()
        } else {
            unit_contributions
                .iter()
                .map(|contribution| format!("`{}`", contribution.receipt_hash))
                .collect::<Vec<_>>()
                .join("<br>")
        };
        let status = match (unit.status.as_str(), unit.claimed_by_agent_id.as_deref()) {
            ("claimed", Some(agent)) => format!("claimed by `{}`", agent),
            _ => unit.status.clone(),
        };
        report.push_str(&format!(
            "| {} | `{}` | {} | {} | {} |\n",
            markdown_table_cell(&unit.title),
            markdown_table_cell(&status),
            unit_contributions.len(),
            unit_accepted,
            receipts
        ));
    }

    if !snapshot.claims.is_empty() {
        report.push_str("\n## Work Unit Claims\n\n| Work Unit | Worker | Claim | Status |\n| --- | --- | --- | --- |\n");
        for claim in &snapshot.claims {
            report.push_str(&format!(
                "| {} | `{}` | `{}` | `{}` |\n",
                markdown_table_cell(&work_unit_title(snapshot, &claim.work_unit_id)),
                claim.worker_agent_id,
                claim.claim_id,
                claim.status
            ));
        }
        report.push('\n');
    }

    report.push_str("\n## Evidence Arbitration\n\n");
    if snapshot.verifications.is_empty() {
        report.push_str("No verifier decisions are present yet. Contributions remain submitted but not accepted into the report body.\n");
    } else {
        report
            .push_str("| Verification | Decision | Reason | Target |\n| --- | --- | --- | --- |\n");
        for verification in &snapshot.verifications {
            report.push_str(&format!(
                "| `{}` | `{}` | {} | `{}` |\n",
                verification.verification_id,
                verification.decision,
                markdown_table_cell(&verification.reason_code),
                verification.target_contribution_id
            ));
        }
    }

    report.push_str("\n## Findings Register\n\n| ID | Severity | Title | Impact | Source |\n| --- | --- | --- | --- | --- |\n");
    if accepted_findings.is_empty() {
        report.push_str("| none | n/a | No accepted reportable findings yet | n/a | n/a |\n");
    } else {
        for (contribution, finding) in &accepted_findings {
            report.push_str(&format!(
                "| {} | {} | {} | {} | `{}` |\n",
                markdown_table_cell(&finding.id),
                markdown_table_cell(&finding.severity),
                markdown_table_cell(&finding.title),
                markdown_table_cell(
                    finding
                        .impact
                        .as_deref()
                        .unwrap_or("evidence-backed coverage")
                ),
                contribution.receipt_hash
            ));
        }
    }

    report.push_str("\n## Accepted Findings\n\n");
    if accepted_findings.is_empty() {
        report.push_str("No accepted reportable findings are included in this bundle.\n\n");
    } else {
        for (contribution, finding) in &accepted_findings {
            report.push_str(&format!(
                "### {} ({})\n\nSeverity: `{}`\n\nStatus: `{}`\n\nImpact: {}\n\nEvidence:\n{}\n\nReceipt: `{}`\n\n",
                finding.id,
                finding.title,
                finding.severity,
                finding.status,
                finding.impact.as_deref().unwrap_or("not declared"),
                finding
                    .evidence
                    .iter()
                    .map(|item| format!("- {item}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
                contribution.receipt_hash
            ));
        }
    }

    report.push_str("## Coverage And Negative Findings\n\n");
    for contribution in &accepted_contributions {
        report.push_str(&format!(
            "### {} / `{}`\n\n{}\n\n",
            work_unit_title(snapshot, &contribution.work_unit_id),
            contribution.contribution_id,
            contribution.notes_markdown
        ));
        if !contribution.coverage.is_empty() {
            report.push_str("| Area | Status | Evidence |\n| --- | --- | --- |\n");
            for coverage in &contribution.coverage {
                report.push_str(&format!(
                    "| {} | `{}` | {} |\n",
                    markdown_table_cell(&coverage.area),
                    coverage.status,
                    markdown_table_cell(&coverage.evidence.join("; "))
                ));
            }
            report.push('\n');
        }
    }

    report.push_str("## Non-reportable, Rejected, Or Duplicate Leads\n\n");
    let mut appendix_rows = 0usize;
    for contribution in &accepted_contributions {
        for finding in contribution
            .findings
            .iter()
            .filter(|finding| !finding.reportable)
        {
            appendix_rows += 1;
            report.push_str(&format!(
                "- `{}` / `{}`: {} (`{}`) from `{}`.\n",
                finding.id,
                finding.severity,
                finding.title,
                finding.status,
                contribution.receipt_hash
            ));
        }
    }
    for contribution in &rejected_contributions {
        appendix_rows += 1;
        report.push_str(&format!(
            "- Contribution `{}` remains appendix-only. Receipt `{}`.\n",
            contribution.contribution_id, contribution.receipt_hash
        ));
        for finding in &contribution.findings {
            report.push_str(&format!(
                "  - `{}`: {} (`{}`)\n",
                finding.id, finding.title, finding.status
            ));
        }
    }
    if appendix_rows == 0 {
        report.push_str("No rejected, duplicate, or non-reportable leads were recorded.\n");
    }

    report.push_str("\n## Runtime And Receipt Appendix\n\n");
    for contribution in &snapshot.contributions {
        report.push_str(&format!(
            "- `{}` by `{}` using `{}` / `{}`. Work unit: `{}`. Receipt: `{}`. Skill: `{}`. Output: `{}`.\n",
            contribution.contribution_id,
            contribution.worker_agent_id,
            contribution.runtime.adapter,
            contribution.runtime.model,
            work_unit_title(snapshot, &contribution.work_unit_id),
            contribution.receipt_hash,
            contribution.runtime.skill_hash.as_deref().unwrap_or("not recorded"),
            contribution.runtime.output_hash.as_deref().unwrap_or("not recorded")
        ));
    }

    report.push_str("\n## Credit Allocation Summary\n\n| Receiver | Total ATP Credits | Source |\n| --- | ---: | --- |\n");
    if snapshot.credits.is_empty() {
        report.push_str("| none | 0 | no accepted verifier receipt yet |\n");
    } else {
        for credit in &snapshot.credits {
            report.push_str(&format!(
                "| `{}` | {} | `{}` |\n",
                credit.receiver_agent_id, credit.total, credit.contribution_receipt_hash
            ));
        }
    }
    report.push_str(&format!(
        "\n## Report Integrity\n\nThis report contains {} contribution{}, {} verifier decision{}, and {} credit allocation{}. It is generated from local SQLite state and portable JSON artifacts; it does not invent missing external receipts.\n",
        snapshot.contributions.len(),
        plural(snapshot.contributions.len()),
        verification_count,
        plural(verification_count),
        snapshot.credits.len(),
        plural(snapshot.credits.len())
    ));
    report
}

fn plural(count: usize) -> &'static str {
    if count == 1 {
        ""
    } else {
        "s"
    }
}

fn markdown_table_cell(value: &str) -> String {
    value
        .replace('|', "\\|")
        .replace('\n', "<br>")
        .trim()
        .to_string()
}

fn work_unit_title(snapshot: &CampaignReportSnapshot, work_unit_id: &str) -> String {
    snapshot
        .work_units
        .iter()
        .find(|unit| unit.work_unit_id == work_unit_id)
        .map(|unit| unit.title.clone())
        .unwrap_or_else(|| work_unit_id.to_string())
}

pub fn validate_campaign(campaign: &ProtocolAuditCampaign) -> Result<(), String> {
    if campaign.profile != CAMPAIGN_PROFILE
        || campaign.profile_version != AUDIT_LABOR_PROFILE_VERSION
        || campaign.campaign_id.trim().is_empty()
        || campaign.requester_agent_id.trim().is_empty()
    {
        return Err("invalid protocol audit campaign profile or identifiers".to_string());
    }
    validate_repository_target(&campaign.repository)?;
    validate_skill_pack(&campaign.skill_pack)?;
    for attachment in &campaign.attachments {
        validate_campaign_attachment(attachment)?;
    }
    if let Some(text) = &campaign.custom_skill_text {
        let hash = campaign
            .custom_skill_hash
            .as_deref()
            .ok_or_else(|| "custom SKILL text requires a custom skill hash".to_string())?;
        if sha256_ref(text.as_bytes()) != hash {
            return Err("custom SKILL hash does not match custom SKILL text".to_string());
        }
    }
    Ok(())
}

pub fn validate_work_unit(work_unit: &AuditWorkUnit) -> Result<(), String> {
    if work_unit.profile != WORK_UNIT_PROFILE
        || work_unit.profile_version != AUDIT_LABOR_PROFILE_VERSION
        || work_unit.work_unit_id.trim().is_empty()
        || work_unit.campaign_id.trim().is_empty()
        || work_unit.kind.trim().is_empty()
        || work_unit.title.trim().is_empty()
        || work_unit.instructions.trim().is_empty()
        || work_unit.expected_artifacts.is_empty()
    {
        return Err("invalid audit work unit".to_string());
    }
    Ok(())
}

pub fn validate_work_unit_claim(claim: &AuditWorkUnitClaim) -> Result<(), String> {
    if claim.profile != WORK_UNIT_CLAIM_PROFILE
        || claim.profile_version != AUDIT_LABOR_PROFILE_VERSION
        || claim.claim_id.trim().is_empty()
        || claim.campaign_id.trim().is_empty()
        || claim.work_unit_id.trim().is_empty()
        || claim.requester_agent_id.trim().is_empty()
        || claim.worker_agent_id.trim().is_empty()
        || claim.claim_hash.trim().is_empty()
        || claim.signature.trim().is_empty()
    {
        return Err("invalid signed work unit claim".to_string());
    }
    match claim.status.as_str() {
        "claimed" | "released" | "expired" => Ok(()),
        _ => Err("unsupported work unit claim status".to_string()),
    }
}

pub fn validate_contribution(contribution: &NodeContribution) -> Result<(), String> {
    if contribution.profile != CONTRIBUTION_PROFILE
        || contribution.profile_version != AUDIT_LABOR_PROFILE_VERSION
        || contribution.contribution_id.trim().is_empty()
        || contribution.campaign_id.trim().is_empty()
        || contribution.work_unit_id.trim().is_empty()
        || contribution.worker_agent_id.trim().is_empty()
        || contribution.notes_markdown.trim().is_empty()
        || contribution.artifacts.is_empty()
        || contribution.coverage.is_empty()
        || contribution.signature.trim().is_empty()
    {
        return Err("invalid signed contribution".to_string());
    }
    for artifact in &contribution.artifacts {
        validate_artifact(artifact)?;
    }
    Ok(())
}

pub fn validate_verification(verification: &VerificationResult) -> Result<(), String> {
    if verification.profile != VERIFICATION_PROFILE
        || verification.profile_version != AUDIT_LABOR_PROFILE_VERSION
        || verification.verification_id.trim().is_empty()
        || verification.campaign_id.trim().is_empty()
        || verification.target_contribution_id.trim().is_empty()
        || verification.verifier_agent_id.trim().is_empty()
        || verification.reason_code.trim().is_empty()
        || verification.reason.trim().is_empty()
        || verification.signature.trim().is_empty()
    {
        return Err("invalid signed verification".to_string());
    }
    validate_verification_decision(&verification.decision)
}

pub fn sha256_ref(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!(
        "sha256:{}",
        digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

fn contribution_signature_value(
    contribution: &NodeContribution,
) -> Result<serde_json::Value, String> {
    let mut value = serde_json::to_value(contribution).map_err(|error| error.to_string())?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| "contribution must serialize as an object".to_string())?;
    object.remove("contributionHash");
    object.remove("receiptHash");
    object.remove("signature");
    Ok(value)
}

fn verification_signature_value(
    verification: &VerificationResult,
) -> Result<serde_json::Value, String> {
    let mut value = serde_json::to_value(verification).map_err(|error| error.to_string())?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| "verification must serialize as an object".to_string())?;
    object.remove("verificationHash");
    object.remove("signature");
    Ok(value)
}

fn claim_signature_value(claim: &AuditWorkUnitClaim) -> Result<serde_json::Value, String> {
    let mut value = serde_json::to_value(claim).map_err(|error| error.to_string())?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| "claim must serialize as an object".to_string())?;
    object.remove("claimHash");
    object.remove("signature");
    Ok(value)
}

fn contribution_receipt_hash(contribution: &NodeContribution) -> Result<String, String> {
    canonical_hash(&json!({
        "profile": CONTRIBUTION_PROFILE,
        "receiptType": "NodeContributionReceipt",
        "campaignId": contribution.campaign_id,
        "workUnitId": contribution.work_unit_id,
        "contributionId": contribution.contribution_id,
        "workerAgentId": contribution.worker_agent_id,
        "contributionHash": contribution.contribution_hash,
        "artifacts": contribution.artifacts,
        "createdAt": contribution.created_at,
    }))
}

fn canonical_hash<T: Serialize>(value: &T) -> Result<String, String> {
    let bytes = serde_jcs::to_vec(value).map_err(|error| error.to_string())?;
    Ok(sha256_ref(&bytes))
}

pub fn default_skill_pack_reference() -> SkillPackReference {
    SkillPackReference {
        skill_pack_id: DEFAULT_SKILL_PACK_ID.to_string(),
        version: DEFAULT_SKILL_PACK_VERSION.to_string(),
        hash: sha256_ref(DEFAULT_AUDIT_SKILL_TEXT.as_bytes()),
        label: DEFAULT_SKILL_PACK_LABEL.to_string(),
    }
}

fn validate_repository_target(repository: &RepositoryTarget) -> Result<(), String> {
    if repository.full_name.split('/').count() != 2
        || repository.url != format!("https://github.com/{}", repository.full_name)
        || !is_git_commit_sha(&repository.commit_sha)
    {
        return Err(
            "campaign repository must be a public GitHub repository pinned to a commit SHA"
                .to_string(),
        );
    }
    Ok(())
}

fn validate_skill_pack(skill_pack: &SkillPackReference) -> Result<(), String> {
    if skill_pack.skill_pack_id.trim().is_empty()
        || skill_pack.version.trim().is_empty()
        || skill_pack.label.trim().is_empty()
        || !is_sha256_ref(&skill_pack.hash)
    {
        return Err("skill pack requires id, version, label, and SHA-256 hash".to_string());
    }
    Ok(())
}

fn validate_campaign_attachment(attachment: &CampaignAttachment) -> Result<(), String> {
    if attachment.attachment_id.trim().is_empty()
        || attachment.label.trim().is_empty()
        || attachment.media_type.trim().is_empty()
        || attachment.size_bytes == 0
        || !is_sha256_ref(&attachment.sha256)
    {
        return Err(
            "campaign attachment requires id, label, media type, size, and SHA-256".to_string(),
        );
    }
    if let Some(text) = &attachment.text {
        if text.len() as u64 != attachment.size_bytes
            || sha256_ref(text.as_bytes()) != attachment.sha256
        {
            return Err("campaign attachment text does not match recorded hash".to_string());
        }
    }
    Ok(())
}

fn validate_artifact(artifact: &ContributionArtifact) -> Result<(), String> {
    if artifact.path.trim().is_empty()
        || artifact.media_type.trim().is_empty()
        || artifact.size_bytes == 0
        || !is_sha256_ref(&artifact.sha256)
    {
        return Err(
            "contribution artifacts require path, media type, size, and SHA-256".to_string(),
        );
    }
    Ok(())
}

fn validate_verification_decision(decision: &str) -> Result<(), String> {
    match decision {
        "accepted" | "rejected" | "reproduced" | "challenged" | "revision_requested" => Ok(()),
        _ => Err("unsupported verification decision".to_string()),
    }
}

fn is_sha256_ref(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}

fn work_unit_difficulty(work_unit_id: &str) -> f64 {
    if work_unit_id.contains("defi-exploit-class-pass")
        || work_unit_id.contains("finding-validation")
        || work_unit_id.contains("peer-verification")
    {
        1.35
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn artifact(path: &str) -> ContributionArtifact {
        ContributionArtifact {
            path: path.to_string(),
            media_type: "text/markdown".to_string(),
            sha256: "sha256:1111111111111111111111111111111111111111111111111111111111111111"
                .to_string(),
            size_bytes: 100,
        }
    }

    #[test]
    fn scoring_applies_model_multiplier_and_penalty() {
        let points = credit_score(CreditScoreInput {
            base_points: 100.0,
            difficulty_multiplier: 1.5,
            verification_multiplier: 1.0,
            model_multiplier: 0.6,
            requester_approval: 1.0,
            penalty_points: 10.0,
        })
        .unwrap();
        assert_eq!(points, 80);
    }

    #[test]
    fn credits_require_accepted_verification_and_receipt() {
        let worker = identity::Keypair::generate_ed25519();
        let verifier = identity::Keypair::generate_ed25519();
        let contribution = signed_contribution(
            &worker,
            "campaign-1".to_string(),
            "campaign-1-04-defi-exploit-class-pass".to_string(),
            RuntimeDescriptor::deterministic_fixture(),
            "Reviewed reentrancy and oracle applicability with no reportable finding.".to_string(),
            vec![],
            vec![artifact("notes.md")],
            vec![CoverageItem {
                area: "reentrancy".to_string(),
                status: "not_applicable".to_string(),
                evidence: vec!["No external value transfer path in this fixture.".to_string()],
            }],
            vec!["no code execution".to_string()],
        )
        .unwrap();
        let rejected = signed_verification(
            &verifier,
            "campaign-1".to_string(),
            contribution.contribution_id.clone(),
            "rejected".to_string(),
            "INSUFFICIENT_EVIDENCE".to_string(),
            "Evidence did not support the submitted lead.".to_string(),
            vec![],
            vec![artifact("verification.md")],
        )
        .unwrap();
        assert!(allocate_credits(&contribution, &rejected).is_err());

        let accepted = signed_verification(
            &verifier,
            "campaign-1".to_string(),
            contribution.contribution_id.clone(),
            "accepted".to_string(),
            "COVERAGE_ACCEPTED".to_string(),
            "Coverage evidence is bounded and useful.".to_string(),
            vec![VerificationEvidence {
                label: "review".to_string(),
                reference: "verification.md".to_string(),
            }],
            vec![artifact("verification.md")],
        )
        .unwrap();
        let credits = allocate_credits(&contribution, &accepted).unwrap();
        assert_eq!(credits.len(), 2);
        assert!(credits.iter().all(|credit| credit.total > 0));
    }

    #[test]
    fn parser_fallback_contributions_receive_quality_deduction() {
        let worker = identity::Keypair::generate_ed25519();
        let verifier = identity::Keypair::generate_ed25519();
        let structured = signed_contribution(
            &worker,
            "campaign-1".to_string(),
            "campaign-1-02-repository-inventory".to_string(),
            RuntimeDescriptor::deterministic_fixture(),
            "Structured negative coverage with evidence.".to_string(),
            vec![],
            vec![artifact("inventory.md")],
            vec![CoverageItem {
                area: "repository inventory".to_string(),
                status: "completed".to_string(),
                evidence: vec!["README.md reviewed at pinned commit.".to_string()],
            }],
            vec!["local model audit skill completed".to_string()],
        )
        .unwrap();
        let fallback = signed_contribution(
            &worker,
            "campaign-1".to_string(),
            "campaign-1-02-repository-inventory".to_string(),
            RuntimeDescriptor::deterministic_fixture(),
            "Raw notes.\n\n> CYPHES parser note: model output was not valid structured JSON: no JSON object start found".to_string(),
            vec![],
            vec![artifact("audit-skill-output.md")],
            vec![CoverageItem {
                area: "local model output".to_string(),
                status: "needs_review".to_string(),
                evidence: vec![
                    "Model returned unstructured output; no reportable finding accepted.".to_string(),
                ],
            }],
            vec![
                "local model audit skill response captured; structured parse failed".to_string(),
            ],
        )
        .unwrap();
        let structured_verification = signed_verification(
            &verifier,
            "campaign-1".to_string(),
            structured.contribution_id.clone(),
            "accepted".to_string(),
            "COVERAGE_ACCEPTED".to_string(),
            "Structured evidence is bounded and useful.".to_string(),
            vec![],
            vec![artifact("verification.md")],
        )
        .unwrap();
        let fallback_verification = signed_verification(
            &verifier,
            "campaign-1".to_string(),
            fallback.contribution_id.clone(),
            "accepted".to_string(),
            "PARSER_FALLBACK_ACCEPTED".to_string(),
            "Fallback notes are accepted with reduced credit.".to_string(),
            vec![],
            vec![artifact("verification.md")],
        )
        .unwrap();

        let structured_worker_total = allocate_credits(&structured, &structured_verification)
            .unwrap()
            .into_iter()
            .find(|credit| credit.receiver_agent_id == structured.worker_agent_id)
            .unwrap()
            .total;
        let fallback_worker_credit = allocate_credits(&fallback, &fallback_verification)
            .unwrap()
            .into_iter()
            .find(|credit| credit.receiver_agent_id == fallback.worker_agent_id)
            .unwrap();

        assert_eq!(structured_worker_total, 35);
        assert_eq!(fallback_worker_credit.total, 4);
        assert!(fallback_worker_credit
            .formula
            .contains("quality(0.10 parser fallback)"));
    }

    #[test]
    fn large_model_bonus_requires_excellent_output() {
        let worker = identity::Keypair::generate_ed25519();
        let verifier = identity::Keypair::generate_ed25519();
        let mut large_model = RuntimeDescriptor::deterministic_fixture();
        large_model.model = "llama-3.3-70b".to_string();
        large_model.model_multiplier = 3.0;
        let ordinary = signed_contribution(
            &worker,
            "campaign-1".to_string(),
            "campaign-1-02-repository-inventory".to_string(),
            large_model.clone(),
            "Structured coverage with one evidence-backed area.".to_string(),
            vec![],
            vec![artifact("inventory.md")],
            vec![CoverageItem {
                area: "repository inventory".to_string(),
                status: "completed".to_string(),
                evidence: vec!["README.md reviewed at pinned commit.".to_string()],
            }],
            vec!["local model audit skill completed".to_string()],
        )
        .unwrap();
        let excellent = signed_contribution(
            &worker,
            "campaign-1".to_string(),
            "campaign-1-03-finding-validation".to_string(),
            large_model,
            "Validated a reportable candidate with evidence.".to_string(),
            vec![AuditFinding {
                id: "CYPHES-001".to_string(),
                title: "Reportable issue".to_string(),
                severity: "high".to_string(),
                status: "candidate".to_string(),
                impact: Some("fund loss".to_string()),
                evidence: vec!["src/Vault.sol:42".to_string()],
                reportable: true,
            }],
            vec![artifact("finding.md")],
            vec![CoverageItem {
                area: "finding validation".to_string(),
                status: "completed".to_string(),
                evidence: vec!["PoC reaches impact path.".to_string()],
            }],
            vec!["local model audit skill completed".to_string()],
        )
        .unwrap();
        let ordinary_verification = signed_verification(
            &verifier,
            "campaign-1".to_string(),
            ordinary.contribution_id.clone(),
            "accepted".to_string(),
            "COVERAGE_ACCEPTED".to_string(),
            "Structured evidence is bounded and useful.".to_string(),
            vec![],
            vec![artifact("verification.md")],
        )
        .unwrap();
        let excellent_verification = signed_verification(
            &verifier,
            "campaign-1".to_string(),
            excellent.contribution_id.clone(),
            "accepted".to_string(),
            "FINDING_ACCEPTED".to_string(),
            "Reportable finding accepted.".to_string(),
            vec![],
            vec![artifact("verification.md")],
        )
        .unwrap();

        let ordinary_credit = allocate_credits(&ordinary, &ordinary_verification)
            .unwrap()
            .into_iter()
            .find(|credit| credit.receiver_agent_id == ordinary.worker_agent_id)
            .unwrap();
        let excellent_credit = allocate_credits(&excellent, &excellent_verification)
            .unwrap()
            .into_iter()
            .find(|credit| credit.receiver_agent_id == excellent.worker_agent_id)
            .unwrap();

        assert_eq!(ordinary_credit.total, 115);
        assert!(ordinary_credit.formula.contains("standard-output cap"));
        assert_eq!(excellent_credit.buckets.participation, 405);
        assert!(excellent_credit.total >= ordinary_credit.total * 4);
        assert!(excellent_credit.formula.contains("excellent-output bonus"));
    }

    #[test]
    fn credits_require_an_independent_verifier() {
        let worker = identity::Keypair::generate_ed25519();
        let contribution = signed_contribution(
            &worker,
            "campaign-1".to_string(),
            "campaign-1-02-repository-inventory".to_string(),
            RuntimeDescriptor::deterministic_fixture(),
            "Mapped repository inventory with bounded evidence.".to_string(),
            vec![],
            vec![artifact("inventory.md")],
            vec![CoverageItem {
                area: "repository inventory".to_string(),
                status: "completed".to_string(),
                evidence: vec!["No repository code execution.".to_string()],
            }],
            vec!["no code execution".to_string()],
        )
        .unwrap();
        let self_verification = signed_verification(
            &worker,
            "campaign-1".to_string(),
            contribution.contribution_id.clone(),
            "accepted".to_string(),
            "SELF_ACCEPTED".to_string(),
            "Self-verification is useful for local preview but cannot issue earned ATP."
                .to_string(),
            vec![],
            vec![artifact("verification.md")],
        )
        .unwrap();
        assert!(allocate_credits(&contribution, &self_verification).is_err());
    }

    #[test]
    fn final_report_keeps_rejected_findings_out_of_findings_section() {
        let requester = "urn:libp2p:12D3KooRequester".to_string();
        let campaign = ProtocolAuditCampaign::new(
            "Fixture Protocol".to_string(),
            RepositoryTarget {
                full_name: "fixture/protocol".to_string(),
                url: "https://github.com/fixture/protocol".to_string(),
                commit_sha: "0000000000000000000000000000000000000001".to_string(),
            },
            "Audit pool accounting.".to_string(),
            None,
            vec!["principal theft".to_string()],
            vec!["best practice notes".to_string()],
            Some("brief".to_string()),
            None,
            Vec::new(),
            None,
            requester,
        )
        .unwrap();
        let worker = identity::Keypair::generate_ed25519();
        let verifier = identity::Keypair::generate_ed25519();
        let accepted = signed_contribution(
            &worker,
            campaign.campaign_id.clone(),
            format!("{}-03-finding-validation", campaign.campaign_id),
            RuntimeDescriptor::deterministic_fixture(),
            "Validated candidate against the reportability gate.".to_string(),
            vec![AuditFinding {
                id: "CYPHES-001".to_string(),
                title: "Accepted issue".to_string(),
                severity: "medium".to_string(),
                status: "candidate".to_string(),
                impact: Some("loss of rewards".to_string()),
                evidence: vec!["src/Rewards.sol:10".to_string()],
                reportable: true,
            }],
            vec![artifact("findings.json")],
            vec![CoverageItem {
                area: "reportability gate".to_string(),
                status: "passed".to_string(),
                evidence: vec!["PoC maps to impact.".to_string()],
            }],
            vec![],
        )
        .unwrap();
        let rejected = signed_contribution(
            &worker,
            campaign.campaign_id.clone(),
            format!("{}-03-finding-validation", campaign.campaign_id),
            RuntimeDescriptor::deterministic_fixture(),
            "Lead was duplicate.".to_string(),
            vec![AuditFinding {
                id: "CYPHES-002".to_string(),
                title: "Rejected lead".to_string(),
                severity: "high".to_string(),
                status: "duplicate".to_string(),
                impact: Some("principal theft".to_string()),
                evidence: vec!["known audit report".to_string()],
                reportable: true,
            }],
            vec![artifact("duplicate.md")],
            vec![CoverageItem {
                area: "known issue search".to_string(),
                status: "failed".to_string(),
                evidence: vec!["Duplicate found.".to_string()],
            }],
            vec![],
        )
        .unwrap();
        let accepted_verification = signed_verification(
            &verifier,
            campaign.campaign_id.clone(),
            accepted.contribution_id.clone(),
            "accepted".to_string(),
            "FINDING_ACCEPTED".to_string(),
            "Accepted.".to_string(),
            vec![],
            vec![artifact("verification-a.md")],
        )
        .unwrap();
        let rejected_verification = signed_verification(
            &verifier,
            campaign.campaign_id.clone(),
            rejected.contribution_id.clone(),
            "rejected".to_string(),
            "DUPLICATE".to_string(),
            "Duplicate.".to_string(),
            vec![],
            vec![artifact("verification-r.md")],
        )
        .unwrap();
        let snapshot = CampaignReportSnapshot {
            campaign: campaign.clone(),
            work_units: default_work_units(&campaign),
            claims: Vec::new(),
            contributions: vec![accepted.clone(), rejected.clone()],
            verifications: vec![accepted_verification.clone(), rejected_verification],
            credits: allocate_credits(&accepted, &accepted_verification).unwrap(),
        };
        let report = final_report_markdown(&snapshot);
        let findings_section = report
            .split("## Non-reportable, Rejected, Or Duplicate Leads")
            .next()
            .unwrap();
        assert!(findings_section.contains("Accepted issue"));
        assert!(!findings_section.contains("Rejected lead"));
        assert!(report.contains("Rejected lead"));
    }
}
