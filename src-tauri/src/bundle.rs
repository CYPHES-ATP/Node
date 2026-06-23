use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use libp2p::identity;
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::{
    atp::{event_hash, raw_ed25519_public_key, AtpEnvelope},
    audit_labor::{final_report_markdown, sha256_ref},
    store::{data_dir, AtpStore},
};

pub fn export_receipt_bundle(store: &AtpStore, transaction_id: &str) -> Result<PathBuf, String> {
    export_receipt_bundle_to(store, transaction_id, &data_dir()?.join("receipts"))
}

pub fn export_campaign_report_bundle(
    store: &AtpStore,
    campaign_id: &str,
) -> Result<PathBuf, String> {
    export_campaign_report_bundle_to(store, campaign_id, &data_dir()?.join("reports"))
}

pub fn export_campaign_report_bundle_to(
    store: &AtpStore,
    campaign_id: &str,
    report_root: &Path,
) -> Result<PathBuf, String> {
    let snapshot = store.campaign_report_snapshot(campaign_id)?;
    let bundle_dir = report_root.join(campaign_id);
    let staging_dir = report_root.join(format!(".{campaign_id}.staging"));
    if staging_dir.exists() {
        fs::remove_dir_all(&staging_dir).map_err(|error| error.to_string())?;
    }
    fs::create_dir_all(staging_dir.join("receipts")).map_err(|error| error.to_string())?;

    let accepted_ids = snapshot
        .verifications
        .iter()
        .filter(|verification| verification.decision == "accepted")
        .map(|verification| verification.target_contribution_id.as_str())
        .collect::<std::collections::HashSet<_>>();
    let accepted_findings = snapshot
        .contributions
        .iter()
        .filter(|contribution| accepted_ids.contains(contribution.contribution_id.as_str()))
        .flat_map(|contribution| contribution.findings.iter())
        .filter(|finding| finding.reportable)
        .collect::<Vec<_>>();

    let report = final_report_markdown(&snapshot).into_bytes();
    let findings =
        serde_json::to_vec_pretty(&accepted_findings).map_err(|error| error.to_string())?;
    let contributions =
        serde_json::to_vec_pretty(&snapshot.contributions).map_err(|error| error.to_string())?;
    let claims = serde_json::to_vec_pretty(&snapshot.claims).map_err(|error| error.to_string())?;
    let verifications =
        serde_json::to_vec_pretty(&snapshot.verifications).map_err(|error| error.to_string())?;
    let credits =
        serde_json::to_vec_pretty(&snapshot.credits).map_err(|error| error.to_string())?;
    let receipts_readme = b"# Receipts\n\nThis directory is reserved for portable ATP receipt bundles and signed contribution receipts. This local export references contribution receipt hashes in `contributions.json` and `credits.json`; it does not invent missing external receipts.\n".to_vec();

    let files = vec![
        ("report.md", "text/markdown", report),
        ("findings.json", "application/json", findings),
        ("claims.json", "application/json", claims),
        ("contributions.json", "application/json", contributions),
        ("verifications.json", "application/json", verifications),
        ("credits.json", "application/json", credits),
        ("receipts/README.md", "text/markdown", receipts_readme),
    ];

    let mut manifest_entries = Vec::new();
    for (path, media_type, bytes) in &files {
        let destination = safe_join(&staging_dir, path)?;
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::write(&destination, bytes).map_err(|error| error.to_string())?;
        manifest_entries.push(json!({
            "path": path,
            "mediaType": media_type,
            "sha256": sha256_ref(bytes),
            "sizeBytes": bytes.len(),
        }));
    }
    write_json(
        &staging_dir.join("manifest.json"),
        &json!({
            "profile": "cyphes.final-audit-report/0.1",
            "campaignId": campaign_id,
            "protocolName": snapshot.campaign.protocol_name,
            "repository": snapshot.campaign.repository,
            "acceptedContributionCount": accepted_ids.len(),
            "creditAllocationCount": snapshot.credits.len(),
            "files": manifest_entries,
            "generatedBy": format!("CYPHES/{}", env!("CARGO_PKG_VERSION")),
        }),
    )?;

    if bundle_dir.exists() {
        fs::remove_dir_all(&bundle_dir).map_err(|error| error.to_string())?;
    }
    fs::rename(&staging_dir, &bundle_dir).map_err(|error| error.to_string())?;
    Ok(bundle_dir)
}

