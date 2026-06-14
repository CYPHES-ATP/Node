use std::{collections::HashMap, fs, path::PathBuf, time::Duration};

use futures::StreamExt;
use libp2p::{
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
    bundle::export_receipt_bundle,
    state::{P2pState, PeerInfo},
    store::{now_millis, rejection_ack, AtpStore, AuditEventBody},
    worker::SignedExecutionResult,
};

pub const ATP_PROTOCOL: &str = "/cyphes/atp/0.3";
pub const DEFAULT_RENDEZVOUS_NAMESPACE: &str = "cyphes.repository-audit.v0.1";
const DEFAULT_NETWORK_CONFIG_URL: &str =
    "https://raw.githubusercontent.com/CYPHES-ATP/Node/main/network/bootstrap.json";
const MAX_WIRE_REQUEST_BYTES: u64 = 32 * 1024 * 1024;
const INFRASTRUCTURE_RETRY_INTERVAL: Duration = Duration::from_secs(15);
const RENDEZVOUS_DISCOVERY_INTERVAL: Duration = Duration::from_secs(20);
const RENDEZVOUS_REGISTRATION_INTERVAL: Duration = Duration::from_secs(60 * 60);
const PEER_IDLE_CONNECTION_TIMEOUT: Duration = Duration::from_secs(60 * 60);

#[derive(Debug, Clone)]
struct InfrastructureTarget {
    peer_id: PeerId,
    address: Multiaddr,
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
                .set_response_size_maximum(2 * 1024 * 1024);
            let request_response = request_response::Behaviour::with_codec(
                codec,
                [(StreamProtocol::new(ATP_PROTOCOL), ProtocolSupport::Full)],
                request_response::Config::default().with_request_timeout(Duration::from_secs(90)),
            );
            let mdns = mdns::tokio::Behaviour::new(mdns::Config::default(), peer_id)?;
            let identify = identify::Behaviour::new(
                identify::Config::new(ATP_PROTOCOL.to_string(), key.public())
                    .with_agent_version("CYPHES/0.2.0-dev".to_string())
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
    }

