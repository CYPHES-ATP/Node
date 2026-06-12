use std::{collections::HashMap, fs, path::PathBuf, time::Duration};

use futures::StreamExt;
use libp2p::{
    identity, mdns, noise, request_response,
    request_response::{OutboundRequestId, ProtocolSupport},
    swarm::{NetworkBehaviour, StreamProtocol, SwarmEvent},
    tcp, yamux, Multiaddr, PeerId, SwarmBuilder,
};
use tauri::{AppHandle, Emitter};
use tokio::{select, sync::mpsc};

use crate::{
    atp::{agent_id, AtpAck, AtpEnvelope},
    state::{P2pState, PeerInfo},
    store::{now_millis, rejection_ack, AtpStore},
};

pub const ATP_PROTOCOL: &str = "/cyphes/atp/0.3";

#[derive(Debug, Clone)]
pub enum SwarmCommand {
    SendEnvelope(AtpEnvelope),
}

#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "AgentBehaviourEvent")]
struct AgentBehaviour {
    request_response: request_response::cbor::Behaviour<AtpEnvelope, AtpAck>,
    mdns: mdns::tokio::Behaviour,
}

#[derive(Debug)]
enum AgentBehaviourEvent {
    RequestResponse(request_response::Event<AtpEnvelope, AtpAck>),
    Mdns(mdns::Event),
}

impl From<request_response::Event<AtpEnvelope, AtpAck>> for AgentBehaviourEvent {
    fn from(event: request_response::Event<AtpEnvelope, AtpAck>) -> Self {
        Self::RequestResponse(event)
    }
}

impl From<mdns::Event> for AgentBehaviourEvent {
    fn from(event: mdns::Event) -> Self {
        Self::Mdns(event)
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

    let mut swarm = SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_tcp(
            tcp::Config::default().nodelay(true),
            noise::Config::new,
            yamux::Config::default,
        )
        .map_err(|error| error.to_string())?
        .with_websocket(noise::Config::new, yamux::Config::default)
        .await
        .map_err(|error| error.to_string())?
        .with_behaviour(move |key| {
            let peer_id = key.public().to_peer_id();
            let request_response = request_response::cbor::Behaviour::new(
                [(StreamProtocol::new(ATP_PROTOCOL), ProtocolSupport::Full)],
                request_response::Config::default().with_request_timeout(Duration::from_secs(15)),
            );
            let mdns = mdns::tokio::Behaviour::new(mdns::Config::default(), peer_id)?;
            Ok(AgentBehaviour {
                request_response,
                mdns,
            })
        })
        .map_err(|error| error.to_string())?
        .build();

    swarm
        .listen_on(
            "/ip4/0.0.0.0/tcp/0"
                .parse::<Multiaddr>()
                .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
    swarm
        .listen_on(
            "/ip4/0.0.0.0/tcp/0/ws"
                .parse::<Multiaddr>()
                .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;

    let listen_addrs = swarm
        .listeners()
        .map(ToString::to_string)
        .collect::<Vec<_>>();

    tauri::async_runtime::spawn(async move {
        let mut outbound = HashMap::<OutboundRequestId, String>::new();
        loop {
            select! {
                maybe_command = rx.recv() => {
                    let Some(command) = maybe_command else {
                        break;
                    };
                    match command {
                        SwarmCommand::SendEnvelope(envelope) => {
                            send_to_known_peers(&mut swarm, &state, envelope, &mut outbound);
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
                        local_peer_id,
                        &local_agent_id,
                        &mut outbound,
                    );
                }
            }
        }
    });

    Ok((local_peer_id.to_string(), listen_addrs))
}

fn handle_swarm_event(
    event: SwarmEvent<AgentBehaviourEvent>,
    swarm: &mut libp2p::Swarm<AgentBehaviour>,
    app: &AppHandle,
    state: &P2pState,
    store: &AtpStore,
    local_peer_id: PeerId,
    local_agent_id: &str,
    outbound: &mut HashMap<OutboundRequestId, String>,
) {
    match event {
        SwarmEvent::Behaviour(AgentBehaviourEvent::RequestResponse(
            request_response::Event::Message { peer, message, .. },
        )) => match message {
            request_response::Message::Request {
                request, channel, ..
            } => {
                touch_peer(state, peer);
                let ack = match store.commit_envelope(
                    &request,
                    local_agent_id,
                    Some(&peer.to_string()),
                ) {
                    Ok(ack) => {
                        if !ack.duplicate {
                            let _ = app.emit("atp:jobs_changed", ());
                        }
                        ack
                    }
                    Err(reason) => rejection_ack(&request, local_agent_id, reason),
                };
                let _ = swarm
                    .behaviour_mut()
                    .request_response
                    .send_response(channel, ack);
            }
            request_response::Message::Response {
                request_id,
                response,
            } => {
                outbound.remove(&request_id);
                touch_peer(state, peer);
                if let Err(error) = store.mark_delivery(&peer.to_string(), &response) {
                    let _ = app.emit(
                        "atp:delivery_failed",
                        serde_json::json!({ "peerId": peer.to_string(), "reason": error }),
                    );
                } else {
                    let _ = app.emit("atp:jobs_changed", ());
                    let _ = app.emit("atp:delivery_acknowledged", response);
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
            let event_hash = outbound.remove(&request_id).unwrap_or_default();
            let _ = app.emit(
                "atp:delivery_failed",
                serde_json::json!({
                    "peerId": peer.to_string(),
                    "eventHash": event_hash,
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
                touch_peer(state, peer_id);
                let peer_agent_id = format!("urn:libp2p:{peer_id}");
                if let Ok(envelopes) = store.envelopes_for_peer(local_agent_id, &peer_agent_id) {
                    for envelope in envelopes {
                        let event_hash = crate::atp::event_hash(&envelope).unwrap_or_default();
                        let request_id = swarm
                            .behaviour_mut()
                            .request_response
                            .send_request(&peer_id, envelope);
                        outbound.insert(request_id, event_hash);
                    }
                }
                let _ = app.emit(
                    "p2p:peer_connected",
                    serde_json::json!({ "peerId": peer_id.to_string() }),
                );
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
        _ => {}
    }
}

fn send_to_known_peers(
    swarm: &mut libp2p::Swarm<AgentBehaviour>,
    state: &P2pState,
    envelope: AtpEnvelope,
    outbound: &mut HashMap<OutboundRequestId, String>,
) {
    let peers = state
        .inner
        .lock()
        .map(|inner| inner.peers.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    let hash = crate::atp::event_hash(&envelope).unwrap_or_default();

    for peer in peers {
        let Ok(peer_id) = peer.parse::<PeerId>() else {
            continue;
        };
        let peer_agent_id = format!("urn:libp2p:{peer_id}");
        if envelope
            .audience
            .as_deref()
            .is_some_and(|audience| audience != peer_agent_id)
        {
            continue;
        }
        let request_id = swarm
            .behaviour_mut()
            .request_response
            .send_request(&peer_id, envelope.clone());
        outbound.insert(request_id, hash.clone());
    }
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
