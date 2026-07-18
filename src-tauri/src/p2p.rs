use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    time::Duration,
};

use futures::StreamExt;
use libp2p::{
    core::transport::ListenerId,
    dcutr, identify, identity, mdns, noise, ping, relay, rendezvous, request_response,
    request_response::{OutboundRequestId, ProtocolSupport},
    swarm::{NetworkBehaviour, StreamProtocol, SwarmEvent},
    tcp, yamux, Multiaddr, PeerId, SwarmBuilder,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::{select, sync::mpsc, time::MissedTickBehavior};

use crate::{
    atp::{agent_id, create_signed_envelope, AtpAck, AtpEnvelope, AtpVerb},
    audit_labor::{
        signed_autonomous_finality_verification, AuditWorkUnitClaim, ContributionArtifact,
        CreditAllocation, NodeContribution, ProtocolAuditCampaign, VerificationEvidence,
        VerificationResult, WORK_UNIT_CLAIM_TTL_MS,
    },
    bundle::export_receipt_bundle,
    state::{P2pState, PeerInfo},
    store::{
        now_millis, rejection_ack, AtpStore, AuditEventBody, LaborObjectPreflight,
        ATP_STORE_TESTNET_ID,
    },
    worker::SignedExecutionResult,
};

pub const ATP_PROTOCOL: &str = "/cyphes/atp/0.15.1";
pub const DEFAULT_RENDEZVOUS_NAMESPACE: &str = "cyphes.repository-audit.v0.15.1";
const LABOR_WIRE_COMPAT_APP_VERSION: &str = "0.15.1";
const DEFAULT_NETWORK_CONFIG_URL: &str =
    "https://raw.githubusercontent.com/CYPHES-ATP/Node/main/network/bootstrap.json";
const EMBEDDED_NETWORK_CONFIG_JSON: &str = include_str!("../../network/bootstrap.json");
const MAX_WIRE_REQUEST_BYTES: u64 = 32 * 1024 * 1024;
// Response codec read limit. Kept symmetric with the request limit so a node
// that fell behind can pull a large catch-up bundle instead of truncating the
// response at the old 2 MiB cap (which surfaced as JSON EOF at column 2097152).
const MAX_WIRE_RESPONSE_BYTES: u64 = 32 * 1024 * 1024;
// Soft budget for the contributions+verifications a single labor bundle SENDS,
// so the serialized response stays readable by peers still on the 2 MiB read
// limit. Dropped objects are re-requested on the next sparse-inventory round,
// so this only paginates the sync.
const MAX_LABOR_BUNDLE_BYTES: usize = 1_200_000;
const INFRASTRUCTURE_RETRY_INTERVAL: Duration = Duration::from_secs(15);
const INFRASTRUCTURE_ACTIVITY_STALE_AFTER: Duration = Duration::from_secs(90);
const RENDEZVOUS_DISCOVERY_INTERVAL: Duration = Duration::from_secs(20);
const RENDEZVOUS_REGISTRATION_INTERVAL: Duration = Duration::from_secs(60 * 60);
const PEER_IDLE_CONNECTION_TIMEOUT: Duration = Duration::from_secs(60 * 60);
const LABOR_NETWORK_SYNC_INTERVAL: Duration = Duration::from_secs(12);
const RELAY_RESERVATION_RETRY_AFTER: Duration = Duration::from_secs(45);
const LABOR_AUTO_VERIFY_LIMIT: usize = 8;
const LABOR_AUTO_VERIFY_SCAN_LIMIT: usize = 512;
const LABOR_INVENTORY_LIMIT: usize = 512;
const MAX_BROADCAST_PEERS_PER_TICK: usize = 32;
const MAX_DISCOVERY_DIAL_CANDIDATES: usize = 2;
const MAX_DISCOVERY_PEER_DIALS_PER_TICK: usize = 3;
const MAX_OUTBOUND_REQUESTS_PER_PEER: usize = 8;
// Bulk sync may use only half of the per-peer window so receipt and
// verification settlement always retains request capacity.
const MAX_BULK_OUTBOUND_REQUESTS_PER_PEER: usize = 4;
const MAX_OUTBOUND_BULK_BACKLOG: usize = 16;
const PEER_FAILURE_BASE_COOLDOWN_MS: u64 = 30_000;
const PEER_FAILURE_MAX_COOLDOWN_MS: u64 = 5 * 60_000;
const STALE_RECEIPT_REPAIR_AFTER: Duration = Duration::from_secs(2 * 60);
const STALE_RECEIPT_REPAIR_INTERVAL: Duration = Duration::from_secs(60);
const STALE_RECEIPT_REPAIR_LIMIT: usize = 32;
const MAX_OUTBOUND_REPAIR_BACKLOG: usize = 32;
const VERIFIER_LIVENESS_STALE_AFTER: Duration = Duration::from_secs(5 * 60);
const VERIFIER_LIVENESS_DISCOVERY_INTERVAL: Duration = Duration::from_secs(30);
const LABOR_CAPABILITY_INVENTORY_V2: &str = "audit_labor_inventory_v2";
const LABOR_CAPABILITY_SPARSE_INVENTORY_V3: &str = "sparse_inventory_v3";
const LABOR_CAPABILITY_HISTORICAL_CLAIMS: &str = "historical_claim_evidence_v1";
const LABOR_CAPABILITY_VERIFY_AFTER_BUNDLE: &str = "verify_after_bundle_v1";
const LABOR_CAPABILITY_TELEMETRY: &str = "audit_labor_telemetry_v1";
const LABOR_CAPABILITY_VERIFIER_PULL: &str = "verifier_pull_v1";

#[derive(Debug, Clone)]
struct InfrastructureTarget {
    peer_id: PeerId,
    address: Multiaddr,
}

#[derive(Debug, Default)]
struct RelayReservationState {
    listener_id: Option<ListenerId>,
    requested_at_ms: Option<u64>,
    confirmed: bool,
}

impl RelayReservationState {
    fn has_pending_request(&self) -> bool {
        self.listener_id.is_some() && !self.confirmed
    }

    fn is_pending_stale(&self, now_ms: u64) -> bool {
        self.has_pending_request()
            && self.requested_at_ms.is_some_and(|requested_at_ms| {
                now_ms.saturating_sub(requested_at_ms)
                    >= RELAY_RESERVATION_RETRY_AFTER.as_millis() as u64
            })
    }

    fn reset(&mut self) {
        self.listener_id = None;
        self.requested_at_ms = None;
        self.confirmed = false;
    }
}

#[derive(Debug, Clone)]
struct NetworkBootstrap {
    relay: Option<InfrastructureTarget>,
    rendezvous: Option<InfrastructureTarget>,
    namespace: rendezvous::Namespace,
    source: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublishedNetworkConfig {
    relay_addr: Option<String>,
    rendezvous_addr: Option<String>,
    rendezvous_namespace: Option<String>,
}

#[derive(Debug, Clone)]
pub enum SwarmCommand {
    SendEnvelope(AtpEnvelope),
    SendCampaign(ProtocolAuditCampaign),
    SendWorkUnitClaim {
        claim: AuditWorkUnitClaim,
        audience: String,
    },
    SendExecutionResult {
        result: SignedExecutionResult,
        audience: String,
    },
    SendContribution {
        contribution: NodeContribution,
        audience: String,
    },
    SendVerificationResult {
        verification: VerificationResult,
        allocations: Vec<CreditAllocation>,
        audience: String,
    },
    Dial(Multiaddr),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
enum WireRequest {
    Envelope(AtpEnvelope),
    Campaign(ProtocolAuditCampaign),
    WorkUnitClaim(AuditWorkUnitClaim),
    ExecutionResult(SignedExecutionResult),
    Contribution(NodeContribution),
    VerificationResult {
        verification: VerificationResult,
        allocations: Vec<CreditAllocation>,
    },
    LaborInventory(LaborInventory),
    LaborObjectRequest(LaborObjectRequest),
    LaborObjectBundle(LaborObjectBundle),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
enum WireResponse {
    Envelope(AtpAck),
    Campaign {
        accepted: bool,
        campaign_id: String,
        reason: Option<String>,
    },
    WorkUnitClaim {
        accepted: bool,
        campaign_id: String,
        work_unit_id: String,
        claim_id: String,
        reason: Option<String>,
    },
    ExecutionResult {
        accepted: bool,
        transaction_id: String,
        result_hash: String,
        reason: Option<String>,
    },
    Contribution {
        accepted: bool,
        campaign_id: String,
        contribution_id: String,
        receipt_hash: String,
        reason: Option<String>,
    },
    VerificationResult {
        accepted: bool,
        campaign_id: String,
        verification_id: String,
        credit_total: u32,
        reason: Option<String>,
    },
    LaborInventory(LaborInventoryResponse),
    LaborObjectBundle(LaborObjectBundle),
    LaborObjectBundleAck(LaborObjectBundleResponse),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LaborInventory {
    testnet_id: String,
    #[serde(default)]
    app_version: String,
    #[serde(default)]
    capabilities: Vec<String>,
    campaigns: Vec<String>,
    claims: Vec<String>,
    contributions: Vec<String>,
    verifications: Vec<String>,
    needs_verifier: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LaborInventoryResponse {
    accepted: bool,
    testnet_id: String,
    #[serde(default)]
    app_version: String,
    #[serde(default)]
    capabilities: Vec<String>,
    missing_campaigns: Vec<String>,
    missing_claims: Vec<String>,
    missing_contributions: Vec<String>,
    missing_verifications: Vec<String>,
    reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LaborObjectRequest {
    testnet_id: String,
    #[serde(default)]
    app_version: String,
    #[serde(default)]
    capabilities: Vec<String>,
    campaign_ids: Vec<String>,
    claim_ids: Vec<String>,
    contribution_ids: Vec<String>,
    verification_ids: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LaborObjectBundle {
    testnet_id: String,
    #[serde(default)]
    app_version: String,
    #[serde(default)]
    capabilities: Vec<String>,
    campaigns: Vec<ProtocolAuditCampaign>,
    claims: Vec<AuditWorkUnitClaim>,
    contributions: Vec<NodeContribution>,
    verifications: Vec<VerificationBundleWire>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VerificationBundleWire {
    verification: VerificationResult,
    allocations: Vec<CreditAllocation>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LaborObjectBundleResponse {
    accepted: bool,
    testnet_id: String,
    #[serde(default)]
    app_version: String,
    #[serde(default)]
    capabilities: Vec<String>,
    campaigns: usize,
    claims: usize,
    contributions: usize,
    verifications: usize,
    queued: usize,
    #[serde(default)]
    skipped: usize,
    reason: Option<String>,
}

#[derive(Debug)]
enum PendingOutbound {
    Envelope {
        peer_id: PeerId,
        event_hash: String,
    },
    Campaign {
        peer_id: PeerId,
        campaign_id: String,
        silent: bool,
    },
    WorkUnitClaim {
        peer_id: PeerId,
        campaign_id: String,
        work_unit_id: String,
        claim_id: String,
        silent: bool,
    },
    ExecutionResult {
        peer_id: PeerId,
        transaction_id: String,
        result_hash: String,
    },
    Contribution {
        peer_id: PeerId,
        campaign_id: String,
        contribution_id: String,
        receipt_hash: String,
        silent: bool,
    },
    VerificationResult {
        peer_id: PeerId,
        campaign_id: String,
        verification_id: String,
        credit_total: u32,
        silent: bool,
    },
    LaborInventory {
        peer_id: PeerId,
    },
    LaborObjectBundle {
        peer_id: PeerId,
        silent: bool,
    },
}

impl PendingOutbound {
    fn peer_id(&self) -> &PeerId {
        match self {
            Self::Envelope { peer_id, .. }
            | Self::Campaign { peer_id, .. }
            | Self::WorkUnitClaim { peer_id, .. }
            | Self::ExecutionResult { peer_id, .. }
            | Self::Contribution { peer_id, .. }
            | Self::VerificationResult { peer_id, .. }
            | Self::LaborInventory { peer_id }
            | Self::LaborObjectBundle { peer_id, .. } => peer_id,
        }
    }

    fn is_silent(&self) -> bool {
        match self {
            Self::Campaign { silent, .. }
            | Self::WorkUnitClaim { silent, .. }
            | Self::Contribution { silent, .. }
            | Self::VerificationResult { silent, .. }
            | Self::LaborObjectBundle { silent, .. } => *silent,
            _ => false,
        }
    }
}

fn send_wire_request_to_peer(
    swarm: &mut libp2p::Swarm<AgentBehaviour>,
    state: &P2pState,
    outbound: &mut HashMap<OutboundRequestId, PendingOutbound>,
    request: WireRequest,
    pending: PendingOutbound,
) -> bool {
    let peer_id = pending.peer_id().clone();
    if !peer_send_allowed(state, outbound, &peer_id) {
        return false;
    }
    let request_id = swarm
        .behaviour_mut()
        .request_response
        .send_request(&peer_id, request);
    outbound.insert(request_id, pending);
    true
}

fn peer_send_allowed(
    state: &P2pState,
    outbound: &HashMap<OutboundRequestId, PendingOutbound>,
    peer_id: &PeerId,
) -> bool {
    if !peer_dial_allowed(state, *peer_id) {
        return false;
    }
    outbound
        .values()
        .filter(|pending| pending.peer_id() == peer_id)
        .count()
        < MAX_OUTBOUND_REQUESTS_PER_PEER
}

fn peer_dial_allowed(state: &P2pState, peer_id: PeerId) -> bool {
    let now = now_millis();
    state
        .inner
        .lock()
        .map(|inner| {
            inner
                .peers
                .get(&peer_id.to_string())
                .is_none_or(|peer| peer.cooldown_until <= now)
        })
        .unwrap_or(true)
}

fn labor_wire_capabilities() -> Vec<String> {
    [
        LABOR_CAPABILITY_INVENTORY_V2,
        LABOR_CAPABILITY_SPARSE_INVENTORY_V3,
        LABOR_CAPABILITY_HISTORICAL_CLAIMS,
        LABOR_CAPABILITY_VERIFY_AFTER_BUNDLE,
        LABOR_CAPABILITY_TELEMETRY,
        LABOR_CAPABILITY_VERIFIER_PULL,
    ]
    .into_iter()
    .map(ToString::to_string)
    .collect()
}

fn labor_wire_app_version() -> String {
    LABOR_WIRE_COMPAT_APP_VERSION.to_string()
}

fn is_labor_wire_compatible_app_version(app_version: &str) -> bool {
    app_version == LABOR_WIRE_COMPAT_APP_VERSION || app_version == env!("CARGO_PKG_VERSION")
}

fn has_labor_capability(capabilities: &[String], capability: &str) -> bool {
    capabilities.iter().any(|candidate| candidate == capability)
}

#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "AgentBehaviourEvent")]
struct AgentBehaviour {
    request_response: request_response::Behaviour<
        request_response::json::codec::Codec<WireRequest, WireResponse>,
    >,
    mdns: mdns::tokio::Behaviour,
    identify: identify::Behaviour,
    ping: ping::Behaviour,
    relay: relay::client::Behaviour,
    rendezvous: rendezvous::client::Behaviour,
    dcutr: dcutr::Behaviour,
}

#[allow(dead_code)]
#[derive(Debug)]
enum AgentBehaviourEvent {
    RequestResponse(request_response::Event<WireRequest, WireResponse>),
    Mdns(mdns::Event),
    Identify(identify::Event),
    Ping(ping::Event),
    Relay(relay::client::Event),
    Rendezvous(rendezvous::client::Event),
    Dcutr(dcutr::Event),
}

impl From<request_response::Event<WireRequest, WireResponse>> for AgentBehaviourEvent {
    fn from(event: request_response::Event<WireRequest, WireResponse>) -> Self {
        Self::RequestResponse(event)
    }
}

impl From<mdns::Event> for AgentBehaviourEvent {
    fn from(event: mdns::Event) -> Self {
        Self::Mdns(event)
    }
}

impl From<identify::Event> for AgentBehaviourEvent {
    fn from(event: identify::Event) -> Self {
        Self::Identify(event)
    }
}

impl From<ping::Event> for AgentBehaviourEvent {
    fn from(event: ping::Event) -> Self {
        Self::Ping(event)
    }
}

impl From<relay::client::Event> for AgentBehaviourEvent {
    fn from(event: relay::client::Event) -> Self {
        Self::Relay(event)
    }
}

impl From<rendezvous::client::Event> for AgentBehaviourEvent {
    fn from(event: rendezvous::client::Event) -> Self {
        Self::Rendezvous(event)
    }
}

impl From<dcutr::Event> for AgentBehaviourEvent {
    fn from(event: dcutr::Event) -> Self {
        Self::Dcutr(event)
    }
}

pub fn load_or_create_identity() -> Result<identity::Keypair, String> {
    let identity_path = identity_path()?;

    if identity_path.exists() {
        secure_identity_file(&identity_path)?;
        let bytes = fs::read(&identity_path).map_err(|error| error.to_string())?;
        return identity::Keypair::from_protobuf_encoding(&bytes)
            .map_err(|error| error.to_string());
    }

    if let Some(parent) = identity_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let keypair = identity::Keypair::generate_ed25519();
    let encoded = keypair
        .to_protobuf_encoding()
        .map_err(|error| error.to_string())?;

    #[cfg(unix)]
    {
        use std::{fs::OpenOptions, io::Write, os::unix::fs::OpenOptionsExt};
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&identity_path)
            .map_err(|error| error.to_string())?;
        file.write_all(&encoded)
            .map_err(|error| error.to_string())?;
    }
    #[cfg(not(unix))]
    {
        fs::write(&identity_path, encoded).map_err(|error| error.to_string())?;
    }

    Ok(keypair)
}

pub async fn spawn_swarm(
    app: AppHandle,
    state: P2pState,
    store: AtpStore,
    keypair: identity::Keypair,
    mut rx: mpsc::UnboundedReceiver<SwarmCommand>,
) -> Result<(String, Vec<String>), String> {
    let local_peer_id = keypair.public().to_peer_id();
    let local_agent_id = agent_id(&keypair.public());
    let runtime_keypair = keypair.clone();
    let network = configured_network().await?;

    let mut swarm = SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_tcp(
            tcp::Config::default().nodelay(true),
            noise::Config::new,
            yamux::Config::default,
        )
        .map_err(|error| error.to_string())?
        .with_quic()
        .with_dns()
        .map_err(|error| error.to_string())?
        .with_websocket(noise::Config::new, yamux::Config::default)
        .await
        .map_err(|error| error.to_string())?
        .with_relay_client(noise::Config::new, yamux::Config::default)
        .map_err(|error| error.to_string())?
        .with_behaviour(move |key, relay| {
            let peer_id = key.public().to_peer_id();
            let codec = request_response::json::codec::Codec::default()
                .set_request_size_maximum(MAX_WIRE_REQUEST_BYTES)
                .set_response_size_maximum(MAX_WIRE_RESPONSE_BYTES);
            let request_response = request_response::Behaviour::with_codec(
                codec,
                [(StreamProtocol::new(ATP_PROTOCOL), ProtocolSupport::Full)],
                request_response::Config::default().with_request_timeout(Duration::from_secs(90)),
            );
            let mdns = mdns::tokio::Behaviour::new(mdns::Config::default(), peer_id)?;
            let identify = identify::Behaviour::new(
                identify::Config::new(ATP_PROTOCOL.to_string(), key.public())
                    .with_agent_version(format!("CYPHES/{}", env!("CARGO_PKG_VERSION")))
                    .with_push_listen_addr_updates(true),
            );
            Ok(AgentBehaviour {
                request_response,
                mdns,
                identify,
                ping: ping::Behaviour::default(),
                relay,
                rendezvous: rendezvous::client::Behaviour::new(key.clone()),
                dcutr: dcutr::Behaviour::new(peer_id),
            })
        })
        .map_err(|error| error.to_string())?
        .with_swarm_config(|config| {
            config.with_idle_connection_timeout(PEER_IDLE_CONNECTION_TIMEOUT)
        })
        .build();

    for address in [
        "/ip4/0.0.0.0/tcp/0",
        "/ip4/0.0.0.0/tcp/0/ws",
        "/ip4/0.0.0.0/udp/0/quic-v1",
    ] {
        swarm
            .listen_on(
                address
                    .parse::<Multiaddr>()
                    .map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
    }

    dial_infrastructure(&mut swarm, &network)?;
    for address in configured_bootstrap_peers()? {
        swarm
            .dial(address)
            .map_err(|error| format!("could not dial bootstrap peer: {error}"))?;
    }

    let listen_addrs = swarm
        .listeners()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if let Ok(mut inner) = state.inner.lock() {
        inner.listen_addrs = listen_addrs.clone();
        inner.relay_configured = network.relay.is_some();
        inner.relay_connected = false;
        inner.rendezvous_registered = false;
        inner.bootstrap_source = network.source.clone();
        inner.last_infrastructure_activity_ms = now_millis();
    }

    tauri::async_runtime::spawn(async move {
        let mut outbound = HashMap::<OutboundRequestId, PendingOutbound>::new();
        let mut relay_reservation = RelayReservationState::default();
        let mut rendezvous_registration_started = false;
        let mut rendezvous_cookie = None;
        let mut last_verifier_liveness_discovery_ms = 0u64;
        let mut last_stale_receipt_repair_ms = 0u64;
        let mut discovery_interval = tokio::time::interval(RENDEZVOUS_DISCOVERY_INTERVAL);
        discovery_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut registration_interval = tokio::time::interval(RENDEZVOUS_REGISTRATION_INTERVAL);
        registration_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        registration_interval.tick().await;
        let mut infrastructure_interval = tokio::time::interval(INFRASTRUCTURE_RETRY_INTERVAL);
        infrastructure_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut labor_sync_interval = tokio::time::interval(LABOR_NETWORK_SYNC_INTERVAL);
        labor_sync_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            select! {
                maybe_command = rx.recv() => {
                    let Some(command) = maybe_command else {
                        break;
                    };
                    match command {
                        SwarmCommand::SendEnvelope(envelope) => {
                            send_envelope(&mut swarm, &state, envelope, &mut outbound);
                        }
                        SwarmCommand::SendCampaign(campaign) => {
                            send_campaign(&mut swarm, &state, campaign, &mut outbound);
                        }
                        SwarmCommand::SendWorkUnitClaim { claim, audience } => {
                            send_work_unit_claim(
                                &mut swarm,
                                &state,
                                claim,
                                &audience,
                                false,
                                &mut outbound,
                            );
                        }
                        SwarmCommand::SendExecutionResult { result, audience } => {
                            send_execution_result(
                                &mut swarm,
                                &state,
                                result,
                                &audience,
                                &mut outbound,
                            );
                        }
                        SwarmCommand::SendContribution {
                            contribution,
                            audience,
                        } => {
                            send_contribution(
                                &mut swarm,
                                &state,
                                contribution,
                                &audience,
                                &mut outbound,
                            );
                        }
                        SwarmCommand::SendVerificationResult {
                            verification,
                            allocations,
                            audience,
                        } => {
                            send_verification_result(
                                &mut swarm,
                                &state,
                                verification,
                                allocations,
                                &audience,
                                &mut outbound,
                            );
                        }
                        SwarmCommand::Dial(address) => {
                            dial_with_telemetry(
                                &mut swarm,
                                &app,
                                &store,
                                "peer_dial_failed",
                                relay_peer_id(&address),
                                address,
                            );
                        }
                    }
                }
                _ = discovery_interval.tick() => {
                    discover_rendezvous(
                        &mut swarm,
                        network.rendezvous.as_ref(),
                        &network.namespace,
                        rendezvous_cookie.clone(),
                    );
                }
                _ = registration_interval.tick() => {
                    register_rendezvous(
                        &mut swarm,
                        &app,
                        network.rendezvous.as_ref(),
                        &network.namespace,
                    );
                }
                _ = infrastructure_interval.tick() => {
                    ensure_infrastructure_connections(&mut swarm, &app, &state, &store, &network);
                    ensure_relay_reservation(
                        &mut swarm,
                        &app,
                        &state,
                        &store,
                        &network,
                        local_peer_id,
                        &mut relay_reservation,
                    );
                }
                _ = labor_sync_interval.tick() => {
                    sync_audit_labor_network(
                        &mut swarm,
                        &app,
                        &state,
                        &store,
                        &runtime_keypair,
                        &local_agent_id,
                        &network,
                        &mut rendezvous_cookie,
                        &mut last_verifier_liveness_discovery_ms,
                        &mut last_stale_receipt_repair_ms,
                        &mut outbound,
                    );
                }
                event = swarm.select_next_some() => {
                    handle_swarm_event(
                        event,
                        &mut swarm,
                        &app,
                        &state,
                        &store,
                        &runtime_keypair,
                        local_peer_id,
                        &local_agent_id,
                        &mut outbound,
                        &network,
                        &mut relay_reservation,
                        &mut rendezvous_registration_started,
                        &mut rendezvous_cookie,
                    );
                }
            }
        }
    });

    Ok((local_peer_id.to_string(), listen_addrs))
}

#[allow(clippy::too_many_arguments)]
fn handle_swarm_event(
    event: SwarmEvent<AgentBehaviourEvent>,
    swarm: &mut libp2p::Swarm<AgentBehaviour>,
    app: &AppHandle,
    state: &P2pState,
    store: &AtpStore,
    keypair: &identity::Keypair,
    local_peer_id: PeerId,
    local_agent_id: &str,
    outbound: &mut HashMap<OutboundRequestId, PendingOutbound>,
    network: &NetworkBootstrap,
    relay_reservation: &mut RelayReservationState,
    rendezvous_registration_started: &mut bool,
    rendezvous_cookie: &mut Option<rendezvous::Cookie>,
) {
    match event {
        SwarmEvent::Behaviour(AgentBehaviourEvent::RequestResponse(
            request_response::Event::Message { peer, message, .. },
        )) => match message {
            request_response::Message::Request {
                request, channel, ..
            } => {
                touch_peer(state, peer);
                let response = match request {
                    WireRequest::Envelope(envelope) => {
                        let ack = match store.commit_envelope(
                            &envelope,
                            local_agent_id,
                            Some(&peer.to_string()),
                        ) {
                            Ok(ack) => {
                                if !ack.duplicate {
                                    let _ = app.emit("atp:jobs_changed", ());
                                    let _ = app.emit("audit:labor_changed", ());
                                    maybe_attest(
                                        swarm,
                                        app,
                                        state,
                                        store,
                                        keypair,
                                        local_agent_id,
                                        peer,
                                        &envelope,
                                        &ack,
                                        outbound,
                                    );
                                    if envelope.verb == AtpVerb::Attest {
                                        emit_bundle_export(app, store, &envelope.transaction_id);
                                    }
                                }
                                ack
                            }
                            Err(reason) => {
                                let _ = app.emit(
                                    "atp:delivery_failed",
                                    serde_json::json!({
                                        "peerId": peer.to_string(),
                                        "reason": reason.clone(),
                                    }),
                                );
                                rejection_ack(&envelope, local_agent_id, reason)
                            }
                        };
                        WireResponse::Envelope(ack)
                    }
                    WireRequest::Campaign(campaign) => {
                        let campaign_id = campaign.campaign_id.clone();
                        let was_known = store.campaign_report_snapshot(&campaign_id).is_ok();
                        match store.upsert_protocol_campaign(&campaign) {
                            Ok(_) => {
                                if !was_known {
                                    let _ = app.emit("audit:labor_changed", ());
                                    let _ = app.emit(
                                        "audit:campaign_received",
                                        serde_json::json!({
                                            "campaignId": campaign_id,
                                            "protocolName": campaign.protocol_name,
                                        }),
                                    );
                                }
                                retry_pending_labor_objects(app, store);
                                WireResponse::Campaign {
                                    accepted: true,
                                    campaign_id,
                                    reason: None,
                                }
                            }
                            Err(reason) => WireResponse::Campaign {
                                accepted: false,
                                campaign_id,
                                reason: Some(reason),
                            },
                        }
                    }
                    WireRequest::WorkUnitClaim(claim) => {
                        let campaign_id = claim.campaign_id.clone();
                        let work_unit_id = claim.work_unit_id.clone();
                        let claim_id = claim.claim_id.clone();
                        match record_work_unit_claim_for_sync(store, &claim) {
                            Ok(_) => {
                                let _ = app.emit("audit:labor_changed", ());
                                let _ = app.emit(
                                    "audit:work_unit_claimed",
                                    serde_json::json!({
                                        "campaignId": campaign_id,
                                        "workUnitId": work_unit_id,
                                        "claimId": claim_id,
                                    }),
                                );
                                retry_pending_labor_objects(app, store);
                                WireResponse::WorkUnitClaim {
                                    accepted: true,
                                    campaign_id,
                                    work_unit_id,
                                    claim_id,
                                    reason: None,
                                }
                            }
                            Err(reason) if is_labor_dependency_error(&reason) => {
                                queue_pending_labor_object(
                                    store, "claim", &claim_id, &claim, &reason,
                                );
                                WireResponse::WorkUnitClaim {
                                    accepted: true,
                                    campaign_id,
                                    work_unit_id,
                                    claim_id,
                                    reason: Some(format!("queued pending dependency: {reason}")),
                                }
                            }
                            Err(reason) => WireResponse::WorkUnitClaim {
                                accepted: false,
                                campaign_id,
                                work_unit_id,
                                claim_id,
                                reason: Some(reason),
                            },
                        }
                    }
                    WireRequest::ExecutionResult(result) => {
                        let transaction_id = result.transaction_id.clone();
                        let result_hash = result.result_hash.clone();
                        match store.save_execution_result(&result) {
                            Ok(()) => {
                                let _ = app.emit("atp:jobs_changed", ());
                                let _ = app.emit(
                                    "atp:result_received",
                                    serde_json::json!({
                                        "transactionId": transaction_id,
                                        "resultHash": result_hash,
                                    }),
                                );
                                WireResponse::ExecutionResult {
                                    accepted: true,
                                    transaction_id,
                                    result_hash,
                                    reason: None,
                                }
                            }
                            Err(reason) => WireResponse::ExecutionResult {
                                accepted: false,
                                transaction_id,
                                result_hash,
                                reason: Some(reason),
                            },
                        }
                    }
                    WireRequest::Contribution(contribution) => {
                        let campaign_id = contribution.campaign_id.clone();
                        let contribution_id = contribution.contribution_id.clone();
                        let receipt_hash = contribution.receipt_hash.clone();
                        if let Some(response) =
                            match store.contribution_preflight_status(&contribution) {
                                Ok(status) if status.skip_reason().is_some() => {
                                    let reason = status
                                        .skip_reason()
                                        .unwrap_or("contribution duplicate skipped");
                                    Some(WireResponse::Contribution {
                                        accepted: true,
                                        campaign_id: campaign_id.clone(),
                                        contribution_id: contribution_id.clone(),
                                        receipt_hash: receipt_hash.clone(),
                                        reason: Some(format!("duplicate skipped: {reason}")),
                                    })
                                }
                                Err(reason) => Some(WireResponse::Contribution {
                                    accepted: false,
                                    campaign_id: campaign_id.clone(),
                                    contribution_id: contribution_id.clone(),
                                    receipt_hash: receipt_hash.clone(),
                                    reason: Some(reason),
                                }),
                                _ => None,
                            }
                        {
                            response
                        } else {
                            match store.record_network_contribution(&contribution) {
                                Ok(_) => {
                                    let _ = app.emit("audit:labor_changed", ());
                                    let _ = app.emit(
                                        "audit:contribution_received",
                                        serde_json::json!({
                                            "campaignId": campaign_id,
                                            "contributionId": contribution_id,
                                            "receiptHash": receipt_hash,
                                        }),
                                    );
                                    retry_pending_labor_objects(app, store);
                                    WireResponse::Contribution {
                                        accepted: true,
                                        campaign_id,
                                        contribution_id,
                                        receipt_hash,
                                        reason: None,
                                    }
                                }
                                Err(reason) if is_labor_dependency_error(&reason) => {
                                    queue_pending_labor_object(
                                        store,
                                        "contribution",
                                        &contribution_id,
                                        &contribution,
                                        &reason,
                                    );
                                    WireResponse::Contribution {
                                        accepted: true,
                                        campaign_id,
                                        contribution_id,
                                        receipt_hash,
                                        reason: Some(format!(
                                            "queued pending dependency: {reason}"
                                        )),
                                    }
                                }
                                Err(reason) => WireResponse::Contribution {
                                    accepted: false,
                                    campaign_id,
                                    contribution_id,
                                    receipt_hash,
                                    reason: Some(reason),
                                },
                            }
                        }
                    }
                    WireRequest::VerificationResult {
                        verification,
                        allocations,
                    } => {
                        let campaign_id = verification.campaign_id.clone();
                        let verification_id = verification.verification_id.clone();
                        let credit_total = allocations
                            .iter()
                            .map(|allocation| allocation.total)
                            .sum::<u32>();
                        if let Some(response) =
                            match store.verification_bundle_preflight_status(&verification) {
                                Ok(status) if status.skip_reason().is_some() => {
                                    let reason = status
                                        .skip_reason()
                                        .unwrap_or("verification duplicate skipped");
                                    Some(WireResponse::VerificationResult {
                                        accepted: true,
                                        campaign_id: campaign_id.clone(),
                                        verification_id: verification_id.clone(),
                                        credit_total,
                                        reason: Some(format!("duplicate skipped: {reason}")),
                                    })
                                }
                                Err(reason) => Some(WireResponse::VerificationResult {
                                    accepted: false,
                                    campaign_id: campaign_id.clone(),
                                    verification_id: verification_id.clone(),
                                    credit_total,
                                    reason: Some(reason),
                                }),
                                _ => None,
                            }
                        {
                            response
                        } else {
                            match store.record_verification_bundle(&verification, &allocations) {
                                Ok(_) => {
                                    let _ = app.emit("audit:labor_changed", ());
                                    let _ = app.emit(
                                        "audit:verification_received",
                                        serde_json::json!({
                                            "campaignId": campaign_id,
                                            "verificationId": verification_id,
                                            "creditTotal": credit_total,
                                        }),
                                    );
                                    retry_pending_labor_objects(app, store);
                                    WireResponse::VerificationResult {
                                        accepted: true,
                                        campaign_id,
                                        verification_id,
                                        credit_total,
                                        reason: None,
                                    }
                                }
                                Err(reason) if is_labor_dependency_error(&reason) => {
                                    let bundle = VerificationBundleWire {
                                        verification: verification.clone(),
                                        allocations: allocations.clone(),
                                    };
                                    queue_pending_labor_object(
                                        store,
                                        "verification",
                                        &verification_id,
                                        &bundle,
                                        &reason,
                                    );
                                    WireResponse::VerificationResult {
                                        accepted: true,
                                        campaign_id,
                                        verification_id,
                                        credit_total,
                                        reason: Some(format!(
                                            "queued pending dependency: {reason}"
                                        )),
                                    }
                                }
                                Err(reason) => WireResponse::VerificationResult {
                                    accepted: false,
                                    campaign_id,
                                    verification_id,
                                    credit_total,
                                    reason: Some(reason),
                                },
                            }
                        }
                    }
                    WireRequest::LaborInventory(inventory) => {
                        WireResponse::LaborInventory(handle_labor_inventory_request(
                            swarm,
                            app,
                            state,
                            store,
                            keypair,
                            local_agent_id,
                            peer,
                            inventory,
                            outbound,
                        ))
                    }
                    WireRequest::LaborObjectRequest(request) => {
                        WireResponse::LaborObjectBundle(build_labor_object_bundle(store, request))
                    }
                    WireRequest::LaborObjectBundle(bundle) => {
                        let peer_id = peer.to_string();
                        let response =
                            ingest_labor_object_bundle(app, store, Some(peer_id.as_str()), bundle);
                        if response.accepted {
                            verify_network_contributions(
                                swarm,
                                app,
                                state,
                                store,
                                keypair,
                                local_agent_id,
                                outbound,
                            );
                        }
                        WireResponse::LaborObjectBundleAck(response)
                    }
                };
                let _ = swarm
                    .behaviour_mut()
                    .request_response
                    .send_response(channel, response);
            }
            request_response::Message::Response {
                request_id,
                response,
            } => {
                let pending = outbound.remove(&request_id);
                mark_peer_success(state, peer);
                if pending.as_ref().is_some_and(PendingOutbound::is_silent) {
                    return;
                }
                match response {
                    WireResponse::Envelope(ack) => {
                        if let Err(error) = store.mark_delivery(&peer.to_string(), &ack) {
                            let _ = app.emit(
                                "atp:delivery_failed",
                                serde_json::json!({ "peerId": peer.to_string(), "reason": error }),
                            );
                        } else if ack.accepted {
                            let _ = app.emit("atp:jobs_changed", ());
                            let _ = app.emit("audit:labor_changed", ());
                            let _ = app.emit("atp:delivery_acknowledged", ack);
                        } else {
                            let _ = app.emit(
                                "atp:delivery_failed",
                                serde_json::json!({
                                    "peerId": peer.to_string(),
                                    "reason": ack.reason.unwrap_or_else(|| {
                                        ack.reason_code
                                            .unwrap_or_else(|| "ATP_VALIDATION_FAILED".to_string())
                                    }),
                                }),
                            );
                        }
                    }
                    WireResponse::Campaign {
                        accepted,
                        campaign_id,
                        reason,
                    } => {
                        let _ = app.emit(
                            if accepted {
                                "audit:campaign_acknowledged"
                            } else {
                                "atp:delivery_failed"
                            },
                            serde_json::json!({
                                "peerId": peer.to_string(),
                                "campaignId": campaign_id,
                                "reason": reason,
                            }),
                        );
                    }
                    WireResponse::WorkUnitClaim {
                        accepted,
                        campaign_id,
                        work_unit_id,
                        claim_id,
                        reason,
                    } => {
                        let _ = app.emit(
                            if accepted {
                                "audit:work_unit_claim_acknowledged"
                            } else {
                                "atp:delivery_failed"
                            },
                            serde_json::json!({
                                "peerId": peer.to_string(),
                                "campaignId": campaign_id,
                                "workUnitId": work_unit_id,
                                "claimId": claim_id,
                                "reason": reason,
                            }),
                        );
                    }
                    WireResponse::ExecutionResult {
                        accepted,
                        transaction_id,
                        result_hash,
                        reason,
                    } => {
                        let _ = app.emit(
                            if accepted {
                                "atp:result_acknowledged"
                            } else {
                                "atp:delivery_failed"
                            },
                            serde_json::json!({
                                "peerId": peer.to_string(),
                                "transactionId": transaction_id,
                                "resultHash": result_hash,
                                "reason": reason,
                            }),
                        );
                    }
                    WireResponse::Contribution {
                        accepted,
                        campaign_id,
                        contribution_id,
                        receipt_hash,
                        reason,
                    } => {
                        let _ = app.emit(
                            if accepted {
                                "audit:contribution_acknowledged"
                            } else {
                                "atp:delivery_failed"
                            },
                            serde_json::json!({
                                "peerId": peer.to_string(),
                                "campaignId": campaign_id,
                                "contributionId": contribution_id,
                                "receiptHash": receipt_hash,
                                "reason": reason,
                            }),
                        );
                    }
                    WireResponse::VerificationResult {
                        accepted,
                        campaign_id,
                        verification_id,
                        credit_total,
                        reason,
                    } => {
                        let _ = app.emit(
                            if accepted {
                                "audit:verification_acknowledged"
                            } else {
                                "atp:delivery_failed"
                            },
                            serde_json::json!({
                                "peerId": peer.to_string(),
                                "campaignId": campaign_id,
                                "verificationId": verification_id,
                                "creditTotal": credit_total,
                                "reason": reason,
                            }),
                        );
                    }
                    WireResponse::LaborInventory(response) => {
                        if response.accepted {
                            let inventory_peer = pending
                                .as_ref()
                                .and_then(|pending| match pending {
                                    PendingOutbound::LaborInventory { peer_id } => {
                                        Some(peer_id.clone())
                                    }
                                    _ => None,
                                })
                                .unwrap_or(peer);
                            push_labor_objects_to_peer(
                                swarm,
                                state,
                                store,
                                &inventory_peer,
                                &response.missing_campaigns,
                                &response.missing_claims,
                                &response.missing_contributions,
                                &response.missing_verifications,
                                outbound,
                            );
                            let _ = app.emit(
                                "audit:labor_inventory_resynced",
                                serde_json::json!({
                                    "peerId": inventory_peer.to_string(),
                                    "missingCampaigns": response.missing_campaigns.len(),
                                    "missingClaims": response.missing_claims.len(),
                                    "missingContributions": response.missing_contributions.len(),
                                    "missingVerifications": response.missing_verifications.len(),
                                }),
                            );
                        } else {
                            let _ = app.emit(
                                "atp:delivery_failed",
                                serde_json::json!({
                                    "peerId": peer.to_string(),
                                    "reason": response.reason,
                                }),
                            );
                        }
                    }
                    WireResponse::LaborObjectBundle(bundle) => {
                        let peer_id = peer.to_string();
                        let response =
                            ingest_labor_object_bundle(app, store, Some(peer_id.as_str()), bundle);
                        if response.accepted {
                            verify_network_contributions(
                                swarm,
                                app,
                                state,
                                store,
                                keypair,
                                local_agent_id,
                                outbound,
                            );
                        }
                        let _ = app.emit(
                            if response.accepted {
                                "audit:labor_object_bundle_received"
                            } else {
                                "atp:delivery_failed"
                            },
                            serde_json::json!({
                                "peerId": peer.to_string(),
                                "campaigns": response.campaigns,
                                "claims": response.claims,
                                "contributions": response.contributions,
                                "verifications": response.verifications,
                                "queued": response.queued,
                                "reason": response.reason,
                            }),
                        );
                    }
                    WireResponse::LaborObjectBundleAck(response) => {
                        let _ = app.emit(
                            if response.accepted {
                                "audit:labor_object_bundle_acknowledged"
                            } else {
                                "atp:delivery_failed"
                            },
                            serde_json::json!({
                                "peerId": peer.to_string(),
                                "campaigns": response.campaigns,
                                "claims": response.claims,
                                "contributions": response.contributions,
                                "verifications": response.verifications,
                                "queued": response.queued,
                                "reason": response.reason,
                            }),
                        );
                    }
                }
                if pending.is_none() {
                    let _ = app.emit(
                        "atp:delivery_failed",
                        serde_json::json!({"reason": "received response for unknown request"}),
                    );
                }
            }
        },
        SwarmEvent::Behaviour(AgentBehaviourEvent::RequestResponse(
            request_response::Event::OutboundFailure {
                peer,
                request_id,
                error,
                ..
            },
        )) => {
            let pending = outbound.remove(&request_id);
            let silent = pending.as_ref().is_some_and(PendingOutbound::is_silent);
            if !silent {
                mark_peer_failure(state, peer);
            }
            *rendezvous_cookie = None;
            discover_rendezvous(swarm, network.rendezvous.as_ref(), &network.namespace, None);
            let peer_string = peer.to_string();
            let error_string = error.to_string();
            let _ = store.record_labor_event(
                "outbound_request_failed",
                Some(peer_string.as_str()),
                None,
                None,
                false,
                Some(error_string.as_str()),
                &serde_json::json!({
                    "peerId": peer_string,
                    "requestId": format!("{request_id:?}"),
                    "silent": silent,
                }),
            );
            let _ = app.emit(
                "p2p:peer_resync_requested",
                serde_json::json!({
                    "peerId": peer.to_string(),
                    "reason": error_string,
                    "silent": silent,
                }),
            );
            if silent {
                return;
            }
            let (event_hash, transaction_id, result_hash) = match pending {
                Some(PendingOutbound::Envelope { event_hash, .. }) => {
                    (Some(event_hash), None, None)
                }
                Some(PendingOutbound::Campaign { campaign_id, .. }) => {
                    (None, Some(campaign_id), None)
                }
                Some(PendingOutbound::WorkUnitClaim {
                    campaign_id,
                    work_unit_id,
                    claim_id,
                    ..
                }) => (
                    None,
                    Some(format!("{campaign_id}:{work_unit_id}:{claim_id}")),
                    None,
                ),
                Some(PendingOutbound::ExecutionResult {
                    transaction_id,
                    result_hash,
                    ..
                }) => (None, Some(transaction_id), Some(result_hash)),
                Some(PendingOutbound::Contribution {
                    campaign_id,
                    contribution_id,
                    receipt_hash,
                    ..
                }) => (
                    None,
                    Some(format!("{campaign_id}:{contribution_id}")),
                    Some(receipt_hash),
                ),
                Some(PendingOutbound::VerificationResult {
                    campaign_id,
                    verification_id,
                    credit_total,
                    ..
                }) => (
                    None,
                    Some(format!("{campaign_id}:{verification_id}:{credit_total}")),
                    None,
                ),
                Some(PendingOutbound::LaborInventory { peer_id }) => {
                    (None, Some(format!("labor-inventory:{peer_id}")), None)
                }
                Some(PendingOutbound::LaborObjectBundle { peer_id, .. }) => {
                    (None, Some(format!("labor-object-bundle:{peer_id}")), None)
                }
                None => (None, None, None),
            };
            let _ = app.emit(
                "atp:delivery_failed",
                serde_json::json!({
                    "peerId": peer.to_string(),
                    "eventHash": event_hash,
                    "transactionId": transaction_id,
                    "resultHash": result_hash,
                    "reason": error.to_string(),
                }),
            );
        }
        SwarmEvent::Behaviour(AgentBehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
            for (peer_id, addr) in list {
                if peer_id == local_peer_id {
                    continue;
                }
                swarm.add_peer_address(peer_id, addr.clone());
                if !swarm.is_connected(&peer_id) {
                    dial_with_telemetry(swarm, app, store, "peer_dial_failed", Some(peer_id), addr);
                }
            }
        }
        SwarmEvent::Behaviour(AgentBehaviourEvent::Mdns(mdns::Event::Expired(list))) => {
            if let Ok(mut inner) = state.inner.lock() {
                for (peer_id, _addr) in list {
                    inner.peers.remove(&peer_id.to_string());
                    let _ = app.emit(
                        "p2p:peer_disconnected",
                        serde_json::json!({ "peerId": peer_id.to_string() }),
                    );
                }
            }
        }
        SwarmEvent::Behaviour(AgentBehaviourEvent::Identify(identify::Event::Received {
            peer_id,
            info,
            ..
        })) => {
            for address in info.listen_addrs {
                if !is_private_or_local_address(&address) {
                    swarm.add_peer_address(peer_id, address);
                }
            }
            if !is_infrastructure_peer(network, peer_id) {
                on_peer_connected(swarm, app, state, store, local_agent_id, peer_id, outbound);
            }
        }
        SwarmEvent::Behaviour(AgentBehaviourEvent::Ping(ping::Event { peer, result, .. })) => {
            if is_infrastructure_peer(network, peer) {
                match result {
                    Ok(_) => mark_infrastructure_activity(state, network, peer),
                    Err(error) => {
                        clear_infrastructure_connection_state(state, network, peer);
                        record_dial_failure(
                            app,
                            store,
                            "infrastructure_dial_failed",
                            Some(peer),
                            None,
                            format!("infrastructure ping failed: {error}").as_str(),
                        );
                    }
                }
            }
        }
        SwarmEvent::Behaviour(AgentBehaviourEvent::Relay(
            relay::client::Event::ReservationReqAccepted {
                relay_peer_id,
                renewal,
                ..
            },
        )) => {
            relay_reservation.confirmed = true;
            relay_reservation.requested_at_ms = None;
            mark_infrastructure_activity(state, network, relay_peer_id);
            if let Some(relay) = network
                .relay
                .as_ref()
                .filter(|relay| relay.peer_id == relay_peer_id)
            {
                let address = relay_circuit_address(relay, local_peer_id);
                swarm.add_external_address(address.clone());
                let address = address.to_string();
                if let Ok(mut inner) = state.inner.lock() {
                    inner.listen_addrs.retain(|existing| existing != &address);
                    inner.listen_addrs.insert(0, address.clone());
                }
                let _ = app.emit(
                    "p2p:listen_address",
                    serde_json::json!({"address": address}),
                );
            }
            if let Ok(mut inner) = state.inner.lock() {
                inner.relay_connected = true;
            }
            let _ = app.emit(
                "p2p:relay_ready",
                serde_json::json!({
                    "relayPeerId": relay_peer_id.to_string(),
                    "renewal": renewal,
                }),
            );
            if !*rendezvous_registration_started
                && register_rendezvous(swarm, app, network.rendezvous.as_ref(), &network.namespace)
            {
                *rendezvous_registration_started = true;
            }
        }
        SwarmEvent::Behaviour(AgentBehaviourEvent::Relay(event)) => match event {
            relay::client::Event::OutboundCircuitEstablished { relay_peer_id, .. }
            | relay::client::Event::InboundCircuitEstablished {
                src_peer_id: relay_peer_id,
                ..
            } => {
                mark_infrastructure_activity(state, network, relay_peer_id);
            }
            relay::client::Event::ReservationReqAccepted { .. } => {}
        },
        SwarmEvent::Behaviour(AgentBehaviourEvent::Rendezvous(event)) => match event {
            rendezvous::client::Event::Registered {
                rendezvous_node,
                namespace,
                ..
            } => {
                mark_infrastructure_activity(state, network, rendezvous_node);
                if let Ok(mut inner) = state.inner.lock() {
                    inner.rendezvous_registered = true;
                }
                let _ = app.emit(
                    "p2p:rendezvous_registered",
                    serde_json::json!({
                        "rendezvousPeerId": rendezvous_node.to_string(),
                        "namespace": namespace.to_string(),
                    }),
                );
                discover_rendezvous(
                    swarm,
                    network.rendezvous.as_ref(),
                    &network.namespace,
                    rendezvous_cookie.clone(),
                );
            }
            rendezvous::client::Event::Discovered {
                registrations,
                cookie,
                rendezvous_node,
                ..
            } => {
                mark_infrastructure_activity(state, network, rendezvous_node);
                *rendezvous_cookie = Some(cookie);
                let mut discovered = 0usize;
                let mut discovery_dials = 0usize;
                for registration in registrations {
                    let peer_id = registration.record.peer_id();
                    if peer_id == local_peer_id || is_infrastructure_peer(network, peer_id) {
                        continue;
                    }
                    let addresses = registration.record.addresses();
                    let dial_candidates =
                        discovered_peer_dial_candidates(network, peer_id, addresses);
                    for address in &dial_candidates {
                        swarm.add_peer_address(peer_id, address.clone());
                    }
                    if discovery_dials < MAX_DISCOVERY_PEER_DIALS_PER_TICK
                        && !swarm.is_connected(&peer_id)
                        && peer_dial_allowed(state, peer_id)
                    {
                        for address in preferred_dial_candidates(&dial_candidates) {
                            if !dial_with_telemetry(
                                swarm,
                                app,
                                store,
                                "peer_dial_failed",
                                Some(peer_id),
                                address,
                            ) {
                                mark_peer_failure(state, peer_id);
                            } else {
                                mark_peer_dial_attempt(state, peer_id);
                            }
                        }
                        discovery_dials += 1;
                    }
                    discovered += 1;
                }
                let _ = app.emit(
                    "p2p:rendezvous_discovered",
                    serde_json::json!({"discovered": discovered}),
                );
            }
            rendezvous::client::Event::RegisterFailed { error, .. } => {
                *rendezvous_registration_started = false;
                if let Ok(mut inner) = state.inner.lock() {
                    inner.rendezvous_registered = false;
                }
                let _ = app.emit(
                    "p2p:connection_failed",
                    serde_json::json!({
                        "reason": format!("rendezvous registration failed: {error:?}"),
                    }),
                );
            }
            rendezvous::client::Event::DiscoverFailed { error, .. } => {
                if error == rendezvous::ErrorCode::InvalidCookie {
                    *rendezvous_cookie = None;
                    discover_rendezvous(
                        swarm,
                        network.rendezvous.as_ref(),
                        &network.namespace,
                        None,
                    );
                }
                let _ = app.emit(
                    "p2p:connection_failed",
                    serde_json::json!({
                        "reason": format!("rendezvous discovery failed: {error:?}"),
                    }),
                );
            }
            rendezvous::client::Event::Expired { peer } => {
                if let Ok(mut inner) = state.inner.lock() {
                    inner.peers.remove(&peer.to_string());
                }
            }
        },
        SwarmEvent::ConnectionEstablished { peer_id, .. } => {
            if is_infrastructure_peer(network, peer_id) {
                if let Ok(mut inner) = state.inner.lock() {
                    inner
                        .infrastructure_recycle_pending
                        .remove(&peer_id.to_string());
                }
                mark_infrastructure_activity(state, network, peer_id);
            }
            if !relay_reservation.confirmed {
                if let Some(relay) = network.relay.as_ref() {
                    if peer_id == relay.peer_id {
                        ensure_relay_reservation(
                            swarm,
                            app,
                            state,
                            store,
                            network,
                            local_peer_id,
                            relay_reservation,
                        );
                    }
                }
            }
            if !is_infrastructure_peer(network, peer_id) {
                on_peer_connected(swarm, app, state, store, local_agent_id, peer_id, outbound);
            }
        }
        SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
            if let Some(peer_id) = peer_id {
                if is_infrastructure_peer(network, peer_id) {
                    if let Ok(mut inner) = state.inner.lock() {
                        inner
                            .infrastructure_recycle_pending
                            .remove(&peer_id.to_string());
                    }
                    clear_infrastructure_connection_state(state, network, peer_id);
                    record_dial_failure(
                        app,
                        store,
                        "infrastructure_dial_failed",
                        Some(peer_id),
                        None,
                        error.to_string().as_str(),
                    );
                } else {
                    mark_peer_failure(state, peer_id);
                    record_dial_failure(
                        app,
                        store,
                        "peer_dial_failed",
                        Some(peer_id),
                        None,
                        error.to_string().as_str(),
                    );
                }
            } else {
                record_dial_failure(
                    app,
                    store,
                    "peer_dial_failed",
                    None,
                    None,
                    error.to_string().as_str(),
                );
            }
        }
        SwarmEvent::ConnectionClosed {
            peer_id,
            num_established,
            ..
        } => {
            if num_established > 0 {
                return;
            }
            if let Ok(mut inner) = state.inner.lock() {
                inner
                    .infrastructure_recycle_pending
                    .remove(&peer_id.to_string());
            }
            if network
                .relay
                .as_ref()
                .is_some_and(|relay| relay.peer_id == peer_id)
            {
                lose_relay_reservation(
                    swarm,
                    app,
                    state,
                    store,
                    network,
                    local_peer_id,
                    relay_reservation,
                    rendezvous_registration_started,
                    rendezvous_cookie,
                    "relay connection closed",
                );
                clear_infrastructure_connection_state(state, network, peer_id);
            }
            if network
                .rendezvous
                .as_ref()
                .is_some_and(|rendezvous| rendezvous.peer_id == peer_id)
            {
                *rendezvous_registration_started = false;
                clear_infrastructure_connection_state(state, network, peer_id);
                if let Ok(mut inner) = state.inner.lock() {
                    inner.rendezvous_registered = false;
                }
            }
            if let Ok(mut inner) = state.inner.lock() {
                inner.peers.remove(&peer_id.to_string());
                inner.active_peer_links.remove(&peer_id.to_string());
            }
            if !is_infrastructure_peer(network, peer_id) {
                *rendezvous_cookie = None;
                discover_rendezvous(swarm, network.rendezvous.as_ref(), &network.namespace, None);
                let _ = app.emit(
                    "p2p:peer_disconnected",
                    serde_json::json!({ "peerId": peer_id.to_string() }),
                );
            }
        }
        SwarmEvent::NewListenAddr { address, .. } => {
            if is_relay_circuit_address(&address) && !relay_reservation.confirmed {
                return;
            }
            let address = address.to_string();
            if let Ok(mut inner) = state.inner.lock() {
                if !inner.listen_addrs.contains(&address) {
                    inner.listen_addrs.push(address.clone());
                }
            }
            let _ = app.emit(
                "p2p:listen_address",
                serde_json::json!({"address": address}),
            );
        }
        SwarmEvent::ExternalAddrConfirmed { address } => {
            if is_relay_circuit_address(&address)
                && relay_reservation.confirmed
                && !*rendezvous_registration_started
                && register_rendezvous(swarm, app, network.rendezvous.as_ref(), &network.namespace)
            {
                *rendezvous_registration_started = true;
            }
        }
        SwarmEvent::ExternalAddrExpired { address } => {
            if is_relay_circuit_address(&address) {
                lose_relay_reservation(
                    swarm,
                    app,
                    state,
                    store,
                    network,
                    local_peer_id,
                    relay_reservation,
                    rendezvous_registration_started,
                    rendezvous_cookie,
                    "relay circuit external address expired",
                );
                ensure_relay_reservation(
                    swarm,
                    app,
                    state,
                    store,
                    network,
                    local_peer_id,
                    relay_reservation,
                );
            }
        }
        SwarmEvent::ListenerClosed {
            listener_id,
            addresses,
            reason,
            ..
        } => {
            if relay_reservation.listener_id == Some(listener_id)
                || addresses.iter().any(is_relay_circuit_address)
            {
                let reason = reason
                    .as_ref()
                    .err()
                    .map(|error| format!("relay circuit listener closed: {error}"))
                    .unwrap_or_else(|| "relay circuit listener closed".to_string());
                lose_relay_reservation(
                    swarm,
                    app,
                    state,
                    store,
                    network,
                    local_peer_id,
                    relay_reservation,
                    rendezvous_registration_started,
                    rendezvous_cookie,
                    &reason,
                );
                ensure_relay_reservation(
                    swarm,
                    app,
                    state,
                    store,
                    network,
                    local_peer_id,
                    relay_reservation,
                );
            }
            if let Ok(mut inner) = state.inner.lock() {
                let expired = addresses
                    .into_iter()
                    .map(|address| address.to_string())
                    .collect::<Vec<_>>();
                inner
                    .listen_addrs
                    .retain(|address| !expired.contains(address));
            }
        }
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn maybe_attest(
    swarm: &mut libp2p::Swarm<AgentBehaviour>,
    app: &AppHandle,
    state: &P2pState,
    store: &AtpStore,
    keypair: &identity::Keypair,
    local_agent_id: &str,
    peer: PeerId,
    envelope: &AtpEnvelope,
    ack: &AtpAck,
    outbound: &mut HashMap<OutboundRequestId, PendingOutbound>,
) {
    if envelope.verb != AtpVerb::Settle {
        return;
    }
    let Ok(AuditEventBody::SettlementApproved { approved, .. }) =
        serde_json::from_value::<AuditEventBody>(envelope.body.clone())
    else {
        return;
    };
    let receipt = match store.build_worker_receipt(
        &envelope.transaction_id,
        &ack.event_hash,
        approved,
        keypair,
    ) {
        Ok(receipt) => receipt,
        Err(reason) => {
            let _ = app.emit(
                "atp:delivery_failed",
                serde_json::json!({"transactionId": envelope.transaction_id, "reason": reason}),
            );
            return;
        }
    };
    let attest = match create_signed_envelope(
        keypair,
        AtpVerb::Attest,
        envelope.transaction_id.clone(),
        Some(envelope.issuer.clone()),
        Some(ack.event_hash.clone()),
        serde_json::to_value(receipt).unwrap_or_default(),
    ) {
        Ok(attest) => attest,
        Err(reason) => {
            let _ = app.emit(
                "atp:delivery_failed",
                serde_json::json!({"transactionId": envelope.transaction_id, "reason": reason}),
            );
            return;
        }
    };
    match store.commit_envelope(&attest, local_agent_id, None) {
        Ok(_) => {
            emit_bundle_export(app, store, &attest.transaction_id);
            let event_hash = crate::atp::event_hash(&attest).unwrap_or_default();
            send_wire_request_to_peer(
                swarm,
                state,
                outbound,
                WireRequest::Envelope(attest),
                PendingOutbound::Envelope {
                    peer_id: peer,
                    event_hash,
                },
            );
            let _ = app.emit("atp:jobs_changed", ());
        }
        Err(reason) => {
            let _ = app.emit(
                "atp:delivery_failed",
                serde_json::json!({"transactionId": envelope.transaction_id, "reason": reason}),
            );
        }
    }
}

fn emit_bundle_export(app: &AppHandle, store: &AtpStore, transaction_id: &str) {
    match export_receipt_bundle(store, transaction_id) {
        Ok(path) => {
            let _ = app.emit(
                "atp:receipt_committed",
                serde_json::json!({
                    "transactionId": transaction_id,
                    "bundlePath": path.to_string_lossy(),
                }),
            );
            let _ = app.emit("atp:jobs_changed", ());
        }
        Err(reason) => {
            let _ = app.emit(
                "atp:delivery_failed",
                serde_json::json!({"transactionId": transaction_id, "reason": reason}),
            );
        }
    }
}

fn on_peer_connected(
    swarm: &mut libp2p::Swarm<AgentBehaviour>,
    app: &AppHandle,
    state: &P2pState,
    store: &AtpStore,
    local_agent_id: &str,
    peer_id: PeerId,
    outbound: &mut HashMap<OutboundRequestId, PendingOutbound>,
) {
    touch_peer(state, peer_id);
    if let Ok(mut inner) = state.inner.lock() {
        inner.active_peer_links.insert(peer_id.to_string());
    }
    let peer_agent_id = format!("urn:libp2p:{peer_id}");
    if let Ok(envelopes) = store.envelopes_for_peer(local_agent_id, &peer_agent_id) {
        for envelope in envelopes {
            let event_hash = crate::atp::event_hash(&envelope).unwrap_or_default();
            send_wire_request_to_peer(
                swarm,
                state,
                outbound,
                WireRequest::Envelope(envelope),
                PendingOutbound::Envelope {
                    peer_id,
                    event_hash,
                },
            );
        }
    }
    send_labor_inventory_to_peer(swarm, state, store, local_agent_id, &peer_id, outbound);
    let _ = app.emit(
        "p2p:peer_connected",
        serde_json::json!({ "peerId": peer_id.to_string() }),
    );
}

fn sync_audit_labor_network(
    swarm: &mut libp2p::Swarm<AgentBehaviour>,
    app: &AppHandle,
    state: &P2pState,
    store: &AtpStore,
    keypair: &identity::Keypair,
    local_agent_id: &str,
    network: &NetworkBootstrap,
    rendezvous_cookie: &mut Option<rendezvous::Cookie>,
    last_verifier_liveness_discovery_ms: &mut u64,
    last_stale_receipt_repair_ms: &mut u64,
    outbound: &mut HashMap<OutboundRequestId, PendingOutbound>,
) {
    let has_peers = !target_peers(state, None).is_empty();

    match store.expire_stale_claims(WORK_UNIT_CLAIM_TTL_MS) {
        Ok(expired) if expired > 0 => {
            let _ = app.emit("audit:labor_changed", ());
            let _ = app.emit(
                "audit:stale_claims_expired",
                serde_json::json!({ "expiredClaims": expired }),
            );
        }
        Err(reason) => {
            let _ = app.emit(
                "atp:delivery_failed",
                serde_json::json!({
                    "reason": format!("stale claim repair failed: {reason}"),
                }),
            );
        }
        _ => {}
    }

    recover_verifier_liveness_if_stale(
        swarm,
        app,
        state,
        store,
        local_agent_id,
        network,
        rendezvous_cookie,
        last_verifier_liveness_discovery_ms,
    );

    if has_peers {
        maybe_repair_stale_receipts(
            swarm,
            app,
            state,
            store,
            last_stale_receipt_repair_ms,
            outbound,
        );
        if outbound.len() < MAX_OUTBOUND_BULK_BACKLOG {
            broadcast_labor_inventory(swarm, state, store, local_agent_id, outbound);
        }
    }

    verify_network_contributions(swarm, app, state, store, keypair, local_agent_id, outbound);
}

#[allow(clippy::too_many_arguments)]
fn recover_verifier_liveness_if_stale(
    swarm: &mut libp2p::Swarm<AgentBehaviour>,
    app: &AppHandle,
    state: &P2pState,
    store: &AtpStore,
    local_agent_id: &str,
    network: &NetworkBootstrap,
    rendezvous_cookie: &mut Option<rendezvous::Cookie>,
    last_verifier_liveness_discovery_ms: &mut u64,
) {
    let pending_receipts = match store.pending_contribution_count_for_worker(local_agent_id) {
        Ok(count) if count > 0 => count,
        _ => return,
    };
    let Some(oldest_pending_at) = store
        .oldest_pending_contribution_time_for_worker(local_agent_id)
        .ok()
        .flatten()
    else {
        return;
    };

    let now = now_millis();
    let oldest_pending_age_ms = now.saturating_sub(oldest_pending_at);
    if oldest_pending_age_ms < STALE_RECEIPT_REPAIR_AFTER.as_millis() as u64 {
        return;
    }

    let latest_independent_verification_age_ms = store
        .latest_independent_verification_time_for_worker(local_agent_id)
        .ok()
        .flatten()
        .map(|verified_at| now.saturating_sub(verified_at));
    if latest_independent_verification_age_ms
        .is_some_and(|age| age < VERIFIER_LIVENESS_STALE_AFTER.as_millis() as u64)
    {
        return;
    }

    if now.saturating_sub(*last_verifier_liveness_discovery_ms)
        < VERIFIER_LIVENESS_DISCOVERY_INTERVAL.as_millis() as u64
    {
        return;
    }

    *last_verifier_liveness_discovery_ms = now;
    *rendezvous_cookie = None;
    ensure_infrastructure_connections(swarm, app, state, store, network);
    discover_rendezvous(swarm, network.rendezvous.as_ref(), &network.namespace, None);

    let _ = app.emit(
        "audit:verifier_liveness_resync",
        serde_json::json!({
            "pendingReceipts": pending_receipts,
            "oldestPendingAgeMs": oldest_pending_age_ms,
            "latestIndependentVerificationAgeMs": latest_independent_verification_age_ms,
            "knownPeers": target_peers(state, None).len(),
        }),
    );
}

fn handle_labor_inventory_request(
    swarm: &mut libp2p::Swarm<AgentBehaviour>,
    app: &AppHandle,
    state: &P2pState,
    store: &AtpStore,
    keypair: &identity::Keypair,
    local_agent_id: &str,
    peer_id: PeerId,
    remote_inventory: LaborInventory,
    outbound: &mut HashMap<OutboundRequestId, PendingOutbound>,
) -> LaborInventoryResponse {
    if remote_inventory.testnet_id != ATP_STORE_TESTNET_ID {
        return LaborInventoryResponse {
            accepted: false,
            testnet_id: ATP_STORE_TESTNET_ID.to_string(),
            app_version: labor_wire_app_version(),
            capabilities: labor_wire_capabilities(),
            reason: Some(format!(
                "peer testnet {} does not match {}",
                remote_inventory.testnet_id, ATP_STORE_TESTNET_ID
            )),
            ..Default::default()
        };
    }
    if !is_labor_wire_compatible_app_version(&remote_inventory.app_version) {
        return LaborInventoryResponse {
            accepted: false,
            testnet_id: ATP_STORE_TESTNET_ID.to_string(),
            app_version: labor_wire_app_version(),
            capabilities: labor_wire_capabilities(),
            reason: Some(format!(
                "peer app version {} does not match {}",
                if remote_inventory.app_version.is_empty() {
                    "unknown"
                } else {
                    remote_inventory.app_version.as_str()
                },
                LABOR_WIRE_COMPAT_APP_VERSION
            )),
            ..Default::default()
        };
    }
    if !has_labor_capability(
        &remote_inventory.capabilities,
        LABOR_CAPABILITY_SPARSE_INVENTORY_V3,
    ) {
        return LaborInventoryResponse {
            accepted: false,
            testnet_id: ATP_STORE_TESTNET_ID.to_string(),
            app_version: labor_wire_app_version(),
            capabilities: labor_wire_capabilities(),
            reason: Some(format!(
                "peer lacks required labor capability {LABOR_CAPABILITY_SPARSE_INVENTORY_V3}"
            )),
            ..Default::default()
        };
    }

    let local_inventory = match wire_labor_inventory(store, local_agent_id) {
        Ok(inventory) => inventory,
        Err(reason) => {
            return LaborInventoryResponse {
                accepted: false,
                testnet_id: ATP_STORE_TESTNET_ID.to_string(),
                app_version: labor_wire_app_version(),
                capabilities: labor_wire_capabilities(),
                reason: Some(reason),
                ..Default::default()
            };
        }
    };

    let peer_missing_campaigns =
        missing_from_remote(&local_inventory.campaigns, &remote_inventory.campaigns);
    let peer_missing_claims =
        missing_from_remote(&local_inventory.claims, &remote_inventory.claims);
    let peer_missing_contributions = missing_from_remote(
        &local_inventory.contributions,
        &remote_inventory.contributions,
    );
    let peer_missing_verifications = missing_from_remote(
        &local_inventory.verifications,
        &remote_inventory.verifications,
    );

    let mut offered_contributions = remote_inventory.contributions.clone();
    merge_strings(
        &mut offered_contributions,
        remote_inventory.needs_verifier.clone(),
    );
    let missing = match store.missing_labor_object_ids(
        &remote_inventory.campaigns,
        &remote_inventory.claims,
        &offered_contributions,
        &remote_inventory.verifications,
    ) {
        Ok(missing) => missing,
        Err(reason) => {
            return LaborInventoryResponse {
                accepted: false,
                testnet_id: ATP_STORE_TESTNET_ID.to_string(),
                app_version: labor_wire_app_version(),
                capabilities: labor_wire_capabilities(),
                reason: Some(format!("labor inventory existence check failed: {reason}")),
                ..Default::default()
            };
        }
    };
    let missing_campaigns = missing.campaign_ids;
    let missing_claims = missing.claim_ids;
    let missing_contributions = missing.contribution_ids;
    let missing_verifications = missing.verification_ids;

    let _ = app.emit(
        "audit:labor_inventory_received",
        serde_json::json!({
            "peerId": peer_id.to_string(),
            "peerNeedsCampaigns": peer_missing_campaigns.len(),
            "peerNeedsClaims": peer_missing_claims.len(),
            "peerNeedsContributions": peer_missing_contributions.len(),
            "peerNeedsVerifications": peer_missing_verifications.len(),
            "localNeedsCampaigns": missing_campaigns.len(),
            "localNeedsClaims": missing_claims.len(),
            "localNeedsContributions": missing_contributions.len(),
            "localNeedsVerifications": missing_verifications.len(),
            "peerNeedsVerifier": remote_inventory.needs_verifier.len(),
        }),
    );

    if !remote_inventory.needs_verifier.is_empty() {
        verify_network_contributions(swarm, app, state, store, keypair, local_agent_id, outbound);
    }

    LaborInventoryResponse {
        accepted: true,
        testnet_id: ATP_STORE_TESTNET_ID.to_string(),
        app_version: labor_wire_app_version(),
        capabilities: labor_wire_capabilities(),
        missing_campaigns,
        missing_claims,
        missing_contributions,
        missing_verifications,
        reason: None,
    }
}

fn wire_labor_inventory(store: &AtpStore, local_agent_id: &str) -> Result<LaborInventory, String> {
    let inventory = store.audit_labor_inventory(local_agent_id, LABOR_INVENTORY_LIMIT)?;
    Ok(LaborInventory {
        testnet_id: ATP_STORE_TESTNET_ID.to_string(),
        app_version: labor_wire_app_version(),
        capabilities: labor_wire_capabilities(),
        campaigns: inventory.campaign_ids,
        claims: inventory.claim_ids,
        contributions: inventory.contribution_ids,
        verifications: inventory.verification_ids,
        needs_verifier: inventory.needs_verifier_contribution_ids,
    })
}

fn missing_from_remote(local_ids: &[String], remote_ids: &[String]) -> Vec<String> {
    let remote = remote_ids.iter().collect::<HashSet<_>>();
    local_ids
        .iter()
        .filter(|id| !remote.contains(id))
        .take(LABOR_INVENTORY_LIMIT)
        .cloned()
        .collect()
}

fn send_labor_inventory_to_peer(
    swarm: &mut libp2p::Swarm<AgentBehaviour>,
    state: &P2pState,
    store: &AtpStore,
    local_agent_id: &str,
    peer_id: &PeerId,
    outbound: &mut HashMap<OutboundRequestId, PendingOutbound>,
) {
    if outbound
        .values()
        .filter(|pending| pending.peer_id() == peer_id)
        .count()
        >= MAX_BULK_OUTBOUND_REQUESTS_PER_PEER
    {
        return;
    }
    let Ok(inventory) = wire_labor_inventory(store, local_agent_id) else {
        return;
    };
    send_wire_request_to_peer(
        swarm,
        state,
        outbound,
        WireRequest::LaborInventory(inventory),
        PendingOutbound::LaborInventory {
            peer_id: peer_id.clone(),
        },
    );
}

fn broadcast_labor_inventory(
    swarm: &mut libp2p::Swarm<AgentBehaviour>,
    state: &P2pState,
    store: &AtpStore,
    local_agent_id: &str,
    outbound: &mut HashMap<OutboundRequestId, PendingOutbound>,
) {
    for peer_id in target_peers(state, None) {
        send_labor_inventory_to_peer(swarm, state, store, local_agent_id, &peer_id, outbound);
    }
}

/// Retain items in order while their cumulative serialized size stays within
/// `budget`. At least one item is always retained (so a single oversized object
/// never stalls the sync forever), and the bytes actually consumed are returned
/// so a caller can chain a second capped fill from the remaining budget.
fn cap_serialized_items<T: serde::Serialize>(items: Vec<T>, budget: usize) -> (Vec<T>, usize) {
    let mut used = 0usize;
    let mut kept = Vec::new();
    for item in items {
        // +2 approximates the JSON array separator/overhead per element.
        let size = serde_json::to_string(&item)
            .map(|json| json.len())
            .unwrap_or(0)
            + 2;
        if !kept.is_empty() && used + size > budget {
            break;
        }
        used += size;
        kept.push(item);
    }
    (kept, used)
}

fn build_labor_object_bundle(store: &AtpStore, request: LaborObjectRequest) -> LaborObjectBundle {
    if request.testnet_id != ATP_STORE_TESTNET_ID
        || !is_labor_wire_compatible_app_version(&request.app_version)
        || !has_labor_capability(&request.capabilities, LABOR_CAPABILITY_SPARSE_INVENTORY_V3)
    {
        return LaborObjectBundle {
            testnet_id: ATP_STORE_TESTNET_ID.to_string(),
            app_version: labor_wire_app_version(),
            capabilities: labor_wire_capabilities(),
            ..Default::default()
        };
    }

    let mut contributions = store
        .contributions_by_ids(&request.contribution_ids)
        .unwrap_or_default();
    let verification_contributions = store
        .contributions_for_verifications(&request.verification_ids)
        .unwrap_or_default();
    merge_contributions(&mut contributions, verification_contributions);

    let verifications = store
        .verification_bundles_by_ids(&request.verification_ids)
        .unwrap_or_default()
        .into_iter()
        .map(|(verification, allocations)| VerificationBundleWire {
            verification,
            allocations,
        })
        .collect::<Vec<_>>();

    // Bound the heavy payload so the serialized response stays under the wire
    // budget. Claims and campaigns are derived from the retained set only, so
    // the bundle is internally consistent, and any dropped object is requested
    // again on the next sparse-inventory round.
    let (contributions, used) = cap_serialized_items(contributions, MAX_LABOR_BUNDLE_BYTES);
    let (verifications, _) =
        cap_serialized_items(verifications, MAX_LABOR_BUNDLE_BYTES.saturating_sub(used));

    let mut campaign_ids = request.campaign_ids;
    let mut claims = store
        .work_unit_claims_by_ids(&request.claim_ids)
        .unwrap_or_default();
    for claim in &claims {
        campaign_ids.push(claim.campaign_id.clone());
    }
    let contribution_claims = store
        .claims_for_contributions(&contributions)
        .unwrap_or_default();
    merge_claims(&mut claims, contribution_claims);
    for contribution in &contributions {
        campaign_ids.push(contribution.campaign_id.clone());
    }
    for verification in &verifications {
        campaign_ids.push(verification.verification.campaign_id.clone());
    }

    dedupe_strings(&mut campaign_ids);
    let campaigns = store.campaigns_by_ids(&campaign_ids).unwrap_or_default();

    LaborObjectBundle {
        testnet_id: ATP_STORE_TESTNET_ID.to_string(),
        app_version: labor_wire_app_version(),
        capabilities: labor_wire_capabilities(),
        campaigns,
        claims,
        contributions,
        verifications,
    }
}

fn ingest_labor_object_bundle(
    app: &AppHandle,
    store: &AtpStore,
    peer_id: Option<&str>,
    bundle: LaborObjectBundle,
) -> LaborObjectBundleResponse {
    if bundle.testnet_id != ATP_STORE_TESTNET_ID
        || !is_labor_wire_compatible_app_version(&bundle.app_version)
        || !has_labor_capability(&bundle.capabilities, LABOR_CAPABILITY_SPARSE_INVENTORY_V3)
    {
        let reason = if bundle.testnet_id != ATP_STORE_TESTNET_ID {
            format!(
                "peer testnet {} does not match {}",
                bundle.testnet_id, ATP_STORE_TESTNET_ID
            )
        } else if !is_labor_wire_compatible_app_version(&bundle.app_version) {
            format!(
                "peer app version {} does not match {}",
                if bundle.app_version.is_empty() {
                    "unknown"
                } else {
                    bundle.app_version.as_str()
                },
                LABOR_WIRE_COMPAT_APP_VERSION
            )
        } else {
            format!("peer lacks required labor capability {LABOR_CAPABILITY_SPARSE_INVENTORY_V3}")
        };
        let _ = store.record_labor_event(
            "labor_object_bundle_rejected",
            peer_id,
            Some("bundle"),
            None,
            false,
            Some(reason.as_str()),
            &serde_json::json!({
                "testnetId": bundle.testnet_id,
                "appVersion": bundle.app_version,
                "capabilities": bundle.capabilities,
            }),
        );
        return LaborObjectBundleResponse {
            accepted: false,
            testnet_id: ATP_STORE_TESTNET_ID.to_string(),
            app_version: labor_wire_app_version(),
            capabilities: labor_wire_capabilities(),
            reason: Some(reason),
            ..Default::default()
        };
    }

    let mut response = LaborObjectBundleResponse {
        accepted: true,
        testnet_id: ATP_STORE_TESTNET_ID.to_string(),
        app_version: labor_wire_app_version(),
        capabilities: labor_wire_capabilities(),
        ..Default::default()
    };
    let mut duplicate_skipped = 0usize;
    let mut superseded_skipped = 0usize;

    for campaign in bundle.campaigns {
        match store.upsert_protocol_campaign(&campaign) {
            Ok(_) => response.campaigns += 1,
            Err(reason) if is_labor_dependency_error(&reason) => {
                queue_pending_labor_object(
                    store,
                    "campaign",
                    &campaign.campaign_id,
                    &campaign,
                    &reason,
                );
                response.queued += 1;
            }
            Err(reason) => response.reason = Some(reason),
        }
    }
    for claim in bundle.claims {
        match record_work_unit_claim_for_sync(store, &claim) {
            Ok(_) => response.claims += 1,
            Err(reason) if is_labor_dependency_error(&reason) => {
                queue_pending_labor_object(store, "claim", &claim.claim_id, &claim, &reason);
                response.queued += 1;
            }
            Err(reason) => response.reason = Some(reason),
        }
    }
    for contribution in bundle.contributions {
        match store.contribution_preflight_status(&contribution) {
            Ok(LaborObjectPreflight::New) => {}
            Ok(LaborObjectPreflight::Duplicate(_reason)) => {
                duplicate_skipped += 1;
                response.skipped += 1;
                continue;
            }
            Ok(LaborObjectPreflight::Superseded(_reason)) => {
                superseded_skipped += 1;
                response.skipped += 1;
                continue;
            }
            Err(reason) => {
                response.reason = Some(reason);
                continue;
            }
        }
        match store.record_network_contribution(&contribution) {
            Ok(_) => response.contributions += 1,
            Err(reason) if is_labor_dependency_error(&reason) => {
                queue_pending_labor_object(
                    store,
                    "contribution",
                    &contribution.contribution_id,
                    &contribution,
                    &reason,
                );
                response.queued += 1;
            }
            Err(reason) => response.reason = Some(reason),
        }
    }
    for verification_bundle in bundle.verifications {
        match store.verification_bundle_preflight_status(&verification_bundle.verification) {
            Ok(LaborObjectPreflight::New) => {}
            Ok(LaborObjectPreflight::Duplicate(_reason)) => {
                duplicate_skipped += 1;
                response.skipped += 1;
                continue;
            }
            Ok(LaborObjectPreflight::Superseded(_reason)) => {
                superseded_skipped += 1;
                response.skipped += 1;
                continue;
            }
            Err(reason) => {
                response.reason = Some(reason);
                continue;
            }
        }
        match store.record_verification_bundle(
            &verification_bundle.verification,
            &verification_bundle.allocations,
        ) {
            Ok(_) => response.verifications += 1,
            Err(reason) if is_labor_dependency_error(&reason) => {
                queue_pending_labor_object(
                    store,
                    "verification",
                    &verification_bundle.verification.verification_id,
                    &verification_bundle,
                    &reason,
                );
                response.queued += 1;
            }
            Err(reason) => response.reason = Some(reason),
        }
    }

    if response.skipped > 0 {
        let _ = store.record_labor_event(
            "labor_object_bundle_duplicate_skipped",
            peer_id,
            Some("bundle"),
            None,
            true,
            None,
            &serde_json::json!({
                "skipped": response.skipped,
                "duplicates": duplicate_skipped,
                "superseded": superseded_skipped,
            }),
        );
    }
    let changed = response.campaigns
        + response.claims
        + response.contributions
        + response.verifications
        + response.queued
        > 0;
    if changed {
        retry_pending_labor_objects(app, store);
        let _ = app.emit("audit:labor_changed", ());
    }
    let _ = store.record_labor_event(
        "labor_object_bundle_ingested",
        peer_id,
        Some("bundle"),
        None,
        response.accepted,
        response.reason.as_deref(),
        &serde_json::json!({
            "campaigns": response.campaigns,
            "claims": response.claims,
            "contributions": response.contributions,
            "verifications": response.verifications,
            "queued": response.queued,
            "skipped": response.skipped,
            "duplicateSkipped": duplicate_skipped,
            "supersededSkipped": superseded_skipped,
        }),
    );
    response
}

fn merge_claims(target: &mut Vec<AuditWorkUnitClaim>, incoming: Vec<AuditWorkUnitClaim>) {
    for claim in incoming {
        if !target
            .iter()
            .any(|existing| existing.claim_id == claim.claim_id)
        {
            target.push(claim);
        }
    }
}

fn merge_contributions(target: &mut Vec<NodeContribution>, incoming: Vec<NodeContribution>) {
    for contribution in incoming {
        if !target
            .iter()
            .any(|existing| existing.contribution_id == contribution.contribution_id)
        {
            target.push(contribution);
        }
    }
}

fn dedupe_strings(values: &mut Vec<String>) {
    let mut seen = HashSet::new();
    values.retain(|value| seen.insert(value.clone()));
}

fn merge_strings(target: &mut Vec<String>, incoming: Vec<String>) {
    for value in incoming {
        if !target.iter().any(|existing| existing == &value) {
            target.push(value);
        }
    }
    target.truncate(LABOR_INVENTORY_LIMIT);
}

fn queue_pending_labor_object<T: Serialize>(
    store: &AtpStore,
    object_kind: &str,
    object_id: &str,
    object: &T,
    reason: &str,
) {
    let Ok(object_json) = serde_json::to_string(object) else {
        return;
    };
    let _ = store.queue_pending_labor_object(object_kind, object_id, &object_json, reason);
}

fn record_work_unit_claim_for_sync(
    store: &AtpStore,
    claim: &AuditWorkUnitClaim,
) -> Result<AuditWorkUnitClaim, String> {
    match store.record_work_unit_claim(claim) {
        Ok(recorded) => Ok(recorded),
        Err(reason) if is_historical_claim_error(&reason) => {
            store.record_historical_work_unit_claim(claim)
        }
        Err(reason) => Err(reason),
    }
}

fn retry_pending_labor_objects(app: &AppHandle, store: &AtpStore) {
    let mut settled_total = 0usize;
    for _ in 0..3 {
        let Ok(objects) = store.pending_labor_objects(64) else {
            return;
        };
        if objects.is_empty() {
            break;
        }
        let mut settled_this_pass = 0usize;
        for object in objects {
            match retry_pending_labor_object(store, &object.object_kind, &object.object_json) {
                Ok(()) => {
                    let _ = store
                        .mark_pending_labor_object_settled(&object.object_kind, &object.object_id);
                    settled_this_pass += 1;
                }
                Err(reason) if is_labor_dependency_error(&reason) => {
                    let _ = store.refresh_pending_labor_object(
                        &object.object_kind,
                        &object.object_id,
                        &reason,
                    );
                }
                Err(reason) => {
                    let _ = store.mark_pending_labor_object_rejected(
                        &object.object_kind,
                        &object.object_id,
                        &reason,
                    );
                }
            }
        }
        settled_total += settled_this_pass;
        if settled_this_pass == 0 {
            break;
        }
    }
    if settled_total > 0 {
        let _ = app.emit("audit:labor_changed", ());
        let _ = app.emit(
            "audit:labor_pending_objects_settled",
            serde_json::json!({ "settled": settled_total }),
        );
    }
}

fn retry_pending_labor_object(
    store: &AtpStore,
    object_kind: &str,
    object_json: &str,
) -> Result<(), String> {
    match object_kind {
        "campaign" => {
            let campaign: ProtocolAuditCampaign =
                serde_json::from_str(object_json).map_err(|error| error.to_string())?;
            store.upsert_protocol_campaign(&campaign)?;
            Ok(())
        }
        "claim" => {
            let claim: AuditWorkUnitClaim =
                serde_json::from_str(object_json).map_err(|error| error.to_string())?;
            record_work_unit_claim_for_sync(store, &claim)?;
            Ok(())
        }
        "contribution" => {
            let contribution: NodeContribution =
                serde_json::from_str(object_json).map_err(|error| error.to_string())?;
            store.record_network_contribution(&contribution)?;
            Ok(())
        }
        "verification" => {
            let bundle: VerificationBundleWire =
                serde_json::from_str(object_json).map_err(|error| error.to_string())?;
            store.record_verification_bundle(&bundle.verification, &bundle.allocations)?;
            Ok(())
        }
        _ => Err(format!("unknown pending labor object kind: {object_kind}")),
    }
}

fn is_labor_dependency_error(reason: &str) -> bool {
    reason.contains("not known locally")
        || reason.contains("must be claimed by this worker")
        || reason.contains("Query returned no rows")
        || reason.contains("FOREIGN KEY constraint failed")
}

fn is_historical_claim_error(reason: &str) -> bool {
    reason.contains("work unit claim has expired")
        || reason.contains("work unit already has submitted or reviewed work")
}

fn push_labor_objects_to_peer(
    swarm: &mut libp2p::Swarm<AgentBehaviour>,
    state: &P2pState,
    store: &AtpStore,
    peer_id: &PeerId,
    campaign_ids: &[String],
    claim_ids: &[String],
    contribution_ids: &[String],
    verification_ids: &[String],
    outbound: &mut HashMap<OutboundRequestId, PendingOutbound>,
) {
    if campaign_ids.is_empty()
        && claim_ids.is_empty()
        && contribution_ids.is_empty()
        && verification_ids.is_empty()
    {
        return;
    }
    if outbound
        .values()
        .filter(|pending| pending.peer_id() == peer_id)
        .count()
        >= MAX_BULK_OUTBOUND_REQUESTS_PER_PEER
    {
        return;
    }
    let request = LaborObjectRequest {
        testnet_id: ATP_STORE_TESTNET_ID.to_string(),
        app_version: labor_wire_app_version(),
        capabilities: labor_wire_capabilities(),
        campaign_ids: campaign_ids.to_vec(),
        claim_ids: claim_ids.to_vec(),
        contribution_ids: contribution_ids.to_vec(),
        verification_ids: verification_ids.to_vec(),
    };
    let bundle = build_labor_object_bundle(store, request);
    if bundle.campaigns.is_empty()
        && bundle.claims.is_empty()
        && bundle.contributions.is_empty()
        && bundle.verifications.is_empty()
    {
        return;
    }
    send_wire_request_to_peer(
        swarm,
        state,
        outbound,
        WireRequest::LaborObjectBundle(bundle),
        PendingOutbound::LaborObjectBundle {
            peer_id: peer_id.clone(),
            silent: true,
        },
    );
}

fn maybe_repair_stale_receipts(
    swarm: &mut libp2p::Swarm<AgentBehaviour>,
    app: &AppHandle,
    state: &P2pState,
    store: &AtpStore,
    last_stale_receipt_repair_ms: &mut u64,
    outbound: &mut HashMap<OutboundRequestId, PendingOutbound>,
) {
    let now = now_millis();
    if now.saturating_sub(*last_stale_receipt_repair_ms)
        < STALE_RECEIPT_REPAIR_INTERVAL.as_millis() as u64
    {
        return;
    }
    if outbound.len() >= MAX_OUTBOUND_REPAIR_BACKLOG {
        let _ = app.emit(
            "audit:stale_receipt_repair_backpressure",
            serde_json::json!({
                "outboundRequests": outbound.len(),
                "limit": MAX_OUTBOUND_REPAIR_BACKLOG,
            }),
        );
        return;
    }

    let repairs = match store.stale_unverified_contributions_with_claims(
        STALE_RECEIPT_REPAIR_AFTER.as_millis() as u64,
        STALE_RECEIPT_REPAIR_LIMIT,
    ) {
        Ok(repairs) => repairs,
        Err(reason) => {
            let _ = app.emit(
                "atp:delivery_failed",
                serde_json::json!({
                    "reason": format!("stale receipt repair failed: {reason}"),
                }),
            );
            return;
        }
    };
    let receipt_count = repairs.len();
    if receipt_count == 0 {
        return;
    }
    *last_stale_receipt_repair_ms = now;
    let mut bundle = LaborObjectBundle {
        testnet_id: ATP_STORE_TESTNET_ID.to_string(),
        app_version: labor_wire_app_version(),
        capabilities: labor_wire_capabilities(),
        campaigns: Vec::new(),
        claims: Vec::new(),
        contributions: Vec::new(),
        verifications: Vec::new(),
    };
    for repair in repairs {
        if !bundle
            .campaigns
            .iter()
            .any(|campaign| campaign.campaign_id == repair.campaign.campaign_id)
        {
            bundle.campaigns.push(repair.campaign);
        }
        if let Some(claim) = repair.claim {
            merge_claims(&mut bundle.claims, vec![claim]);
        }
        merge_contributions(&mut bundle.contributions, vec![repair.contribution]);
    }
    let claims = bundle.claims.len();
    let receipts = bundle.contributions.len();
    broadcast_labor_object_bundle(swarm, state, bundle, true, outbound);
    let _ = app.emit(
        "audit:stale_receipts_rebroadcast",
        serde_json::json!({
            "receipts": receipts,
            "claims": claims,
        }),
    );
}

fn verify_network_contributions(
    swarm: &mut libp2p::Swarm<AgentBehaviour>,
    app: &AppHandle,
    state: &P2pState,
    store: &AtpStore,
    keypair: &identity::Keypair,
    local_agent_id: &str,
    outbound: &mut HashMap<OutboundRequestId, PendingOutbound>,
) {
    let candidates =
        match store.network_verification_candidates(local_agent_id, LABOR_AUTO_VERIFY_SCAN_LIMIT) {
            Ok(candidates) => candidates,
            Err(reason) => {
                let _ = app.emit(
                    "atp:delivery_failed",
                    serde_json::json!({
                        "reason": format!("network verifier scan failed: {reason}"),
                    }),
                );
                return;
            }
        };

    for contribution in candidates.into_iter().take(LABOR_AUTO_VERIFY_LIMIT) {
        let evidence_ref = format!("contribution:{}", contribution.receipt_hash);
        let evidence_hash = crate::audit_labor::sha256_ref(evidence_ref.as_bytes());
        let evidence_size = evidence_ref.len() as u64;
        let verification = match signed_autonomous_finality_verification(
            keypair,
            &contribution,
            "accepted".to_string(),
            "AUTONOMOUS_FINALITY_ACCEPTED".to_string(),
            "Independent network verifier accepted the signed Cognition Proof and receipt for immediate ATP finality.".to_string(),
            vec![VerificationEvidence {
                label: "signed Cognition Proof receipt".to_string(),
                reference: evidence_ref,
            }],
            vec![ContributionArtifact {
                path: "network-verification.md".to_string(),
                media_type: "text/markdown".to_string(),
                sha256: evidence_hash,
                size_bytes: evidence_size,
            }],
        ) {
            Ok(verification) => verification,
            Err(reason) => {
                let _ = store.record_labor_event(
                    "network_verification_failed",
                    None,
                    Some("contribution"),
                    Some(contribution.contribution_id.as_str()),
                    false,
                    Some(reason.as_str()),
                    &serde_json::json!({
                        "campaignId": contribution.campaign_id.clone(),
                        "contributionId": contribution.contribution_id.clone(),
                        "reason": format!("network verifier signing failed: {reason}"),
                    }),
                );
                let _ = app.emit(
                    "atp:delivery_failed",
                    serde_json::json!({"reason": format!("network verifier signing failed: {reason}")}),
                );
                continue;
            }
        };
        let allocations = match store.record_verification(&verification) {
            Ok(allocations) => allocations,
            Err(reason) => {
                let _ = store.record_labor_event(
                    "network_verification_failed",
                    None,
                    Some("contribution"),
                    Some(contribution.contribution_id.as_str()),
                    false,
                    Some(reason.as_str()),
                    &serde_json::json!({
                        "campaignId": contribution.campaign_id.clone(),
                        "contributionId": contribution.contribution_id.clone(),
                        "reason": format!("network verification failed: {reason}"),
                    }),
                );
                let _ = app.emit(
                    "atp:delivery_failed",
                    serde_json::json!({
                        "campaignId": contribution.campaign_id,
                        "contributionId": contribution.contribution_id,
                        "reason": format!("network verification failed: {reason}"),
                    }),
                );
                continue;
            }
        };
        if !allocations
            .iter()
            .all(|allocation| allocation.verification_id == verification.verification_id)
        {
            continue;
        }
        let credit_total = allocations
            .iter()
            .map(|allocation| allocation.total)
            .sum::<u32>();
        let _ = store.record_labor_event(
            "network_verification_issued",
            None,
            Some("verification"),
            Some(verification.verification_id.as_str()),
            true,
            None,
            &serde_json::json!({
                "campaignId": verification.campaign_id.clone(),
                "verificationId": verification.verification_id.clone(),
                "targetContributionId": verification.target_contribution_id.clone(),
                "creditTotal": credit_total,
            }),
        );
        broadcast_verification_result(
            swarm,
            state,
            verification.clone(),
            allocations,
            false,
            outbound,
        );
        let _ = app.emit("audit:labor_changed", ());
        let _ = app.emit(
            "audit:network_verification_issued",
            serde_json::json!({
                "campaignId": verification.campaign_id,
                "verificationId": verification.verification_id,
                "targetContributionId": verification.target_contribution_id,
                "creditTotal": credit_total,
            }),
        );
    }
}

fn send_envelope(
    swarm: &mut libp2p::Swarm<AgentBehaviour>,
    state: &P2pState,
    envelope: AtpEnvelope,
    outbound: &mut HashMap<OutboundRequestId, PendingOutbound>,
) {
    let peers = target_peers(state, envelope.audience.as_deref());
    let hash = crate::atp::event_hash(&envelope).unwrap_or_default();
    for peer_id in peers {
        send_wire_request_to_peer(
            swarm,
            state,
            outbound,
            WireRequest::Envelope(envelope.clone()),
            PendingOutbound::Envelope {
                peer_id,
                event_hash: hash.clone(),
            },
        );
    }
}

fn send_campaign(
    swarm: &mut libp2p::Swarm<AgentBehaviour>,
    state: &P2pState,
    campaign: ProtocolAuditCampaign,
    outbound: &mut HashMap<OutboundRequestId, PendingOutbound>,
) {
    broadcast_campaign(swarm, state, campaign, false, outbound);
}

fn broadcast_campaign(
    swarm: &mut libp2p::Swarm<AgentBehaviour>,
    state: &P2pState,
    campaign: ProtocolAuditCampaign,
    silent: bool,
    outbound: &mut HashMap<OutboundRequestId, PendingOutbound>,
) {
    for peer_id in target_peers(state, None) {
        send_wire_request_to_peer(
            swarm,
            state,
            outbound,
            WireRequest::Campaign(campaign.clone()),
            PendingOutbound::Campaign {
                peer_id,
                campaign_id: campaign.campaign_id.clone(),
                silent,
            },
        );
    }
}

fn send_work_unit_claim(
    swarm: &mut libp2p::Swarm<AgentBehaviour>,
    state: &P2pState,
    claim: AuditWorkUnitClaim,
    audience: &str,
    silent: bool,
    outbound: &mut HashMap<OutboundRequestId, PendingOutbound>,
) {
    for peer_id in target_peers(state, Some(audience)) {
        send_wire_request_to_peer(
            swarm,
            state,
            outbound,
            WireRequest::WorkUnitClaim(claim.clone()),
            PendingOutbound::WorkUnitClaim {
                peer_id,
                campaign_id: claim.campaign_id.clone(),
                work_unit_id: claim.work_unit_id.clone(),
                claim_id: claim.claim_id.clone(),
                silent,
            },
        );
    }
}

fn send_execution_result(
    swarm: &mut libp2p::Swarm<AgentBehaviour>,
    state: &P2pState,
    result: SignedExecutionResult,
    audience: &str,
    outbound: &mut HashMap<OutboundRequestId, PendingOutbound>,
) {
    let peers = target_peers(state, Some(audience));
    for peer_id in peers {
        send_wire_request_to_peer(
            swarm,
            state,
            outbound,
            WireRequest::ExecutionResult(result.clone()),
            PendingOutbound::ExecutionResult {
                peer_id,
                transaction_id: result.transaction_id.clone(),
                result_hash: result.result_hash.clone(),
            },
        );
    }
}

fn send_contribution(
    swarm: &mut libp2p::Swarm<AgentBehaviour>,
    state: &P2pState,
    contribution: NodeContribution,
    audience: &str,
    outbound: &mut HashMap<OutboundRequestId, PendingOutbound>,
) {
    let peers = target_peers(state, Some(audience));
    for peer_id in peers {
        send_wire_request_to_peer(
            swarm,
            state,
            outbound,
            WireRequest::Contribution(contribution.clone()),
            PendingOutbound::Contribution {
                peer_id,
                campaign_id: contribution.campaign_id.clone(),
                contribution_id: contribution.contribution_id.clone(),
                receipt_hash: contribution.receipt_hash.clone(),
                silent: false,
            },
        );
    }
}

fn send_verification_result(
    swarm: &mut libp2p::Swarm<AgentBehaviour>,
    state: &P2pState,
    verification: VerificationResult,
    allocations: Vec<CreditAllocation>,
    audience: &str,
    outbound: &mut HashMap<OutboundRequestId, PendingOutbound>,
) {
    let peers = target_peers(state, Some(audience));
    let credit_total = allocations
        .iter()
        .map(|allocation| allocation.total)
        .sum::<u32>();
    for peer_id in peers {
        send_wire_request_to_peer(
            swarm,
            state,
            outbound,
            WireRequest::VerificationResult {
                verification: verification.clone(),
                allocations: allocations.clone(),
            },
            PendingOutbound::VerificationResult {
                peer_id,
                campaign_id: verification.campaign_id.clone(),
                verification_id: verification.verification_id.clone(),
                credit_total,
                silent: false,
            },
        );
    }
}

fn broadcast_verification_result(
    swarm: &mut libp2p::Swarm<AgentBehaviour>,
    state: &P2pState,
    verification: VerificationResult,
    allocations: Vec<CreditAllocation>,
    silent: bool,
    outbound: &mut HashMap<OutboundRequestId, PendingOutbound>,
) {
    let credit_total = allocations
        .iter()
        .map(|allocation| allocation.total)
        .sum::<u32>();
    for peer_id in target_peers(state, None) {
        send_wire_request_to_peer(
            swarm,
            state,
            outbound,
            WireRequest::VerificationResult {
                verification: verification.clone(),
                allocations: allocations.clone(),
            },
            PendingOutbound::VerificationResult {
                peer_id,
                campaign_id: verification.campaign_id.clone(),
                verification_id: verification.verification_id.clone(),
                credit_total,
                silent,
            },
        );
    }
}

fn broadcast_labor_object_bundle(
    swarm: &mut libp2p::Swarm<AgentBehaviour>,
    state: &P2pState,
    bundle: LaborObjectBundle,
    silent: bool,
    outbound: &mut HashMap<OutboundRequestId, PendingOutbound>,
) {
    for peer_id in target_peers(state, None) {
        send_wire_request_to_peer(
            swarm,
            state,
            outbound,
            WireRequest::LaborObjectBundle(bundle.clone()),
            PendingOutbound::LaborObjectBundle { peer_id, silent },
        );
    }
}

fn target_peers(state: &P2pState, audience: Option<&str>) -> Vec<PeerId> {
    let now = now_millis();
    state
        .inner
        .lock()
        .map(|inner| {
            let mut peers = inner
                .peers
                .values()
                .filter(|peer| peer.cooldown_until <= now)
                .filter_map(|peer| {
                    peer.peer_id
                        .parse::<PeerId>()
                        .ok()
                        .map(|peer_id| (peer_id, peer.last_seen))
                })
                .filter(|(peer, _)| {
                    audience.is_none_or(|audience| audience == format!("urn:libp2p:{peer}"))
                })
                .collect::<Vec<_>>();
            peers.sort_by(|left, right| right.1.cmp(&left.1));
            peers
                .into_iter()
                .take(if audience.is_some() {
                    usize::MAX
                } else {
                    MAX_BROADCAST_PEERS_PER_TICK
                })
                .map(|(peer, _)| peer)
                .collect()
        })
        .unwrap_or_default()
}

fn touch_peer(state: &P2pState, peer_id: PeerId) {
    if let Ok(mut inner) = state.inner.lock() {
        let now = now_millis();
        inner
            .peers
            .entry(peer_id.to_string())
            .and_modify(|peer| {
                peer.last_seen = now;
            })
            .or_insert_with(|| PeerInfo {
                peer_id: peer_id.to_string(),
                last_seen: now,
                failure_streak: 0,
                cooldown_until: 0,
            });
    }
}

fn mark_peer_success(state: &P2pState, peer_id: PeerId) {
    if let Ok(mut inner) = state.inner.lock() {
        let now = now_millis();
        inner
            .peers
            .entry(peer_id.to_string())
            .and_modify(|peer| {
                peer.last_seen = now;
                peer.failure_streak = 0;
                peer.cooldown_until = 0;
            })
            .or_insert_with(|| PeerInfo {
                peer_id: peer_id.to_string(),
                last_seen: now,
                failure_streak: 0,
                cooldown_until: 0,
            });
    }
}

fn mark_peer_dial_attempt(state: &P2pState, peer_id: PeerId) {
    if let Ok(mut inner) = state.inner.lock() {
        let now = now_millis();
        let cooldown_until = now.saturating_add(PEER_FAILURE_BASE_COOLDOWN_MS);
        inner
            .peers
            .entry(peer_id.to_string())
            .and_modify(|peer| {
                peer.last_seen = now;
                peer.cooldown_until = peer.cooldown_until.max(cooldown_until);
            })
            .or_insert_with(|| PeerInfo {
                peer_id: peer_id.to_string(),
                last_seen: now,
                failure_streak: 0,
                cooldown_until,
            });
    }
}

fn mark_peer_failure(state: &P2pState, peer_id: PeerId) {
    if let Ok(mut inner) = state.inner.lock() {
        let now = now_millis();
        inner
            .peers
            .entry(peer_id.to_string())
            .and_modify(|peer| {
                peer.last_seen = now;
                peer.failure_streak = peer.failure_streak.saturating_add(1);
                let multiplier = 1u64 << peer.failure_streak.min(4);
                let cooldown = PEER_FAILURE_BASE_COOLDOWN_MS
                    .saturating_mul(multiplier)
                    .min(PEER_FAILURE_MAX_COOLDOWN_MS);
                peer.cooldown_until = now.saturating_add(cooldown);
            })
            .or_insert_with(|| PeerInfo {
                peer_id: peer_id.to_string(),
                last_seen: now,
                failure_streak: 1,
                cooldown_until: now.saturating_add(PEER_FAILURE_BASE_COOLDOWN_MS),
            });
    }
}

async fn configured_network() -> Result<NetworkBootstrap, String> {
    let namespace_override = std::env::var("CYPHES_RENDEZVOUS_NAMESPACE")
        .ok()
        .filter(|value| !value.trim().is_empty());
    let runtime_relay = std::env::var("CYPHES_RELAY_ADDR")
        .ok()
        .filter(|value| !value.trim().is_empty());
    let runtime_rendezvous = std::env::var("CYPHES_RENDEZVOUS_ADDR")
        .ok()
        .filter(|value| !value.trim().is_empty());

    if runtime_relay.is_some() || runtime_rendezvous.is_some() {
        return build_network_bootstrap(
            runtime_relay,
            runtime_rendezvous,
            namespace_override,
            Some("environment".to_string()),
        );
    }

    let embedded_relay = option_env!("CYPHES_DEFAULT_RELAY_ADDR").map(ToString::to_string);
    let embedded_rendezvous =
        option_env!("CYPHES_DEFAULT_RENDEZVOUS_ADDR").map(ToString::to_string);
    if embedded_relay.is_some() || embedded_rendezvous.is_some() {
        return build_network_bootstrap(
            embedded_relay,
            embedded_rendezvous,
            namespace_override,
            Some("embedded release default".to_string()),
        );
    }

    if std::env::var("CYPHES_DISABLE_DEFAULT_NETWORK").as_deref() == Ok("1") {
        return build_network_bootstrap(None, None, namespace_override, None);
    }

    let config_url = std::env::var("CYPHES_NETWORK_CONFIG_URL")
        .unwrap_or_else(|_| DEFAULT_NETWORK_CONFIG_URL.to_string());
    let published = match fetch_published_network_config(&config_url).await {
        Ok(config) => build_network_bootstrap(
            config.relay_addr,
            config.rendezvous_addr,
            namespace_override.clone().or(config.rendezvous_namespace),
            Some(config_url),
        ),
        Err(error) => Err(error),
    };

    published
        .or_else(|_| embedded_network_bootstrap(namespace_override.clone()))
        .or_else(|_| build_network_bootstrap(None, None, namespace_override, None))
}

async fn fetch_published_network_config(url: &str) -> Result<PublishedNetworkConfig, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|error| error.to_string())?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;
    let body = response.text().await.map_err(|error| error.to_string())?;
    serde_json::from_str(&body).map_err(|error| error.to_string())
}

fn embedded_network_bootstrap(
    namespace_override: Option<String>,
) -> Result<NetworkBootstrap, String> {
    let config: PublishedNetworkConfig =
        serde_json::from_str(EMBEDDED_NETWORK_CONFIG_JSON).map_err(|error| error.to_string())?;
    build_network_bootstrap(
        config.relay_addr,
        config.rendezvous_addr,
        namespace_override.or(config.rendezvous_namespace),
        Some("embedded bootstrap manifest".to_string()),
    )
}

fn build_network_bootstrap(
    relay_addr: Option<String>,
    rendezvous_addr: Option<String>,
    namespace: Option<String>,
    source: Option<String>,
) -> Result<NetworkBootstrap, String> {
    let relay = relay_addr
        .as_deref()
        .map(|value| parse_infrastructure_target("relay", value))
        .transpose()?;
    let rendezvous = rendezvous_addr
        .as_deref()
        .or(relay_addr.as_deref())
        .map(|value| parse_infrastructure_target("rendezvous", value))
        .transpose()?;
    let namespace = rendezvous::Namespace::new(
        namespace.unwrap_or_else(|| DEFAULT_RENDEZVOUS_NAMESPACE.to_string()),
    )
    .map_err(|error| error.to_string())?;

    Ok(NetworkBootstrap {
        relay,
        rendezvous,
        namespace,
        source,
    })
}

fn parse_infrastructure_target(kind: &str, value: &str) -> Result<InfrastructureTarget, String> {
    let address = value
        .parse::<Multiaddr>()
        .map_err(|error| format!("{kind} address is invalid: {error}"))?;
    let peer_id = relay_peer_id(&address)
        .ok_or_else(|| format!("{kind} address must end with /p2p/PEER_ID"))?;
    Ok(InfrastructureTarget { peer_id, address })
}

fn dial_infrastructure(
    swarm: &mut libp2p::Swarm<AgentBehaviour>,
    network: &NetworkBootstrap,
) -> Result<(), String> {
    let mut addresses = Vec::<Multiaddr>::new();
    for target in [network.relay.as_ref(), network.rendezvous.as_ref()]
        .into_iter()
        .flatten()
    {
        if addresses.contains(&target.address) {
            continue;
        }
        swarm
            .dial(target.address.clone())
            .map_err(|error| format!("could not dial CYPHES infrastructure: {error}"))?;
        addresses.push(target.address.clone());
    }
    Ok(())
}

fn mark_infrastructure_activity(state: &P2pState, network: &NetworkBootstrap, peer_id: PeerId) {
    if !is_infrastructure_peer(network, peer_id) {
        return;
    }
    if let Ok(mut inner) = state.inner.lock() {
        inner.last_infrastructure_activity_ms = now_millis();
        if network
            .relay
            .as_ref()
            .is_some_and(|relay| relay.peer_id == peer_id)
        {
            inner.relay_connected = true;
        }
    }
}

fn clear_infrastructure_connection_state(
    state: &P2pState,
    network: &NetworkBootstrap,
    peer_id: PeerId,
) {
    if let Ok(mut inner) = state.inner.lock() {
        if network
            .relay
            .as_ref()
            .is_some_and(|relay| relay.peer_id == peer_id)
        {
            inner.relay_connected = false;
            inner.rendezvous_registered = false;
        }
        if network
            .rendezvous
            .as_ref()
            .is_some_and(|rendezvous| rendezvous.peer_id == peer_id)
        {
            inner.rendezvous_registered = false;
        }
    }
}

fn infrastructure_activity_is_stale(state: &P2pState) -> bool {
    state
        .inner
        .lock()
        .map(|inner| {
            now_millis().saturating_sub(inner.last_infrastructure_activity_ms)
                > INFRASTRUCTURE_ACTIVITY_STALE_AFTER.as_millis() as u64
        })
        .unwrap_or(true)
}

fn record_dial_failure(
    app: &AppHandle,
    store: &AtpStore,
    event_kind: &str,
    peer_id: Option<PeerId>,
    address: Option<&Multiaddr>,
    reason: &str,
) {
    let peer_string = peer_id.map(|peer| peer.to_string());
    let address_string = address.map(ToString::to_string);
    let _ = store.record_labor_event(
        event_kind,
        peer_string.as_deref(),
        Some("network_route"),
        address_string.as_deref().or(peer_string.as_deref()),
        false,
        Some(reason),
        &serde_json::json!({
            "peerId": peer_string,
            "address": address_string,
            "reason": reason,
        }),
    );
    let _ = app.emit(
        "p2p:connection_failed",
        serde_json::json!({
            "peerId": peer_string,
            "address": address_string,
            "reason": reason,
        }),
    );
}

fn dial_with_telemetry(
    swarm: &mut libp2p::Swarm<AgentBehaviour>,
    app: &AppHandle,
    store: &AtpStore,
    event_kind: &str,
    peer_id: Option<PeerId>,
    address: Multiaddr,
) -> bool {
    match swarm.dial(address.clone()) {
        Ok(()) => true,
        Err(error) => {
            record_dial_failure(
                app,
                store,
                event_kind,
                peer_id,
                Some(&address),
                error.to_string().as_str(),
            );
            false
        }
    }
}

fn is_relay_circuit_address(address: &Multiaddr) -> bool {
    address
        .iter()
        .any(|protocol| protocol == libp2p::multiaddr::Protocol::P2pCircuit)
}

fn clear_local_relay_circuit_address(
    swarm: &mut libp2p::Swarm<AgentBehaviour>,
    state: &P2pState,
    network: &NetworkBootstrap,
    local_peer_id: PeerId,
) {
    let Some(relay) = network.relay.as_ref() else {
        return;
    };
    let address = relay_circuit_address(relay, local_peer_id);
    swarm.remove_external_address(&address);
    let address_string = address.to_string();
    let local_peer_suffix = format!("/p2p-circuit/p2p/{local_peer_id}");
    if let Ok(mut inner) = state.inner.lock() {
        inner.listen_addrs.retain(|existing| {
            existing != &address_string && !existing.contains(&local_peer_suffix)
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn lose_relay_reservation(
    swarm: &mut libp2p::Swarm<AgentBehaviour>,
    app: &AppHandle,
    state: &P2pState,
    store: &AtpStore,
    network: &NetworkBootstrap,
    local_peer_id: PeerId,
    relay_reservation: &mut RelayReservationState,
    rendezvous_registration_started: &mut bool,
    rendezvous_cookie: &mut Option<rendezvous::Cookie>,
    reason: &str,
) {
    if let Some(listener_id) = relay_reservation.listener_id.take() {
        let _ = swarm.remove_listener(listener_id);
    }
    relay_reservation.reset();
    *rendezvous_registration_started = false;
    *rendezvous_cookie = None;
    clear_local_relay_circuit_address(swarm, state, network, local_peer_id);
    if let Ok(mut inner) = state.inner.lock() {
        inner.relay_connected = false;
        inner.rendezvous_registered = false;
    }

    let relay_peer_id = network
        .relay
        .as_ref()
        .map(|relay| relay.peer_id.to_string());
    let relay_address = network
        .relay
        .as_ref()
        .map(|relay| relay.address.to_string());
    let _ = store.record_labor_event(
        "relay_reservation_lost",
        relay_peer_id.as_deref(),
        Some("network_route"),
        relay_address.as_deref(),
        false,
        Some(reason),
        &serde_json::json!({
            "peerId": relay_peer_id,
            "address": relay_address,
            "reason": reason,
        }),
    );
    let _ = app.emit(
        "p2p:connection_failed",
        serde_json::json!({
            "peerId": relay_peer_id,
            "address": relay_address,
            "reason": reason,
        }),
    );
}

fn ensure_relay_reservation(
    swarm: &mut libp2p::Swarm<AgentBehaviour>,
    app: &AppHandle,
    state: &P2pState,
    store: &AtpStore,
    network: &NetworkBootstrap,
    local_peer_id: PeerId,
    relay_reservation: &mut RelayReservationState,
) {
    let Some(relay) = network.relay.as_ref() else {
        return;
    };
    if !swarm.is_connected(&relay.peer_id) {
        return;
    }
    if relay_reservation.confirmed {
        return;
    }
    let now_ms = now_millis();
    if relay_reservation.has_pending_request() && !relay_reservation.is_pending_stale(now_ms) {
        return;
    }
    if let Some(listener_id) = relay_reservation.listener_id.take() {
        let _ = swarm.remove_listener(listener_id);
        let peer_id = relay.peer_id.to_string();
        let address = relay.address.to_string();
        let _ = store.record_labor_event(
            "relay_reservation_retry",
            Some(&peer_id),
            Some("network_route"),
            Some(&address),
            false,
            Some("relay reservation request timed out before acceptance"),
            &serde_json::json!({
                "peerId": peer_id,
                "address": address,
                "retryAfterMs": RELAY_RESERVATION_RETRY_AFTER.as_millis(),
            }),
        );
    }
    relay_reservation.reset();

    clear_local_relay_circuit_address(swarm, state, network, local_peer_id);
    let mut circuit_addr = relay.address.clone();
    circuit_addr.push(libp2p::multiaddr::Protocol::P2pCircuit);
    match swarm.listen_on(circuit_addr.clone()) {
        Ok(listener_id) => {
            relay_reservation.listener_id = Some(listener_id);
            relay_reservation.requested_at_ms = Some(now_ms);
            relay_reservation.confirmed = false;
            let peer_id = relay.peer_id.to_string();
            let address = circuit_addr.to_string();
            let _ = store.record_labor_event(
                "relay_reservation_requested",
                Some(&peer_id),
                Some("network_route"),
                Some(&address),
                true,
                None,
                &serde_json::json!({
                    "peerId": peer_id,
                    "address": address,
                    "listenerId": format!("{listener_id:?}"),
                }),
            );
        }
        Err(error) => {
            relay_reservation.reset();
            record_dial_failure(
                app,
                store,
                "relay_reservation_failed",
                Some(relay.peer_id),
                Some(&relay.address),
                format!("could not reserve relay circuit: {error}").as_str(),
            );
        }
    }
}

fn ensure_infrastructure_connections(
    swarm: &mut libp2p::Swarm<AgentBehaviour>,
    app: &AppHandle,
    state: &P2pState,
    store: &AtpStore,
    network: &NetworkBootstrap,
) {
    let mut peers = Vec::<PeerId>::new();
    for target in [network.relay.as_ref(), network.rendezvous.as_ref()]
        .into_iter()
        .flatten()
    {
        if peers.contains(&target.peer_id) {
            continue;
        }
        if swarm.is_connected(&target.peer_id) {
            let recycle_pending = state
                .inner
                .lock()
                .map(|inner| {
                    inner
                        .infrastructure_recycle_pending
                        .contains(&target.peer_id.to_string())
                })
                .unwrap_or(false);
            if recycle_pending {
                peers.push(target.peer_id);
                continue;
            }
            if !infrastructure_activity_is_stale(state) {
                peers.push(target.peer_id);
                continue;
            }
            let reason = format!(
                "infrastructure peer has had no observable activity for {} seconds; recycling connection",
                INFRASTRUCTURE_ACTIVITY_STALE_AFTER.as_secs()
            );
            let _ = store.record_labor_event(
                "infrastructure_connection_recycled",
                Some(target.peer_id.to_string().as_str()),
                Some("network_route"),
                Some(target.address.to_string().as_str()),
                false,
                Some(reason.as_str()),
                &serde_json::json!({
                    "peerId": target.peer_id.to_string(),
                    "address": target.address.to_string(),
                    "reason": reason,
                }),
            );
            let _ = app.emit(
                "p2p:connection_failed",
                serde_json::json!({
                    "peerId": target.peer_id.to_string(),
                    "address": target.address.to_string(),
                    "reason": reason,
                }),
            );
            clear_infrastructure_connection_state(state, network, target.peer_id);
            if let Ok(mut inner) = state.inner.lock() {
                inner
                    .infrastructure_recycle_pending
                    .insert(target.peer_id.to_string());
            }
            let _ = swarm.disconnect_peer_id(target.peer_id);
            // ConnectionClosed clears the pending marker. Dialing in this same
            // poll races the closing transport and can strand the relay socket
            // in CLOSE_WAIT.
            peers.push(target.peer_id);
            continue;
        }
        dial_with_telemetry(
            swarm,
            app,
            store,
            "infrastructure_dial_failed",
            Some(target.peer_id),
            target.address.clone(),
        );
        peers.push(target.peer_id);
    }
}

fn relay_circuit_address(target: &InfrastructureTarget, local_peer_id: PeerId) -> Multiaddr {
    let mut address = target.address.clone();
    address.push(libp2p::multiaddr::Protocol::P2pCircuit);
    address.push(libp2p::multiaddr::Protocol::P2p(local_peer_id));
    address
}

fn discovered_peer_dial_candidates(
    network: &NetworkBootstrap,
    peer_id: PeerId,
    advertised_addresses: &[Multiaddr],
) -> Vec<Multiaddr> {
    let mut candidates = Vec::new();
    for address in advertised_addresses {
        // Private addresses learned through rendezvous belong to the remote
        // peer's LAN. Same-LAN routes are learned separately through mDNS, so
        // retaining these here only creates unreachable route noise.
        if !is_private_or_local_address(address) && !candidates.contains(address) {
            candidates.push(address.clone());
        }
    }
    if let Some(relay) = network.relay.as_ref() {
        let circuit_address = relay_circuit_address(relay, peer_id);
        if !candidates.contains(&circuit_address) {
            candidates.push(circuit_address);
        }
    }
    candidates.sort_by(|left, right| {
        route_score(left)
            .cmp(&route_score(right))
            .then_with(|| left.to_string().cmp(&right.to_string()))
    });
    candidates
}

fn is_private_or_local_address(address: &Multiaddr) -> bool {
    address.iter().any(|protocol| match protocol {
        libp2p::multiaddr::Protocol::Ip4(ip) => {
            ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.octets()[0] == 0
        }
        libp2p::multiaddr::Protocol::Ip6(ip) => {
            ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
        }
        _ => false,
    })
}

fn preferred_dial_candidates(candidates: &[Multiaddr]) -> Vec<Multiaddr> {
    let best_score = candidates.iter().map(route_score).min().unwrap_or(u8::MAX);
    candidates
        .iter()
        .filter(|address| route_score(address) == best_score)
        .take(MAX_DISCOVERY_DIAL_CANDIDATES)
        .cloned()
        .collect()
}

fn route_score(address: &Multiaddr) -> u8 {
    let mut has_circuit = false;
    let mut has_public_direct = false;
    let mut has_private_direct = false;
    let mut has_loopback = false;
    let mut has_dns = false;

    for protocol in address.iter() {
        match protocol {
            libp2p::multiaddr::Protocol::P2pCircuit => has_circuit = true,
            libp2p::multiaddr::Protocol::Dns(_)
            | libp2p::multiaddr::Protocol::Dns4(_)
            | libp2p::multiaddr::Protocol::Dns6(_) => has_dns = true,
            libp2p::multiaddr::Protocol::Ip4(ip) => {
                if ip.is_loopback()
                    || ip.is_link_local()
                    || ip.is_unspecified()
                    || ip.octets()[0] == 0
                {
                    has_loopback = true;
                } else if ip.is_private() {
                    has_private_direct = true;
                } else {
                    has_public_direct = true;
                }
            }
            libp2p::multiaddr::Protocol::Ip6(ip) => {
                if ip.is_loopback() || ip.is_unspecified() {
                    has_loopback = true;
                } else if ip.is_unique_local() || ip.is_unicast_link_local() {
                    has_private_direct = true;
                } else {
                    has_public_direct = true;
                }
            }
            _ => {}
        }
    }

    if !has_circuit && (has_public_direct || has_dns) {
        0
    } else if has_circuit {
        1
    } else if has_private_direct {
        3
    } else if has_loopback {
        4
    } else {
        2
    }
}

fn register_rendezvous(
    swarm: &mut libp2p::Swarm<AgentBehaviour>,
    app: &AppHandle,
    target: Option<&InfrastructureTarget>,
    namespace: &rendezvous::Namespace,
) -> bool {
    let Some(target) = target else {
        return false;
    };
    if swarm.external_addresses().next().is_none() {
        return false;
    }
    match swarm
        .behaviour_mut()
        .rendezvous
        .register(namespace.clone(), target.peer_id, None)
    {
        Ok(()) => true,
        Err(error) => {
            let _ = app.emit(
                "p2p:connection_failed",
                serde_json::json!({
                    "reason": format!("could not register with rendezvous: {error}"),
                }),
            );
            false
        }
    }
}

fn discover_rendezvous(
    swarm: &mut libp2p::Swarm<AgentBehaviour>,
    target: Option<&InfrastructureTarget>,
    namespace: &rendezvous::Namespace,
    cookie: Option<rendezvous::Cookie>,
) {
    let Some(target) = target else {
        return;
    };
    if !swarm.is_connected(&target.peer_id) {
        return;
    }
    swarm.behaviour_mut().rendezvous.discover(
        Some(namespace.clone()),
        cookie,
        Some(100),
        target.peer_id,
    );
}

fn is_infrastructure_peer(network: &NetworkBootstrap, peer_id: PeerId) -> bool {
    network
        .relay
        .as_ref()
        .is_some_and(|target| target.peer_id == peer_id)
        || network
            .rendezvous
            .as_ref()
            .is_some_and(|target| target.peer_id == peer_id)
}

fn configured_bootstrap_peers() -> Result<Vec<Multiaddr>, String> {
    std::env::var("CYPHES_BOOTSTRAP_PEERS")
        .unwrap_or_default()
        .split(',')
        .filter(|value| !value.trim().is_empty())
        .map(|value| {
            value
                .trim()
                .parse::<Multiaddr>()
                .map_err(|error| format!("bootstrap peer address is invalid: {error}"))
        })
        .collect()
}

fn relay_peer_id(address: &Multiaddr) -> Option<PeerId> {
    address.iter().find_map(|protocol| match protocol {
        libp2p::multiaddr::Protocol::P2p(peer_id) => Some(peer_id),
        _ => None,
    })
}

fn secure_identity_file(path: &PathBuf) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn identity_path() -> Result<PathBuf, String> {
    if let Ok(data_dir) = std::env::var("CYPHES_DATA_DIR") {
        return Ok(PathBuf::from(data_dir).join("identity.key"));
    }
    let home = dirs::home_dir().ok_or_else(|| "Could not resolve home directory".to_string())?;
    Ok(home.join(".cyphes").join("identity.key"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_serialized_items_bounds_total_size() {
        // ~100-byte strings; budget of 1000 keeps only a handful.
        let items: Vec<String> = (0..100).map(|_| "x".repeat(100)).collect();
        let (kept, used) = cap_serialized_items(items, 1000);
        assert!(!kept.is_empty() && kept.len() < 100);
        // Never overshoot by more than a single element.
        assert!(used <= 1000 + 100 + 4);

        // A set that fits is returned intact.
        let small = vec!["a".to_string(), "b".to_string()];
        let (kept_small, _) = cap_serialized_items(small.clone(), 1_000_000);
        assert_eq!(kept_small, small);

        // A single oversized item is still retained so the sync cannot stall.
        let (kept_big, _) = cap_serialized_items(vec!["z".repeat(5000)], 100);
        assert_eq!(kept_big.len(), 1);
    }

    fn infrastructure_address() -> String {
        let peer_id = identity::Keypair::generate_ed25519().public().to_peer_id();
        format!("/ip4/127.0.0.1/tcp/4001/p2p/{peer_id}")
    }

    #[test]
    fn relay_is_also_the_default_rendezvous_node() {
        let address = infrastructure_address();
        let network =
            build_network_bootstrap(Some(address.clone()), None, None, Some("test".to_string()))
                .expect("valid network");

        assert_eq!(
            network.relay.as_ref().map(|target| &target.address),
            network.rendezvous.as_ref().map(|target| &target.address)
        );
        assert_eq!(network.namespace.to_string(), DEFAULT_RENDEZVOUS_NAMESPACE);
    }

    #[test]
    fn infrastructure_addresses_require_a_peer_id() {
        let error = build_network_bootstrap(
            Some("/ip4/127.0.0.1/tcp/4001".to_string()),
            None,
            None,
            None,
        )
        .expect_err("address without peer id must fail");

        assert!(error.contains("/p2p/PEER_ID"));
    }

    #[test]
    fn canonical_relay_address_targets_the_local_node() {
        let network = build_network_bootstrap(
            Some(infrastructure_address()),
            None,
            None,
            Some("test".to_string()),
        )
        .expect("valid network");
        let local_peer_id = identity::Keypair::generate_ed25519().public().to_peer_id();

        let address =
            relay_circuit_address(network.relay.as_ref().expect("relay target"), local_peer_id);

        assert!(address
            .to_string()
            .ends_with(&format!("/p2p-circuit/p2p/{local_peer_id}")));
    }

    #[test]
    fn discovered_peer_dial_candidates_drop_remote_private_routes() {
        let relay_address = infrastructure_address();
        let network =
            build_network_bootstrap(Some(relay_address), None, None, Some("test".to_string()))
                .expect("valid network");
        let peer_id = identity::Keypair::generate_ed25519().public().to_peer_id();
        let private_address = format!("/ip4/172.16.8.82/tcp/47166/p2p/{peer_id}")
            .parse::<Multiaddr>()
            .unwrap();

        let candidates = discovered_peer_dial_candidates(&network, peer_id, &[private_address]);

        assert_eq!(candidates.len(), 1);
        assert!(candidates.iter().any(|address| address
            .to_string()
            .ends_with(&format!("/p2p-circuit/p2p/{peer_id}"))));
        assert!(candidates[0]
            .to_string()
            .ends_with(&format!("/p2p-circuit/p2p/{peer_id}")));
        let preferred = preferred_dial_candidates(&candidates);
        assert_eq!(preferred.len(), 1);
        assert!(preferred[0]
            .to_string()
            .ends_with(&format!("/p2p-circuit/p2p/{peer_id}")));
    }

    #[test]
    fn public_direct_route_is_preferred_before_relay_fallback() {
        let relay_address = infrastructure_address();
        let network =
            build_network_bootstrap(Some(relay_address), None, None, Some("test".to_string()))
                .expect("valid network");
        let peer_id = identity::Keypair::generate_ed25519().public().to_peer_id();
        let public_address = format!("/ip4/198.51.100.8/tcp/47166/p2p/{peer_id}")
            .parse::<Multiaddr>()
            .unwrap();

        let candidates =
            discovered_peer_dial_candidates(&network, peer_id, &[public_address.clone()]);
        let preferred = preferred_dial_candidates(&candidates);

        assert_eq!(preferred, vec![public_address]);
    }

    #[test]
    fn peer_cooldown_blocks_discovery_dials_and_sends() {
        let state = P2pState::default();
        let peer_id = identity::Keypair::generate_ed25519().public().to_peer_id();
        let peer_key = peer_id.to_string();
        {
            let mut inner = state.inner.lock().expect("state lock");
            inner.peers.insert(
                peer_key.clone(),
                PeerInfo {
                    peer_id: peer_key,
                    last_seen: now_millis(),
                    failure_streak: 3,
                    cooldown_until: now_millis() + 60_000,
                },
            );
        }
        let outbound = HashMap::<OutboundRequestId, PendingOutbound>::new();

        assert!(!peer_dial_allowed(&state, peer_id));
        assert!(!peer_send_allowed(&state, &outbound, &peer_id));

        let fresh_peer_id = identity::Keypair::generate_ed25519().public().to_peer_id();
        assert!(peer_dial_allowed(&state, fresh_peer_id));
        mark_peer_dial_attempt(&state, fresh_peer_id);
        assert!(!peer_dial_allowed(&state, fresh_peer_id));
    }

    #[test]
    fn labor_wire_capabilities_advertise_sparse_inventory_gate() {
        let capabilities = labor_wire_capabilities();
        assert!(capabilities.contains(&LABOR_CAPABILITY_INVENTORY_V2.to_string()));
        assert!(capabilities.contains(&LABOR_CAPABILITY_SPARSE_INVENTORY_V3.to_string()));
    }

    #[test]
    fn published_network_config_accepts_an_offline_manifest() {
        let config: PublishedNetworkConfig = serde_json::from_str(
            r#"{
                "relayAddr": null,
                "rendezvousAddr": null,
                "rendezvousNamespace": "cyphes.repository-audit.v0.15.1"
            }"#,
        )
        .expect("valid manifest");

        assert!(config.relay_addr.is_none());
        assert!(config.rendezvous_addr.is_none());
    }

    #[test]
    fn embedded_bootstrap_manifest_configures_the_public_network() {
        let network = embedded_network_bootstrap(None).expect("valid embedded network");

        assert!(network.relay.is_some());
        assert!(network.rendezvous.is_some());
        assert_eq!(
            network.source.as_deref(),
            Some("embedded bootstrap manifest")
        );
        assert_eq!(network.namespace.to_string(), DEFAULT_RENDEZVOUS_NAMESPACE);
    }

    #[test]
    fn network_bootstrap_accepts_the_branded_public_hostname() {
        let peer_id = identity::Keypair::generate_ed25519().public().to_peer_id();
        let address = format!("/dns4/relay.cyphes.com/tcp/4001/p2p/{peer_id}");
        let network = build_network_bootstrap(
            Some(address.clone()),
            None,
            None,
            Some("published manifest".to_string()),
        )
        .expect("valid branded public address");

        assert_eq!(
            network.relay.expect("relay target").address.to_string(),
            address
        );
    }
}