pub fn export_receipt_bundle_to(
    store: &AtpStore,
    transaction_id: &str,
    receipt_root: &Path,
) -> Result<PathBuf, String> {
    let envelopes = store.transaction_envelopes(transaction_id)?;
    if envelopes.len() != 6 {
        return Err(
            "receipt bundle requires the complete six-envelope ATP transaction".to_string(),
        );
    }
    let contract = store.get_contract(transaction_id)?;
    let leases = store.get_leases(transaction_id)?;
    let result = store.get_execution_result(transaction_id)?;
    let receipt = store.get_receipt(transaction_id)?;

    let bundle_dir = receipt_root.join(transaction_id);
    let staging_dir = receipt_root.join(format!(".{transaction_id}.staging"));
    if staging_dir.exists() {
        fs::remove_dir_all(&staging_dir).map_err(|error| error.to_string())?;
    }
    fs::create_dir_all(staging_dir.join("artifacts")).map_err(|error| error.to_string())?;

    write_json(
        &staging_dir.join("public-keys.json"),
        &public_key_records(&envelopes)?,
    )?;
    write_jsonl(&staging_dir.join("envelopes.jsonl"), &envelopes)?;
    let transcript = envelopes
        .iter()
        .map(transcript_row)
        .collect::<Result<Vec<_>, _>>()?;
    write_jsonl(&staging_dir.join("transcript.jsonl"), &transcript)?;
    write_json(&staging_dir.join("contract.json"), &contract)?;
    write_json(&staging_dir.join("leases.json"), &leases)?;
    write_jsonl(
        &staging_dir.join("lease-access-log.jsonl"),
        &result.access_log,
    )?;
    write_json(&staging_dir.join("receipt.json"), &receipt)?;

    for artifact in &result.artifacts {
        let relative = artifact.path.strip_prefix("artifacts/").ok_or_else(|| {
            format!(
                "artifact path is outside bundle namespace: {}",
                artifact.path
            )
        })?;
        let destination = safe_join(&staging_dir.join("artifacts"), relative)?;
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::write(destination, artifact.bytes()?).map_err(|error| error.to_string())?;
    }

    write_json(
        &staging_dir.join("bundle.json"),
        &json!({
            "format": "artifact-two/0.1",
            "transactionId": transaction_id,
            "receiptHash": receipt.receipt_hash,
            "eventRoot": receipt.event_root,
            "artifactCount": result.artifacts.len(),
            "generatedBy": format!("CYPHES/{}", env!("CARGO_PKG_VERSION")),
        }),
    )?;

    if bundle_dir.exists() {
        fs::remove_dir_all(&bundle_dir).map_err(|error| error.to_string())?;
    }
    fs::rename(&staging_dir, &bundle_dir).map_err(|error| error.to_string())?;
    store.set_bundle_path(transaction_id, &bundle_dir.to_string_lossy())?;
    Ok(bundle_dir)
}

fn public_key_records(envelopes: &[AtpEnvelope]) -> Result<Value, String> {
    let mut records = BTreeMap::new();
    for envelope in envelopes {
        let proof = envelope
            .proofs
            .first()
            .ok_or_else(|| "envelope proof is missing".to_string())?;
        let protobuf = URL_SAFE_NO_PAD
            .decode(&proof.public_key)
            .map_err(|_| "envelope proof public key is not valid base64url".to_string())?;
        let public_key = identity::PublicKey::try_decode_protobuf(&protobuf)
            .map_err(|_| "envelope proof public key is not a libp2p identity".to_string())?;
        records.insert(
            envelope.issuer.clone(),
            json!({
                "alg": "Ed25519",
                "kid": proof.kid,
                "publicKeyBase64Url": URL_SAFE_NO_PAD.encode(raw_ed25519_public_key(&public_key)?),
            }),
        );
    }
    serde_json::to_value(records).map_err(|error| error.to_string())
}

fn transcript_row(envelope: &AtpEnvelope) -> Result<Value, String> {
    let body_hash = canonical_hash(&envelope.body)?;
    Ok(json!({
        "verb": envelope.verb.as_str(),
        "actor": envelope.issuer,
        "prev": envelope.prev,
        "bodyHash": body_hash,
        "time": envelope.created_at,
        "nonce": envelope.nonce,
        "sig": envelope.proofs.first().map(|proof| proof.signature.clone()).unwrap_or_default(),
        "eventHash": event_hash(envelope)?,
    }))
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

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    fs::write(path, bytes).map_err(|error| error.to_string())
}

fn write_jsonl<T: Serialize>(path: &Path, values: &[T]) -> Result<(), String> {
    let mut file = fs::File::create(path).map_err(|error| error.to_string())?;
    for value in values {
        serde_json::to_writer(&mut file, value).map_err(|error| error.to_string())?;
        file.write_all(b"\n").map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn safe_join(root: &Path, relative: impl AsRef<Path>) -> Result<PathBuf, String> {
    let relative = relative.as_ref();
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("bundle artifact path escapes its root".to_string());
    }
    Ok(root.join(relative))
}
