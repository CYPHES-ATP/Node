use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use serde::Serialize;
use tokio::sync::mpsc;

use crate::p2p::SwarmCommand;

#[derive(Clone, Debug, Serialize)]
pub struct PeerInfo {
    pub peer_id: String,
    pub last_seen: u64,
    pub failure_streak: u32,
    pub cooldown_until: u64,
}

#[derive(Default)]
pub struct P2pShared {
    pub started: bool,
    pub local_peer_id: Option<String>,
    pub keypair: Option<libp2p::identity::Keypair>,
    pub sender: Option<mpsc::UnboundedSender<SwarmCommand>>,
    pub peers: HashMap<String, PeerInfo>,
    pub active_peer_links: HashSet<String>,
    pub listen_addrs: Vec<String>,
    pub relay_configured: bool,
    pub relay_connected: bool,
    pub rendezvous_registered: bool,
    pub bootstrap_source: Option<String>,
    pub last_infrastructure_activity_ms: u64,
}

#[derive(Default, Clone)]
pub struct P2pState {
    pub inner: Arc<Mutex<P2pShared>>,
}
