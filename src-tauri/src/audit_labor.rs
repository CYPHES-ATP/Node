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
pub const CONTRIBUTION_PROFILE: &str = "cyphes.audit-contribution/0.1";
pub const VERIFICATION_PROFILE: &str = "cyphes.verification-result/0.1";
pub const CREDIT_PROFILE: &str = "cyphes.credit-ledger/0.1";
pub const FINAL_REPORT_PROFILE: &str = "cyphes.final-audit-report/0.1";
pub const AUDIT_LABOR_PROFILE_VERSION: &str = "0.1";

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
        requester_agent_id: String,
    ) -> Result<Self, String> {
        validate_repository_target(&repository)?;
        if protocol_name.trim().is_empty() || scope_text.trim().is_empty() {
            return Err("protocol name and scope are required".to_string());
        }
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
    pub created_at: String,
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CampaignReportSnapshot {
    pub campaign: ProtocolAuditCampaign,
    pub work_units: Vec<AuditWorkUnit>,
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
                created_at: now_rfc3339(),
            },
        )
        .collect()
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
    let participation = credit_score(CreditScoreInput {
        base_points: 100.0,
        difficulty_multiplier: work_unit_difficulty(&contribution.work_unit_id),
        verification_multiplier: 1.0,
        model_multiplier: contribution.runtime.model_multiplier,
        requester_approval: 1.0,
        penalty_points: 0.0,
    })?;
    let coverage = high_quality_coverage.saturating_mul(15);
    let finding = reportable_findings.saturating_mul(75);
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
        formula: "base * difficulty * verification * model * requesterApproval - penalties"
            .to_string(),
        issued_at: now_rfc3339(),
    }];
    allocations[0].total = allocations[0].buckets.total();

    if verification.verifier_agent_id != contribution.worker_agent_id {
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
                verification: 40,
                coverage: 0,
                finding: 0,
                bonus_allocation_placeholder: 0,
            },
            total: 0,
            formula: "accepted peer verification credit".to_string(),
            issued_at: now_rfc3339(),
        };
        verifier_allocation.total = verifier_allocation.buckets.total();
        allocations.push(verifier_allocation);
    }
    Ok(allocations)
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
    let mut report = format!(
        "# CYPHES Protocol Audit Report\n\n\
         ## Executive Summary\n\n\
         Protocol: **{}**\n\n\
         Repository: `{}` at `{}`\n\n\
         This report is generated from accepted CYPHES audit-labor receipts only. \
         It does not claim bounty payment, token settlement, or unverified exploit validity.\n\n\
         ## Scope\n\n{}\n\n\
         ## Methodology\n\n\
         CYPHES applied the repository discovery, evidence-first audit, DeFi exploit-class pass, \
         reportability gate, and peer verification process defined for this campaign.\n\n\
         ## Work Units Completed\n\n",
        snapshot.campaign.protocol_name,
        snapshot.campaign.repository.full_name,
        snapshot.campaign.repository.commit_sha,
        snapshot.campaign.scope_text
    );
    for unit in &snapshot.work_units {
        report.push_str(&format!("- {}: `{}`\n", unit.title, unit.status));
    }
    report.push_str(
        "\n## Findings Table\n\n| ID | Severity | Title | Impact |\n| --- | --- | --- | --- |\n",
    );
    let mut accepted_finding_count = 0;
    for contribution in &accepted_contributions {
        for finding in contribution
            .findings
            .iter()
            .filter(|finding| finding.reportable)
        {
            accepted_finding_count += 1;
            report.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                finding.id,
                finding.severity,
                finding.title,
                finding
                    .impact
                    .as_deref()
                    .unwrap_or("evidence-backed coverage")
            ));
        }
    }
    if accepted_finding_count == 0 {
        report.push_str("| none | n/a | No accepted reportable findings yet | n/a |\n");
    }
    report.push_str("\n## Accepted Findings\n\n");
    for contribution in &accepted_contributions {
        for finding in contribution
            .findings
            .iter()
            .filter(|finding| finding.reportable)
        {
            report.push_str(&format!(
                "### {}\n\nSeverity: `{}`\n\nImpact: `{}`\n\nEvidence:\n{}\n\n",
                finding.title,
                finding.severity,
                finding.impact.as_deref().unwrap_or("not declared"),
                finding
                    .evidence
                    .iter()
                    .map(|item| format!("- {item}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }
    }
    report.push_str("\n## Coverage Evidence\n\n");
    for contribution in &accepted_contributions {
        report.push_str(&format!(
            "### Contribution `{}`\n\n{}\n\n",
            contribution.contribution_id, contribution.notes_markdown
        ));
        for coverage in &contribution.coverage {
            report.push_str(&format!(
                "- {}: `{}` ({})\n",
                coverage.area,
                coverage.status,
                coverage.evidence.join("; ")
            ));
        }
        report.push('\n');
    }
    report.push_str("\n## Rejected / Duplicate / Non-reportable Leads\n\n");
    for contribution in &rejected_contributions {
        report.push_str(&format!(
            "- Contribution `{}` remains appendix-only. Receipt `{}`.\n",
            contribution.contribution_id, contribution.receipt_hash
        ));
        for finding in &contribution.findings {
            report.push_str(&format!(
                "  - {}: {} (`{}`)\n",
                finding.id, finding.title, finding.status
            ));
        }
    }
    report.push_str("\n## Node Contribution Appendix\n\n");
    for contribution in &snapshot.contributions {
        report.push_str(&format!(
            "- `{}` by `{}` using `{}` / `{}`. Receipt: `{}`.\n",
            contribution.contribution_id,
            contribution.worker_agent_id,
            contribution.runtime.adapter,
            contribution.runtime.model,
            contribution.receipt_hash
        ));
    }
    report.push_str("\n## Receipt Appendix\n\n");
    for verification in &snapshot.verifications {
        report.push_str(&format!(
            "- Verification `{}`: `{}` / `{}` targeting `{}`.\n",
            verification.verification_id,
            verification.decision,
            verification.reason_code,
            verification.target_contribution_id
        ));
    }
    report.push_str("\n## Credit Allocation Summary\n\n| Receiver | Total ATP Credits | Source |\n| --- | ---: | --- |\n");
    for credit in &snapshot.credits {
        report.push_str(&format!(
            "| `{}` | {} | `{}` |\n",
            credit.receiver_agent_id, credit.total, credit.contribution_receipt_hash
        ));
    }
    report
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
    Ok(())
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
            contributions: vec![accepted.clone(), rejected.clone()],
            verifications: vec![accepted_verification.clone(), rejected_verification],
            credits: allocate_credits(&accepted, &accepted_verification).unwrap(),
        };
        let report = final_report_markdown(&snapshot);
        let findings_section = report
            .split("## Rejected / Duplicate / Non-reportable Leads")
            .next()
            .unwrap();
        assert!(findings_section.contains("Accepted issue"));
        assert!(!findings_section.contains("Rejected lead"));
        assert!(report.contains("Rejected lead"));
    }
}
