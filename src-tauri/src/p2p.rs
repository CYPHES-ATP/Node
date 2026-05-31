use std::{
    fs,
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use futures::StreamExt;
use libp2p::{
    gossipsub::{self, IdentTopic, MessageAuthenticity, ValidationMode},
    identity, mdns, noise,
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux, Multiaddr, PeerId, SwarmBuilder,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::{select, sync::mpsc, time};

use crate::state::{P2pState, PeerInfo};

pub const WIRE_TOPIC: &str = "cyphes-v0.1-wire";
const HEARTBEAT_EVERY: Duration = Duration::from_secs(30);

const BOOTSTRAP_RELAYS: &[&str] = &[];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub msg_type: String,
    pub agent_id: String,
    pub name: String,
    pub capabilities: Vec<String>,
    pub endpoint: Option<String>,
    pub timestamp: u64,
    pub signature: Option<String>,
    pub payload: Option<String>,
    pub target_peer_id: Option<String>,
    pub location: Option<String>,
    pub source: Option<String>,
}

#[derive(Debug, Clone)]
pub enum SwarmCommand {
    Publish(AgentMessage),
}

#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "AgentBehaviourEvent")]
struct AgentBehaviour {
    gossipsub: gossipsub::Behaviour,
    mdns: mdns::tokio::Behaviour,
}

#[derive(Debug)]
enum AgentBehaviourEvent {
    Gossipsub(gossipsub::Event),
    Mdns(mdns::Event),
}

impl From<gossipsub::Event> for AgentBehaviourEvent {
    fn from(event: gossipsub::Event) -> Self {
        Self::Gossipsub(event)
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
    fs::write(&identity_path, encoded).map_err(|error| error.to_string())?;
    Ok(keypair)
}

pub async fn spawn_swarm(
    app: AppHandle,
    state: P2pState,
    keypair: identity::Keypair,
    mut rx: mpsc::UnboundedReceiver<SwarmCommand>,
) -> Result<(String, Vec<String>), String> {
    let local_peer_id = keypair.public().to_peer_id();
    let topic = IdentTopic::new(WIRE_TOPIC);

    let gossipsub_config = gossipsub::ConfigBuilder::default()
        .heartbeat_interval(Duration::from_secs(10))
        .validation_mode(ValidationMode::Permissive)
        .build()
        .map_err(|error| error.to_string())?;

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
            let mut gossipsub = gossipsub::Behaviour::new(
                MessageAuthenticity::Signed(key.clone()),
                gossipsub_config.clone(),
            )?;
            gossipsub.subscribe(&topic)?;

            let mdns = mdns::tokio::Behaviour::new(mdns::Config::default(), peer_id)?;

            Ok(AgentBehaviour { gossipsub, mdns })
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

    for relay in BOOTSTRAP_RELAYS {
        if let Ok(address) = relay.parse::<Multiaddr>() {
            let _ = swarm.dial(address);
        }
    }

    let listen_addrs = swarm
        .listeners()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let topic_for_publish = IdentTopic::new(WIRE_TOPIC);
    let topic_for_heartbeat = IdentTopic::new(WIRE_TOPIC);
    let mut heartbeat = time::interval(HEARTBEAT_EVERY);
    let mut local_name = "CYPHES_NODE".to_string();
    let mut local_capabilities: Vec<String> = Vec::new();
    let mut local_endpoint: Option<String> = None;

    tauri::async_runtime::spawn(async move {
        loop {
            select! {
                maybe_command = rx.recv() => {
                    let Some(command) = maybe_command else {
                        break;
                    };

                    match command {
                        SwarmCommand::Publish(message) => {
                            local_name = message.name.clone();
                            local_capabilities = message.capabilities.clone();
                            local_endpoint = message.endpoint.clone();

                            if let Ok(bytes) = serde_json::to_vec(&message) {
                                let _ = swarm.behaviour_mut().gossipsub.publish(topic_for_publish.clone(), bytes);
                            }
                        }
                    }
                }
                _ = heartbeat.tick() => {
                    let message = AgentMessage {
                        msg_type: "heartbeat".to_string(),
                        agent_id: local_peer_id.to_string(),
                        name: local_name.clone(),
                        capabilities: local_capabilities.clone(),
                        endpoint: local_endpoint.clone(),
                        timestamp: current_millis(),
                        signature: None,
                        payload: Some("checked in on cyphes-v0.1-wire".to_string()),
                        target_peer_id: None,
                        location: None,
                        source: Some("local".to_string()),
                    };

                    if let Ok(bytes) = serde_json::to_vec(&message) {
                        let _ = swarm.behaviour_mut().gossipsub.publish(topic_for_heartbeat.clone(), bytes);
                    }
                }
                event = swarm.select_next_some() => {
                    handle_swarm_event(event, &mut swarm, &app, &state, local_peer_id);
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
    local_peer_id: PeerId,
) {
    match event {
        SwarmEvent::Behaviour(AgentBehaviourEvent::Gossipsub(gossipsub::Event::Message {
            propagation_source,
            message,
            ..
        })) => {
            if propagation_source == local_peer_id {
                return;
            }

            if let Ok(mut agent_message) = serde_json::from_slice::<AgentMessage>(&message.data) {
                if agent_message.source.is_none() {
                    agent_message.source = Some("global".to_string());
                }

                upsert_peer(state, &agent_message);

                let event_name = match agent_message.msg_type.as_str() {
                    "advertise" => "p2p:advertise",
                    "heartbeat" => "p2p:heartbeat",
                    "ping" => "p2p:ping",
                    "pong" => "p2p:pong",
                    _ => "p2p:advertise",
                };
                let _ = app.emit(event_name, agent_message);
            }
        }
        SwarmEvent::Behaviour(AgentBehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
            for (peer_id, _addr) in list {
                if peer_id == local_peer_id {
                    continue;
                }

                swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);

                if let Ok(mut inner) = state.inner.lock() {
                    inner.peers.insert(
                        peer_id.to_string(),
                        PeerInfo {
                            peer_id: peer_id.to_string(),
                            name: None,
                            capabilities: Vec::new(),
                            endpoint: None,
                            last_seen: current_millis(),
                            source: "local".to_string(),
                        },
                    );
                }

                let _ = app.emit(
                    "p2p:peer_connected",
                    serde_json::json!({
                        "peer_id": peer_id.to_string(),
                        "source": "local"
                    }),
                );
            }
        }
        SwarmEvent::Behaviour(AgentBehaviourEvent::Mdns(mdns::Event::Expired(list))) => {
            if let Ok(mut inner) = state.inner.lock() {
                for (peer_id, _addr) in list {
                    swarm
                        .behaviour_mut()
                        .gossipsub
                        .remove_explicit_peer(&peer_id);
                    inner.peers.remove(&peer_id.to_string());
                    let _ = app.emit(
                        "p2p:peer_disconnected",
                        serde_json::json!({
                            "peer_id": peer_id.to_string()
                        }),
                    );
                }
            }
        }
        _ => {}
    }
}

fn upsert_peer(state: &P2pState, message: &AgentMessage) {
    if let Ok(mut inner) = state.inner.lock() {
        inner.peers.insert(
            message.agent_id.clone(),
            PeerInfo {
                peer_id: message.agent_id.clone(),
                name: Some(message.name.clone()),
                capabilities: message.capabilities.clone(),
                endpoint: message.endpoint.clone(),
                last_seen: message.timestamp,
                source: message
                    .source
                    .clone()
                    .unwrap_or_else(|| "global".to_string()),
            },
        );
    }
}

pub fn current_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn identity_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "Could not resolve home directory".to_string())?;
    Ok(home.join(".cyphes").join("identity.key"))
}
