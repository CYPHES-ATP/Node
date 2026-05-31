use serde::Serialize;
use tauri::{AppHandle, State};
use tokio::sync::mpsc;

use crate::{
    p2p::{
        current_millis, load_or_create_identity, spawn_swarm, AgentMessage, SwarmCommand,
        WIRE_TOPIC,
    },
    state::{P2pState, PeerInfo},
};

#[derive(Debug, Serialize)]
pub struct StartNodeResponse {
    pub peer_id: String,
    pub topic: String,
    pub listen_addrs: Vec<String>,
}

#[tauri::command]
pub async fn start_node(
    app: AppHandle,
    state: State<'_, P2pState>,
) -> Result<StartNodeResponse, String> {
    {
        let inner = state.inner.lock().map_err(|error| error.to_string())?;
        if inner.started {
            return Ok(StartNodeResponse {
                peer_id: inner.local_peer_id.clone().unwrap_or_default(),
                topic: WIRE_TOPIC.to_string(),
                listen_addrs: Vec::new(),
            });
        }
    }

    let keypair = load_or_create_identity()?;
    let (tx, rx) = mpsc::unbounded_channel();
    let (peer_id, listen_addrs) = spawn_swarm(app, state.inner().clone(), keypair, rx).await?;

    let mut inner = state.inner.lock().map_err(|error| error.to_string())?;
    inner.started = true;
    inner.local_peer_id = Some(peer_id.clone());
    inner.sender = Some(tx);

    Ok(StartNodeResponse {
        peer_id,
        topic: WIRE_TOPIC.to_string(),
        listen_addrs,
    })
}

#[tauri::command]
pub async fn broadcast_advertise(
    state: State<'_, P2pState>,
    name: String,
    capabilities: Vec<String>,
    endpoint: Option<String>,
    payload: Option<String>,
) -> Result<(), String> {
    let (sender, peer_id) = {
        let inner = state.inner.lock().map_err(|error| error.to_string())?;
        (
            inner
                .sender
                .clone()
                .ok_or_else(|| "P2P node has not started".to_string())?,
            inner
                .local_peer_id
                .clone()
                .ok_or_else(|| "P2P identity missing".to_string())?,
        )
    };

    let message = AgentMessage {
        msg_type: "advertise".to_string(),
        agent_id: peer_id,
        name,
        capabilities,
        endpoint,
        timestamp: current_millis(),
        signature: None,
        payload,
        target_peer_id: None,
        location: None,
        source: Some("local".to_string()),
    };

    sender
        .send(SwarmCommand::Publish(message))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn send_ping(
    state: State<'_, P2pState>,
    target_peer_id: String,
    message: String,
) -> Result<(), String> {
    let (sender, peer_id) = {
        let inner = state.inner.lock().map_err(|error| error.to_string())?;
        (
            inner
                .sender
                .clone()
                .ok_or_else(|| "P2P node has not started".to_string())?,
            inner
                .local_peer_id
                .clone()
                .ok_or_else(|| "P2P identity missing".to_string())?,
        )
    };

    let message = AgentMessage {
        msg_type: "ping".to_string(),
        agent_id: peer_id,
        name: "CYPHES_LOCAL".to_string(),
        capabilities: Vec::new(),
        endpoint: None,
        timestamp: current_millis(),
        signature: None,
        payload: Some(message),
        target_peer_id: Some(target_peer_id),
        location: None,
        source: Some("local".to_string()),
    };

    sender
        .send(SwarmCommand::Publish(message))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn get_peers(state: State<'_, P2pState>) -> Result<Vec<PeerInfo>, String> {
    let inner = state.inner.lock().map_err(|error| error.to_string())?;
    Ok(inner.peers.values().cloned().collect())
}
