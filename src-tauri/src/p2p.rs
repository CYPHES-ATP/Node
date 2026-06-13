use std::{collections::HashMap, fs, path::PathBuf, time::Duration};

use futures::StreamExt;
use libp2p::{
    dcutr, identify, identity, mdns, noise, ping, relay, request_response,
    request_response::{OutboundRequestId, ProtocolSupport},
    swarm::{NetworkBehaviour, StreamProtocol, SwarmEvent},
    tcp, yamux, Multiaddr, PeerId, SwarmBuilder,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::{select, sync::mpsc};

use crate::{
    atp::{agent_id, create_signed_envelope, AtpAck, AtpEnvelope, AtpVerb},
    bundle::export_receipt_bundle,
    state::{P2pState, PeerInfo},
    store::{now_millis, rejection_ack, AtpStore, AuditEventBody},
    worker::SignedExecutionResult,
};

pub const ATP_PROTOCOL: &str = "/cyphes/atp/0.3";
const MAX_WIRE_REQUEST_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Debug, Clone)]
pub enum SwarmCommand {
    SendEnvelope(AtpEnvelope),
    SendExecutionResult {
        result: SignedExecutionResult,
        audience: String,
    },
    Dial(Multiaddr),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
enum WireRequest {
    Envelope(AtpEnvelope),
    ExecutionResult(SignedExecutionResult),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
enum WireResponse {
    Envelope(AtpAck),
    ExecutionResult {
        accepted: bool,
        transaction_id: String,
        result_hash: String,
        reason: Option<String>,
    },
}

#[derive(Debug)]
enum PendingOutbound {
    Envelope(String),
    ExecutionResult {
        transaction_id: String,
        result_hash: String,
    },
}

#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "AgentBehaviourEvent")]
struct AgentBehaviour {
    request_response: request_response::Behaviour<
        request_response::cbor::codec::Codec<WireRequest, WireResponse>,
    >,
    mdns: mdns::tokio::Behaviour,
    identify: identify::Behaviour,
    ping: ping::Behaviour,
    relay: relay::client::Behaviour,
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
            let codec = request_response::cbor::codec::Codec::default()
                .set_request_size_maximum(MAX_WIRE_REQUEST_BYTES)
                .set_response_size_maximum(2 * 1024 * 1024);
            let request_response = request_response::Behaviour::with_codec(
                codec,
                [(StreamProtocol::new(ATP_PROTOCOL), ProtocolSupport::Full)],
                request_response::Config::default().with_request_timeout(Duration::from_secs(90)),
            );
            let mdns = mdns::tokio::Behaviour::new(mdns::Config::default(), peer_id)?;
            let identify = identify::Behaviour::new(
                identify::Config::new(ATP_PROTOCOL.to_string(), key.public())
                    .with_agent_version("CYPHES/0.1.0-dev".to_string())
                    .with_push_listen_addr_updates(true),
            );
            Ok(AgentBehaviour {
                request_response,
                mdns,
                identify,
                ping: ping::Behaviour::default(),
                relay,
                dcutr: dcutr::Behaviour::new(peer_id),
            })
        })
        .map_err(|error| error.to_string())?
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