    tauri::async_runtime::spawn(async move {
        let mut outbound = HashMap::<OutboundRequestId, PendingOutbound>::new();
        let mut relay_listener_started = false;
        let mut rendezvous_registration_started = false;
        let mut rendezvous_cookie = None;
        let mut discovery_interval = tokio::time::interval(RENDEZVOUS_DISCOVERY_INTERVAL);
        discovery_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut registration_interval = tokio::time::interval(RENDEZVOUS_REGISTRATION_INTERVAL);
        registration_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        registration_interval.tick().await;
        let mut infrastructure_interval = tokio::time::interval(INFRASTRUCTURE_RETRY_INTERVAL);
        infrastructure_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
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
                    ensure_infrastructure_connections(&mut swarm, &network);
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
                        &mut relay_listener_started,
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
    relay_listener_started: &mut bool,
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
                        } else if ack.accepted {
                            let _ = app.emit("atp:jobs_changed", ());
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
                swarm.add_peer_address(peer_id, addr.clone());
                if !swarm.is_connected(&peer_id) {
                    let _ = swarm.dial(addr);
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
                swarm.add_peer_address(peer_id, address);
            }
            if !is_infrastructure_peer(network, peer_id) {
                on_peer_connected(swarm, app, state, store, local_agent_id, peer_id, outbound);
            }
        }
        SwarmEvent::Behaviour(AgentBehaviourEvent::Relay(
            relay::client::Event::ReservationReqAccepted {
                relay_peer_id,
                renewal,
                ..
            },
        )) => {
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
        }
        SwarmEvent::Behaviour(AgentBehaviourEvent::Rendezvous(event)) => match event {
            rendezvous::client::Event::Registered {
                rendezvous_node,
                namespace,
                ..
            } => {
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
                ..
            } => {
                *rendezvous_cookie = Some(cookie);
                let mut discovered = 0usize;
                for registration in registrations {
                    let peer_id = registration.record.peer_id();
                    if peer_id == local_peer_id || is_infrastructure_peer(network, peer_id) {
                        continue;
                    }
                    let addresses = registration.record.addresses();
                    for address in addresses {
                        swarm.add_peer_address(peer_id, address.clone());
                    }
                    if !swarm.is_connected(&peer_id) {
                        if let Some(address) = addresses.first() {
                            let _ = swarm.dial(address.clone());
                        }
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
            if !*relay_listener_started {
                if let Some(relay) = network.relay.as_ref() {
                    if peer_id == relay.peer_id {
                        let mut circuit_addr = relay.address.clone();
                        circuit_addr.push(libp2p::multiaddr::Protocol::P2pCircuit);
                        match swarm.listen_on(circuit_addr) {
                            Ok(_) => *relay_listener_started = true,
                            Err(error) => {
                                let _ = app.emit(
                                    "p2p:connection_failed",
                                    serde_json::json!({
                                        "address": relay.address.to_string(),
                                        "reason": format!("could not reserve relay circuit: {error}"),
                                    }),
                                );
                            }
                        }
                    }
                }
            }
            if !is_infrastructure_peer(network, peer_id) {
                on_peer_connected(swarm, app, state, store, local_agent_id, peer_id, outbound);
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
            if network
                .relay
                .as_ref()
                .is_some_and(|relay| relay.peer_id == peer_id)
            {
                *rendezvous_registration_started = false;
                if let Ok(mut inner) = state.inner.lock() {
                    inner.relay_connected = false;
                    inner.rendezvous_registered = false;
                }
            }
            if network
                .rendezvous
                .as_ref()
                .is_some_and(|rendezvous| rendezvous.peer_id == peer_id)
            {
                *rendezvous_registration_started = false;
                if let Ok(mut inner) = state.inner.lock() {
                    inner.rendezvous_registered = false;
                }
            }
            if let Ok(mut inner) = state.inner.lock() {
                inner.peers.remove(&peer_id.to_string());
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
            if address
                .iter()
                .any(|protocol| protocol == libp2p::multiaddr::Protocol::P2pCircuit)
                && !*rendezvous_registration_started
                && register_rendezvous(swarm, app, network.rendezvous.as_ref(), &network.namespace)
            {
                *rendezvous_registration_started = true;
            }
        }
        SwarmEvent::ListenerClosed { addresses, .. } => {
            if addresses.iter().any(|address| {
                address
                    .iter()
                    .any(|protocol| protocol == libp2p::multiaddr::Protocol::P2pCircuit)
            }) {
                *relay_listener_started = false;
                *rendezvous_registration_started = false;
                if let Ok(mut inner) = state.inner.lock() {
                    inner.relay_connected = false;
                    inner.rendezvous_registered = false;
                }
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
    let published = fetch_published_network_config(&config_url).await;
    match published {
        Ok(config) => build_network_bootstrap(
            config.relay_addr,
            config.rendezvous_addr,
            namespace_override.clone().or(config.rendezvous_namespace),
            Some(config_url),
        )
        .or_else(|_| build_network_bootstrap(None, None, namespace_override, None)),
        Err(_) => build_network_bootstrap(None, None, namespace_override, None),
    }
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

fn ensure_infrastructure_connections(
    swarm: &mut libp2p::Swarm<AgentBehaviour>,
    network: &NetworkBootstrap,
) {
    let mut peers = Vec::<PeerId>::new();
    for target in [network.relay.as_ref(), network.rendezvous.as_ref()]
        .into_iter()
        .flatten()
    {
        if peers.contains(&target.peer_id) || swarm.is_connected(&target.peer_id) {
            continue;
        }
        let _ = swarm.dial(target.address.clone());
        peers.push(target.peer_id);
    }
}

fn relay_circuit_address(target: &InfrastructureTarget, local_peer_id: PeerId) -> Multiaddr {
    let mut address = target.address.clone();
    address.push(libp2p::multiaddr::Protocol::P2pCircuit);
    address.push(libp2p::multiaddr::Protocol::P2p(local_peer_id));
    address
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
    fn published_network_config_accepts_an_offline_manifest() {
        let config: PublishedNetworkConfig = serde_json::from_str(
            r#"{
                "relayAddr": null,
                "rendezvousAddr": null,
                "rendezvousNamespace": "cyphes.repository-audit.v0.1"
            }"#,
        )
        .expect("valid manifest");

        assert!(config.relay_addr.is_none());
        assert!(config.rendezvous_addr.is_none());
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
