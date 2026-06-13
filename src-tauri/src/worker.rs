use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Utc};
use flate2::read::GzDecoder;
use libp2p::identity;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tar::Archive;
use uuid::Uuid;
use walkdir::WalkDir;

use crate::{
    atp::{
        agent_id, now_rfc3339, public_key_from_raw_ed25519, raw_ed25519_public_key, sign_canonical,
        verify_canonical,
    },
    audit_profile::{AuditArtifact, AuditContract, RepositoryTarget},
};

const MAX_ARCHIVE_BYTES: usize = 100 * 1024 * 1024;
const MAX_SCANNED_FILES: usize = 25_000;
const MAX_INSPECTED_FILE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LeaseTtl {
    pub start: String,
    pub end: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ContextLease {
    pub id: String,
    pub issuer: String,
    pub resource_ref: String,
    pub operations: Vec<String>,
    pub purpose: String,
    pub boundary: String,
    pub retention: String,
    pub audit: bool,
    pub nonce: String,
    pub ttl: LeaseTtl,
    pub sig: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LeaseSigningPayload<'a> {
    id: &'a str,
    issuer: &'a str,
    resource_ref: &'a str,
    operations: &'a [String],
    purpose: &'a str,
    boundary: &'a str,
    retention: &'a str,
    audit: bool,
    nonce: &'a str,
    ttl: &'a LeaseTtl,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LeaseAccess {
    pub allowed: bool,
    pub lease_id: String,
    pub operation: String,
    pub path: String,
    pub reason: String,
    pub time: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionArtifact {
    pub path: String,
    pub media_type: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub content_base64: String,
}

impl ExecutionArtifact {
    pub fn receipt_record(&self) -> AuditArtifact {
        AuditArtifact {
            path: self.path.clone(),
            media_type: self.media_type.clone(),
            sha256: self.sha256.clone(),
            size_bytes: self.size_bytes,
        }
    }

    pub fn bytes(&self) -> Result<Vec<u8>, String> {
        URL_SAFE_NO_PAD
            .decode(&self.content_base64)
            .map_err(|_| format!("artifact {} is not valid base64url", self.path))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SignedExecutionResult {
    pub transaction_id: String,
    pub worker_agent_id: String,
    pub contract_hash: String,
    pub repository: RepositoryTarget,
    pub lease_ids: Vec<String>,
    pub access_log: Vec<LeaseAccess>,
    pub artifacts: Vec<ExecutionArtifact>,
    pub created_at: String,
    pub public_key_base64_url: String,
    pub result_hash: String,
    pub signature: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ExecutionSigningPayload<'a> {
    transaction_id: &'a str,
    worker_agent_id: &'a str,
    contract_hash: &'a str,
    repository: &'a RepositoryTarget,
    lease_ids: &'a [String],
    access_log: &'a [LeaseAccess],
    artifacts: &'a [ExecutionArtifact],
    created_at: &'a str,
    public_key_base64_url: &'a str,
}

impl SignedExecutionResult {
    fn signing_payload(&self) -> ExecutionSigningPayload<'_> {
        ExecutionSigningPayload {
            transaction_id: &self.transaction_id,
            worker_agent_id: &self.worker_agent_id,
            contract_hash: &self.contract_hash,
            repository: &self.repository,
            lease_ids: &self.lease_ids,
            access_log: &self.access_log,
            artifacts: &self.artifacts,
            created_at: &self.created_at,
            public_key_base64_url: &self.public_key_base64_url,
        }
    }

    pub fn verify(&self) -> Result<(), String> {
        let public_bytes = URL_SAFE_NO_PAD
            .decode(&self.public_key_base64_url)
            .map_err(|_| "execution result public key is not valid base64url".to_string())?;
        let public_key = public_key_from_raw_ed25519(&public_bytes)?;
        if agent_id(&public_key) != self.worker_agent_id {
            return Err("execution result key does not match worker identity".to_string());
        }
        let expected_hash = canonical_hash(&self.signing_payload())?;
        if self.result_hash != expected_hash {
            return Err("execution result hash mismatch".to_string());
        }
        verify_canonical(&public_key, &self.signing_payload(), &self.signature)?;

        for artifact in &self.artifacts {
            let bytes = artifact.bytes()?;
            if artifact.size_bytes != bytes.len() as u64 || artifact.sha256 != sha256_bytes(&bytes)
            {
                return Err(format!(
                    "artifact {} failed hash verification",
                    artifact.path
                ));
            }
        }
        Ok(())
    }
}

pub fn create_repository_leases(
    keypair: &identity::Keypair,
    contract: &AuditContract,
) -> Result<Vec<ContextLease>, String> {
    let start = now_rfc3339();
    let end = contract.expires_at.clone();
    let resource = repository_resource(&contract.repository);
    let artifact_boundary = format!("artifacts:{}", contract.transaction_id);
    let mut leases = vec![
        ContextLease {
            id: format!("lease-repository-read-{}", Uuid::new_v4()),
            issuer: contract.requester_agent_id.clone(),
            resource_ref: resource.clone(),
            operations: vec!["read".to_string()],
            purpose: "repository-security-audit".to_string(),
            boundary: resource,
            retention: "delete-checkout-after-receipt".to_string(),
            audit: true,
            nonce: Uuid::new_v4().to_string(),
            ttl: LeaseTtl {
                start: start.clone(),
                end: end.clone(),
            },
            sig: String::new(),
        },
        ContextLease {
            id: format!("lease-artifacts-write-{}", Uuid::new_v4()),
            issuer: contract.requester_agent_id.clone(),
            resource_ref: artifact_boundary.clone(),
            operations: vec!["write".to_string()],
            purpose: "repository-security-audit-artifacts".to_string(),
            boundary: artifact_boundary,
            retention: "retain-verification-bundle".to_string(),
            audit: true,
            nonce: Uuid::new_v4().to_string(),
            ttl: LeaseTtl { start, end },
            sig: String::new(),
        },
    ];
    for lease in &mut leases {
        lease.sig = sign_canonical(keypair, &lease_signing_payload(lease))?;
    }
    Ok(leases)
}

pub fn verify_leases(
    leases: &[ContextLease],
    requester_public_key: &identity::PublicKey,
    contract: &AuditContract,
) -> Result<(), String> {
    if leases.len() != 2 || agent_id(requester_public_key) != contract.requester_agent_id {
        return Err(
            "route must contain requester-signed repository and artifact leases".to_string(),
        );
    }
    let now = Utc::now();
    let repository = repository_resource(&contract.repository);
    let artifact_boundary = format!("artifacts:{}", contract.transaction_id);
    let mut has_read = false;
    let mut has_write = false;
    for lease in leases {
        if lease.issuer != contract.requester_agent_id || !lease.audit {
            return Err("lease issuer or audit policy does not match the contract".to_string());
        }
        verify_canonical(
            requester_public_key,
            &lease_signing_payload(lease),
            &lease.sig,
        )?;
        let start = parse_time(&lease.ttl.start)?;
        let end = parse_time(&lease.ttl.end)?;
        if now < start || now >= end || end > parse_time(&contract.expires_at)? {
            return Err("lease is not active within the contract lifetime".to_string());
        }
        has_read |= lease.boundary == repository
            && lease.resource_ref == repository
            && lease.operations.len() == 1
            && lease.operations[0] == "read";
        has_write |= lease.boundary == artifact_boundary
            && lease.resource_ref == artifact_boundary
            && lease.operations.len() == 1
            && lease.operations[0] == "write";
    }
    if !has_read || !has_write {
        return Err(
            "route leases do not grant the exact repository read and artifact write bounds"
                .to_string(),
        );
    }
    Ok(())
}

pub async fn execute_repository_audit(
    keypair: &identity::Keypair,
    contract: &AuditContract,
    contract_hash: &str,
    leases: &[ContextLease],
    data_dir: &Path,
) -> Result<SignedExecutionResult, String> {
    let session_dir = data_dir.join("work").join(&contract.transaction_id);
    if session_dir.exists() {
        fs::remove_dir_all(&session_dir).map_err(|error| error.to_string())?;
    }
    let checkout_dir = session_dir.join("checkout");
    let artifact_dir = session_dir.join("artifacts");
    fs::create_dir_all(&checkout_dir).map_err(|error| error.to_string())?;
    fs::create_dir_all(&artifact_dir).map_err(|error| error.to_string())?;

    download_checkout(&contract.repository, &checkout_dir).await?;
    make_read_only(&checkout_dir)?;

    let repository_lease = leases
        .iter()
        .find(|lease| lease.operations.len() == 1 && lease.operations[0] == "read")
        .ok_or_else(|| "repository read lease missing".to_string())?;
    let artifact_lease = leases
        .iter()
        .find(|lease| lease.operations.len() == 1 && lease.operations[0] == "write")
        .ok_or_else(|| "artifact write lease missing".to_string())?;
    let guard = LeaseGuard::new(
        repository_lease.clone(),
        artifact_lease.clone(),
        checkout_dir.clone(),
        artifact_dir.clone(),
    );
    let (files, access_log) = scan_repository(&guard)?;
    let artifact_contents = build_artifacts(contract, &files)?;
    let mut result_access_log = access_log;
    let mut artifacts = Vec::new();
    for (path, media_type, bytes) in artifact_contents {
        guard.write_artifact(&path, &bytes, &mut result_access_log)?;
        artifacts.push(ExecutionArtifact {
            path,
            media_type,
            sha256: sha256_bytes(&bytes),
            size_bytes: bytes.len() as u64,
            content_base64: URL_SAFE_NO_PAD.encode(bytes),
        });
    }

    let public_key = raw_ed25519_public_key(&keypair.public())?;
    let mut result = SignedExecutionResult {
        transaction_id: contract.transaction_id.clone(),
        worker_agent_id: agent_id(&keypair.public()),
        contract_hash: contract_hash.to_string(),
        repository: contract.repository.clone(),
        lease_ids: leases.iter().map(|lease| lease.id.clone()).collect(),
        access_log: result_access_log,
        artifacts,
        created_at: now_rfc3339(),
        public_key_base64_url: URL_SAFE_NO_PAD.encode(public_key),
        result_hash: String::new(),
        signature: String::new(),
    };
    result.result_hash = canonical_hash(&result.signing_payload())?;
    result.signature = sign_canonical(keypair, &result.signing_payload())?;

    if contract.execution.delete_checkout_after_receipt {
        make_writable(&checkout_dir)?;
        fs::remove_dir_all(&checkout_dir).map_err(|error| error.to_string())?;
    }
    Ok(result)
}

pub fn verify_execution_result(
    result: &SignedExecutionResult,
    contract: &AuditContract,
    leases: &[ContextLease],
) -> Result<(), String> {
    result.verify()?;
    if result.transaction_id != contract.transaction_id
        || result.worker_agent_id != contract.worker_agent_id
        || result.contract_hash != crate::audit_profile::contract_hash(contract)?
        || result.repository != contract.repository
    {
        return Err("execution result does not bind the accepted contract".to_string());
    }
    let expected_ids = leases
        .iter()
        .map(|lease| lease.id.as_str())
        .collect::<BTreeSet<_>>();
    let result_ids = result
        .lease_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if expected_ids != result_ids {
        return Err("execution result lease set mismatch".to_string());
    }
    for access in &result.access_log {
        let lease = leases
            .iter()
            .find(|lease| lease.id == access.lease_id)
            .ok_or_else(|| "execution access references an unknown lease".to_string())?;
        if !access.allowed
            || !lease.operations.contains(&access.operation)
            || !access.path.starts_with(&lease.boundary)
        {
            return Err("execution result contains an out-of-lease access".to_string());
        }
        let observed = parse_time(&access.time)?;
        if observed < parse_time(&lease.ttl.start)? || observed > parse_time(&lease.ttl.end)? {
            return Err("execution access occurred outside lease ttl".to_string());
        }
    }
    Ok(())
}

struct LeaseGuard {
    repository_lease: ContextLease,
    artifact_lease: ContextLease,
    checkout_root: PathBuf,
    artifact_root: PathBuf,
}

impl LeaseGuard {
    fn new(
        repository_lease: ContextLease,
        artifact_lease: ContextLease,
        checkout_root: PathBuf,
        artifact_root: PathBuf,
    ) -> Self {
        Self {
            repository_lease,
            artifact_lease,
            checkout_root,
            artifact_root,
        }
    }

    fn log_repository_read(&self) -> Result<LeaseAccess, String> {
        self.ensure_active(&self.repository_lease, "read")?;
        Ok(LeaseAccess {
            allowed: true,
            lease_id: self.repository_lease.id.clone(),
            operation: "read".to_string(),
            path: format!("{}/", self.repository_lease.boundary),
            reason: "allowed by active repository lease".to_string(),
            time: now_rfc3339(),
        })
    }

    fn write_artifact(
        &self,
        path: &str,
        bytes: &[u8],
        log: &mut Vec<LeaseAccess>,
    ) -> Result<(), String> {
        self.ensure_active(&self.artifact_lease, "write")?;
        let relative = path
            .strip_prefix("artifacts/")
            .ok_or_else(|| "artifact path is outside the contracted namespace".to_string())?;
        let candidate = safe_join(&self.artifact_root, relative)?;
        if let Some(parent) = candidate.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::write(&candidate, bytes).map_err(|error| error.to_string())?;
        log.push(LeaseAccess {
            allowed: true,
            lease_id: self.artifact_lease.id.clone(),
            operation: "write".to_string(),
            path: format!("{}/{}", self.artifact_lease.boundary, relative),
            reason: "allowed by active artifact lease".to_string(),
            time: now_rfc3339(),
        });
        Ok(())
    }

    fn ensure_active(&self, lease: &ContextLease, operation: &str) -> Result<(), String> {
        if !lease.operations.iter().any(|allowed| allowed == operation) {
            return Err(format!(
                "ATP_LEASE_DENIED: lease does not permit {operation}"
            ));
        }
        let now = Utc::now();
        if now < parse_time(&lease.ttl.start)? || now > parse_time(&lease.ttl.end)? {
            return Err("ATP_LEASE_DENIED: lease is outside its ttl".to_string());
        }
        Ok(())
    }
}

fn scan_repository(guard: &LeaseGuard) -> Result<(Vec<String>, Vec<LeaseAccess>), String> {
    let mut files = Vec::new();
    let mut log = vec![guard.log_repository_read()?];
    for entry in WalkDir::new(&guard.checkout_root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(&guard.checkout_root)
            .map_err(|error| error.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        files.push(relative);
        if files.len() >= MAX_SCANNED_FILES {
            break;
        }
    }
    files.sort();

    for candidate in [
        "Cargo.toml",
        "package.json",
        "pyproject.toml",
        "go.mod",
        "SECURITY.md",
        ".github/dependabot.yml",
    ] {
        let path = safe_join(&guard.checkout_root, candidate)?;
        if path.is_file()
            && path.metadata().map_err(|error| error.to_string())?.len() <= MAX_INSPECTED_FILE_BYTES
        {
            let mut contents = String::new();
            fs::File::open(&path)
                .map_err(|error| error.to_string())?
                .take(MAX_INSPECTED_FILE_BYTES)
                .read_to_string(&mut contents)
                .map_err(|error| error.to_string())?;
            log.push(LeaseAccess {
                allowed: true,
                lease_id: guard.repository_lease.id.clone(),
                operation: "read".to_string(),
                path: format!("{}/{}", guard.repository_lease.boundary, candidate),
                reason: "allowed by active repository lease".to_string(),
                time: now_rfc3339(),
            });
        }
    }
    Ok((files, log))
}

fn build_artifacts(
    contract: &AuditContract,
    files: &[String],
) -> Result<Vec<(String, String, Vec<u8>)>, String> {
    let workflow_count = files
        .iter()
        .filter(|path| path.starts_with(".github/workflows/"))
        .count();
    let has_security = files
        .iter()
        .any(|path| path.eq_ignore_ascii_case("SECURITY.md"));
    let exposed_env = files
        .iter()
        .filter(|path| {
            let name = Path::new(path)
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default();
            name == ".env" || name.starts_with(".env.")
        })
        .cloned()
        .collect::<Vec<_>>();

    let mut findings = Vec::new();
    if !has_security {
        findings.push(serde_json::json!({
            "id": "CYPHES-SECURITY-POLICY-MISSING",
            "severity": "low",
            "title": "Repository does not publish SECURITY.md",
            "evidence": "No root SECURITY.md was present at the pinned commit."
        }));
    }
    if workflow_count == 0 {
        findings.push(serde_json::json!({
            "id": "CYPHES-CI-WORKFLOW-MISSING",
            "severity": "medium",
            "title": "No GitHub Actions workflow detected",
            "evidence": "No files were present under .github/workflows at the pinned commit."
        }));
    }
    for path in &exposed_env {
        findings.push(serde_json::json!({
            "id": "CYPHES-ENV-FILE-TRACKED",
            "severity": "high",
            "title": "Environment file is tracked",
            "evidence": path
        }));
    }

    let report = format!(
        "# CYPHES Repository Audit\n\n\
         - Repository: `{}`\n\
         - Commit: `{}`\n\
         - Files inventoried: `{}`\n\
         - GitHub Actions workflows: `{}`\n\
         - SECURITY.md: `{}`\n\
         - Findings: `{}`\n\n\
         This deterministic ATP-L1 worker does not execute repository code. It inventories the \
         pinned source snapshot and performs bounded repository-security posture checks.\n",
        contract.repository.full_name,
        contract.repository.commit_sha,
        files.len(),
        workflow_count,
        if has_security { "present" } else { "missing" },
        findings.len(),
    );
    let findings_json = serde_json::to_vec_pretty(&serde_json::json!({
        "schemaVersion": "0.1",
        "repository": contract.repository,
        "findings": findings,
    }))
    .map_err(|error| error.to_string())?;
    let sarif_results = findings
        .iter()
        .map(|finding| {
            serde_json::json!({
                "ruleId": finding["id"],
                "level": match finding["severity"].as_str().unwrap_or("low") {
                    "high" => "error",
                    "medium" => "warning",
                    _ => "note",
                },
                "message": {"text": finding["title"]}
            })
        })
        .collect::<Vec<_>>();
    let sarif = serde_json::to_vec_pretty(&serde_json::json!({
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {"driver": {"name": "CYPHES deterministic repository auditor", "version": "0.2.0-dev"}},
            "results": sarif_results
        }]
    }))
    .map_err(|error| error.to_string())?;
    let checks = serde_json::to_vec_pretty(&serde_json::json!({
        "schemaVersion": "0.1",
        "repositoryCommit": contract.repository.commit_sha,
        "codeExecuted": false,
        "networkUsedAfterFetch": false,
        "checks": [
            {"name": "file-inventory", "status": "passed", "observed": files.len()},
            {"name": "security-policy", "status": if has_security {"passed"} else {"finding"}},
            {"name": "github-actions", "status": if workflow_count > 0 {"passed"} else {"finding"}},
            {"name": "tracked-env-files", "status": if exposed_env.is_empty() {"passed"} else {"finding"}}
        ]
    }))
    .map_err(|error| error.to_string())?;

    let mut artifacts = vec![
        (
            "artifacts/audit-report.md".to_string(),
            "text/markdown".to_string(),
            report.into_bytes(),
        ),
        (
            "artifacts/findings.json".to_string(),
            "application/json".to_string(),
            findings_json,
        ),
        (
            "artifacts/results.sarif".to_string(),
            "application/sarif+json".to_string(),
            sarif,
        ),
        (
            "artifacts/checks.json".to_string(),
            "application/json".to_string(),
            checks,
        ),
    ];
    let manifest_entries = artifacts
        .iter()
        .map(|(path, media_type, bytes)| {
            serde_json::json!({
                "path": path,
                "mediaType": media_type,
                "sha256": sha256_bytes(bytes),
                "sizeBytes": bytes.len(),
            })
        })
        .collect::<Vec<_>>();
    let manifest = serde_json::to_vec_pretty(&serde_json::json!({
        "schemaVersion": "0.1",
        "transactionId": contract.transaction_id,
        "contractHash": crate::audit_profile::contract_hash(contract)?,
        "repository": contract.repository,
        "artifacts": manifest_entries,
    }))
    .map_err(|error| error.to_string())?;
    artifacts.push((
        "artifacts/manifest.json".to_string(),
        "application/json".to_string(),
        manifest,
    ));
    Ok(artifacts)
}

async fn download_checkout(
    repository: &RepositoryTarget,
    checkout_dir: &Path,
) -> Result<(), String> {
    let url = format!(
        "https://codeload.github.com/{}/tar.gz/{}",
        repository.full_name, repository.commit_sha
    );
    let response = reqwest::Client::builder()
        .user_agent("CYPHES/0.2.0-dev")
        .redirect(reqwest::redirect::Policy::limited(2))
        .build()
        .map_err(|error| error.to_string())?
        .get(url)
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "GitHub archive fetch failed with {}",
            response.status()
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_ARCHIVE_BYTES as u64)
    {
        return Err("repository archive exceeds the ATP-L1 worker limit".to_string());
    }
    let bytes = response.bytes().await.map_err(|error| error.to_string())?;
    if bytes.len() > MAX_ARCHIVE_BYTES {
        return Err("repository archive exceeds the ATP-L1 worker limit".to_string());
    }

    let decoder = GzDecoder::new(bytes.as_ref());
    let mut archive = Archive::new(decoder);
    for entry in archive.entries().map_err(|error| error.to_string())? {
        let mut entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path().map_err(|error| error.to_string())?;
        let relative = path.components().skip(1).collect::<PathBuf>();
        if relative.as_os_str().is_empty() {
            continue;
        }
        let destination = safe_join(checkout_dir, &relative)?;
        if entry.header().entry_type().is_symlink() || entry.header().entry_type().is_hard_link() {
            continue;
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        entry
            .unpack(&destination)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn safe_join(root: &Path, relative: impl AsRef<Path>) -> Result<PathBuf, String> {
    let relative = relative.as_ref();
    if relative
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("path escapes the lease boundary".to_string());
    }
    Ok(root.join(relative))
}

fn make_read_only(root: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for entry in WalkDir::new(root)
            .contents_first(true)
            .into_iter()
            .filter_map(Result::ok)
        {
            let mode = if entry.file_type().is_dir() {
                0o500
            } else {
                0o400
            };
            fs::set_permissions(entry.path(), fs::Permissions::from_mode(mode))
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn make_writable(root: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for entry in WalkDir::new(root).into_iter().filter_map(Result::ok) {
            let mode = if entry.file_type().is_dir() {
                0o700
            } else {
                0o600
            };
            fs::set_permissions(entry.path(), fs::Permissions::from_mode(mode))
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn lease_signing_payload(lease: &ContextLease) -> LeaseSigningPayload<'_> {
    LeaseSigningPayload {
        id: &lease.id,
        issuer: &lease.issuer,
        resource_ref: &lease.resource_ref,
        operations: &lease.operations,
        purpose: &lease.purpose,
        boundary: &lease.boundary,
        retention: &lease.retention,
        audit: lease.audit,
        nonce: &lease.nonce,
        ttl: &lease.ttl,
    }
}

fn repository_resource(repository: &RepositoryTarget) -> String {
    format!("github:{}@{}", repository.full_name, repository.commit_sha)
}

fn parse_time(value: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| "lease time is not RFC3339".to_string())
}

fn canonical_hash<T: Serialize>(value: &T) -> Result<String, String> {
    let bytes = serde_jcs::to_vec(value).map_err(|error| error.to_string())?;
    Ok(sha256_bytes(&bytes))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!(
        "sha256:{}",
        digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifact_paths_cannot_escape_the_write_boundary() {
        let root = PathBuf::from("/tmp/cyphes-artifacts");
        assert!(safe_join(&root, "audit-report.md").is_ok());
        assert!(safe_join(&root, "../identity.key").is_err());
        assert!(safe_join(&root, "/etc/passwd").is_err());
    }

    #[test]
    fn signed_execution_result_detects_artifact_tampering() {
        let keypair = identity::Keypair::generate_ed25519();
        let bytes = b"verified artifact".to_vec();
        let public_key = raw_ed25519_public_key(&keypair.public()).unwrap();
        let mut result = SignedExecutionResult {
            transaction_id: "tx-1".to_string(),
            worker_agent_id: agent_id(&keypair.public()),
            contract_hash:
                "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .to_string(),
            repository: RepositoryTarget {
                full_name: "CYPHES-ATP/Node".to_string(),
                url: "https://github.com/CYPHES-ATP/Node".to_string(),
                commit_sha: "1111111111111111111111111111111111111111".to_string(),
            },
            lease_ids: vec!["lease-1".to_string()],
            access_log: vec![],
            artifacts: vec![ExecutionArtifact {
                path: "artifacts/audit-report.md".to_string(),
                media_type: "text/markdown".to_string(),
                sha256: sha256_bytes(&bytes),
                size_bytes: bytes.len() as u64,
                content_base64: URL_SAFE_NO_PAD.encode(&bytes),
            }],
            created_at: now_rfc3339(),
            public_key_base64_url: URL_SAFE_NO_PAD.encode(public_key),
            result_hash: String::new(),
            signature: String::new(),
        };
        result.result_hash = canonical_hash(&result.signing_payload()).unwrap();
        result.signature = sign_canonical(&keypair, &result.signing_payload()).unwrap();
        result.verify().unwrap();
        result.artifacts[0].content_base64 = URL_SAFE_NO_PAD.encode(b"tampered");
        assert!(result.verify().is_err());
    }
}