    let relay_addr = configured_relay()?;
    if let Some(relay_addr) = relay_addr.as_ref() {
        swarm
            .dial(relay_addr.clone())
            .map_err(|error| format!("could not dial configured relay: {error}"))?;
    }
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
        inner.relay_configured = relay_addr.is_some();
    }

    tauri::async_runtime::spawn(async move {
        let mut outbound = HashMap::<OutboundRequestId, PendingOutbound>::new();
        let relay_target = relay_addr
            .and_then(|address| relay_peer_id(&address).map(|peer_id| (peer_id, address)));
        let mut relay_listener_started = false;
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
                        SwarmCommand::SendExecutionResult { result, audience } => {
                            send_execution_result(
                                &mut swarm,
                                &state,
                                result,
                                &audience,
                                &mut outbound,
                            );
                        }
                        SwarmCommand::Dial(address) => {
                            if let Err(error) = swarm.dial(address.clone()) {
                                let _ = app.emit(
                                    "p2p:connection_failed",
                                    serde_json::json!({"address": address.to_string(), "reason": error.to_string()}),
                                );
                            }
                        }
                    }
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
                        relay_target.as_ref(),
                        &mut relay_listener_started,
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
    relay_target: Option<&(PeerId, Multiaddr)>,
    relay_listener_started: &mut bool,
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
                                    maybe_attest(
                                        swarm,
                                        app,
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
                            Err(reason) => rejection_ack(&envelope, local_agent_id, reason),
                        };
                        WireResponse::Envelope(ack)
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
                touch_peer(state, peer);
                match response {
                    WireResponse::Envelope(ack) => {
                        if let Err(error) = store.mark_delivery(&peer.to_string(), &ack) {
                            let _ = app.emit(
                                "atp:delivery_failed",
                                serde_json::json!({ "peerId": peer.to_string(), "reason": error }),
                            );
                        } else {
                            let _ = app.emit("atp:jobs_changed", ());
                            let _ = app.emit("atp:delivery_acknowledged", ack);
                        }
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
            let (event_hash, transaction_id, result_hash) = match pending {
                Some(PendingOutbound::Envelope(event_hash)) => (Some(event_hash), None, None),
                Some(PendingOutbound::ExecutionResult {
                    transaction_id,
                    result_hash,
                }) => (None, Some(transaction_id), Some(result_hash)),
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
                swarm.add_peer_address(peer_id, addr);
                on_peer_connected(swarm, app, state, store, local_agent_id, peer_id, outbound);
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
                swarm.add_peer_address(peer_id, address);
            }
            on_peer_connected(swarm, app, state, store, local_agent_id, peer_id, outbound);
        }
        SwarmEvent::Behaviour(AgentBehaviourEvent::Relay(
            relay::client::Event::ReservationReqAccepted {
                relay_peer_id,
                renewal,
                ..
            },
        )) => {
            let _ = app.emit(
                "p2p:relay_ready",
                serde_json::json!({
                    "relayPeerId": relay_peer_id.to_string(),
                    "renewal": renewal,
                }),
            );
        }
        SwarmEvent::ConnectionEstablished { peer_id, .. } => {
            if !*relay_listener_started {
                if let Some((relay_peer_id, relay_addr)) = relay_target {
                    if peer_id == *relay_peer_id {
                        let mut circuit_addr = relay_addr.clone();
                        circuit_addr.push(libp2p::multiaddr::Protocol::P2pCircuit);
                        match swarm.listen_on(circuit_addr) {
                            Ok(_) => *relay_listener_started = true,
                            Err(error) => {
                                let _ = app.emit(
                                    "p2p:connection_failed",
                                    serde_json::json!({
                                        "address": relay_addr.to_string(),
                                        "reason": format!("could not reserve relay circuit: {error}"),
                                    }),
                                );
                            }
                        }
                    }
                }
            }
            on_peer_connected(swarm, app, state, store, local_agent_id, peer_id, outbound);
        }
        SwarmEvent::ConnectionClosed { peer_id, .. } => {
            if let Ok(mut inner) = state.inner.lock() {
                inner.peers.remove(&peer_id.to_string());
            }
            let _ = app.emit(
                "p2p:peer_disconnected",
                serde_json::json!({ "peerId": peer_id.to_string() }),
            );
        }
        SwarmEvent::NewListenAddr { address, .. } => {
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
        SwarmEvent::ListenerClosed { addresses, .. } => {
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
            let request_id = swarm
                .behaviour_mut()
                .request_response
                .send_request(&peer, WireRequest::Envelope(attest));
            outbound.insert(request_id, PendingOutbound::Envelope(event_hash));
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
    let peer_agent_id = format!("urn:libp2p:{peer_id}");
    if let Ok(envelopes) = store.envelopes_for_peer(local_agent_id, &peer_agent_id) {
        for envelope in envelopes {
            let event_hash = crate::atp::event_hash(&envelope).unwrap_or_default();
            let request_id = swarm
                .behaviour_mut()
                .request_response
                .send_request(&peer_id, WireRequest::Envelope(envelope));
            outbound.insert(request_id, PendingOutbound::Envelope(event_hash));
        }
    }
    let _ = app.emit(
        "p2p:peer_connected",
        serde_json::json!({ "peerId": peer_id.to_string() }),
    );
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
        let request_id = swarm
            .behaviour_mut()
            .request_response
            .send_request(&peer_id, WireRequest::Envelope(envelope.clone()));
        outbound.insert(request_id, PendingOutbound::Envelope(hash.clone()));
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
        let request_id = swarm
            .behaviour_mut()
            .request_response
            .send_request(&peer_id, WireRequest::ExecutionResult(result.clone()));
        outbound.insert(
            request_id,
            PendingOutbound::ExecutionResult {
                transaction_id: result.transaction_id.clone(),
                result_hash: result.result_hash.clone(),
            },
        );
    }
}

fn target_peers(state: &P2pState, audience: Option<&str>) -> Vec<PeerId> {
    state
        .inner
        .lock()
        .map(|inner| {
            inner
                .peers
                .keys()
                .filter_map(|peer| peer.parse::<PeerId>().ok())
                .filter(|peer| {
                    audience.is_none_or(|audience| audience == format!("urn:libp2p:{peer}"))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn touch_peer(state: &P2pState, peer_id: PeerId) {
    if let Ok(mut inner) = state.inner.lock() {
        inner.peers.insert(
            peer_id.to_string(),
            PeerInfo {
                peer_id: peer_id.to_string(),
                last_seen: now_millis(),
            },
        );
    }
}

fn configured_relay() -> Result<Option<Multiaddr>, String> {
    std::env::var("CYPHES_RELAY_ADDR")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(|value| {
            value
                .parse::<Multiaddr>()
                .map_err(|error| format!("CYPHES_RELAY_ADDR is invalid: {error}"))
        })
        .transpose()
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
