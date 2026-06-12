use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, SecondsFormat, Utc};
use libp2p::identity;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const ATP_VERSION: &str = "0.3";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum AtpVerb {
    Discover,
    Negotiate,
    Route,
    Execute,
    Settle,
    Attest,
    Reject,
    Revoke,
}

impl AtpVerb {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Discover => "DISCOVER",
            Self::Negotiate => "NEGOTIATE",
            Self::Route => "ROUTE",
            Self::Execute => "EXECUTE",
            Self::Settle => "SETTLE",
            Self::Attest => "ATTEST",
            Self::Reject => "REJECT",
            Self::Revoke => "REVOKE",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtpProof {
    #[serde(rename = "type")]
    pub proof_type: String,
    pub kid: String,
    pub public_key: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtpEnvelope {
    pub atp: String,
    pub verb: AtpVerb,
    pub transaction_id: String,
    pub idempotency_key: String,
    pub issuer: String,
    pub audience: Option<String>,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub nonce: String,
    pub prev: Option<String>,
    pub body: Value,
    pub proofs: Vec<AtpProof>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtpAck {
    pub accepted: bool,
    pub duplicate: bool,
    pub event_hash: String,
    pub transaction_id: String,
    pub state: Option<String>,
    pub receiver_agent_id: String,
    pub committed_at: String,
    pub reason_code: Option<String>,
    pub reason: Option<String>,
}

#[derive(Serialize)]
struct SigningPayload<'a> {
    atp: &'a str,
    verb: AtpVerb,
    transaction_id: &'a str,
    idempotency_key: &'a str,
    issuer: &'a str,
    audience: &'a Option<String>,
    created_at: &'a str,
    expires_at: &'a Option<String>,
    nonce: &'a str,
    prev: &'a Option<String>,
    body: &'a Value,
}

pub fn agent_id(public_key: &identity::PublicKey) -> String {
    format!("urn:libp2p:{}", public_key.to_peer_id())
}

pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub fn create_signed_envelope(
    keypair: &identity::Keypair,
    verb: AtpVerb,
    transaction_id: String,
    audience: Option<String>,
    prev: Option<String>,
    body: Value,
) -> Result<AtpEnvelope, String> {
    let public_key = keypair.public();
    let issuer = agent_id(&public_key);
    let mut envelope = AtpEnvelope {
        atp: ATP_VERSION.to_string(),
        verb,
        transaction_id,
        idempotency_key: Uuid::new_v4().to_string(),
        issuer: issuer.clone(),
        audience,
        created_at: now_rfc3339(),
        expires_at: None,
        nonce: Uuid::new_v4().to_string(),
        prev,
        body,
        proofs: Vec::new(),
    };

    let signing_bytes = signing_bytes(&envelope)?;
    let signature = keypair
        .sign(&signing_bytes)
        .map_err(|error| error.to_string())?;

    envelope.proofs.push(AtpProof {
        proof_type: "Ed25519Signature2020".to_string(),
        kid: format!("{issuer}#identity"),
        public_key: URL_SAFE_NO_PAD.encode(public_key.encode_protobuf()),
        signature: URL_SAFE_NO_PAD.encode(signature),
    });

    Ok(envelope)
}

pub fn verify_envelope(envelope: &AtpEnvelope) -> Result<(), String> {
    if envelope.atp != ATP_VERSION {
        return Err(format!("Unsupported ATP version {}", envelope.atp));
    }
    if envelope.proofs.len() != 1 {
        return Err("ATP envelope must contain exactly one identity proof".to_string());
    }

    let proof = &envelope.proofs[0];
    let public_key_bytes = URL_SAFE_NO_PAD
        .decode(&proof.public_key)
        .map_err(|_| "ATP proof public key is not valid base64url".to_string())?;
    let public_key = identity::PublicKey::try_decode_protobuf(&public_key_bytes)
        .map_err(|_| "ATP proof public key is not a supported libp2p identity".to_string())?;
    let expected_issuer = agent_id(&public_key);
    if envelope.issuer != expected_issuer {
        return Err("ATP proof key does not match the declared issuer".to_string());
    }
    if proof.kid != format!("{}#identity", envelope.issuer) {
        return Err("ATP proof key identifier does not match the issuer".to_string());
    }

    let signature = URL_SAFE_NO_PAD
        .decode(&proof.signature)
        .map_err(|_| "ATP proof signature is not valid base64url".to_string())?;
    if !public_key.verify(&signing_bytes(envelope)?, &signature) {
        return Err("ATP envelope signature verification failed".to_string());
    }

    if let Some(expires_at) = &envelope.expires_at {
        let expiry = DateTime::parse_from_rfc3339(expires_at)
            .map_err(|_| "ATP expiry is not RFC3339".to_string())?;
        if expiry.with_timezone(&Utc) <= Utc::now() {
            return Err("ATP envelope has expired".to_string());
        }
    }

    Ok(())
}

pub fn event_hash(envelope: &AtpEnvelope) -> Result<String, String> {
    let canonical_body = serde_jcs::to_vec(&envelope.body).map_err(|error| error.to_string())?;
    let body_hash = hex_sha256(&canonical_body);
    let preimage = format!(
        "{}{}{}{}{}{}",
        envelope.prev.as_deref().unwrap_or_default(),
        envelope.verb.as_str(),
        envelope.issuer,
        body_hash,
        envelope.created_at,
        envelope.nonce
    );
    Ok(format!("sha256:{}", hex_sha256(preimage.as_bytes())))
}

pub fn transition(current: Option<&str>, verb: AtpVerb) -> Result<&'static str, String> {
    match (current, verb) {
        (None, AtpVerb::Discover) => Ok("discovered"),
        (Some("discovered"), AtpVerb::Negotiate) => Ok("negotiating"),
        (Some("negotiating"), AtpVerb::Negotiate) => Ok("negotiated"),
        (Some("negotiated"), AtpVerb::Route) => Ok("routed"),
        (Some("routed"), AtpVerb::Execute) => Ok("executing"),
        (Some("executing"), AtpVerb::Settle) => Ok("settled"),
        (Some("settled"), AtpVerb::Attest) => Ok("attested"),
        (Some(_), AtpVerb::Reject) => Ok("rejected"),
        (Some(_), AtpVerb::Revoke) => Ok("revoked"),
        (state, verb) => Err(format!(
            "ATP verb {} is not valid from state {}",
            verb.as_str(),
            state.unwrap_or("new")
        )),
    }
}

fn signing_bytes(envelope: &AtpEnvelope) -> Result<Vec<u8>, String> {
    serde_jcs::to_vec(&SigningPayload {
        atp: &envelope.atp,
        verb: envelope.verb,
        transaction_id: &envelope.transaction_id,
        idempotency_key: &envelope.idempotency_key,
        issuer: &envelope.issuer,
        audience: &envelope.audience,
        created_at: &envelope.created_at,
        expires_at: &envelope.expires_at,
        nonce: &envelope.nonce,
        prev: &envelope.prev,
        body: &envelope.body,
    })
    .map_err(|error| error.to_string())
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_envelope_verifies_and_detects_tampering() {
        let keypair = identity::Keypair::generate_ed25519();
        let mut envelope = create_signed_envelope(
            &keypair,
            AtpVerb::Discover,
            "audit-1".to_string(),
            None,
            None,
            serde_json::json!({"action": "announce", "amount": 100}),
        )
        .unwrap();

        verify_envelope(&envelope).unwrap();
        envelope.body["amount"] = serde_json::json!(101);
        assert!(verify_envelope(&envelope).is_err());
    }

    #[test]
    fn state_machine_rejects_skipped_steps() {
        assert_eq!(transition(None, AtpVerb::Discover).unwrap(), "discovered");
        assert_eq!(
            transition(Some("discovered"), AtpVerb::Negotiate).unwrap(),
            "negotiating"
        );
        assert!(transition(Some("discovered"), AtpVerb::Execute).is_err());
    }
}
