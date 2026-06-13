use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use serde::Serialize;
use tokio::sync::mpsc;

use crate::p2p::SwarmCommand;

#[derive(Clone, Debug, Serialize)]
pub struct PeerInfo {
    pub peer_id: String,
    pub last_seen: u64,
}

#[derive(Default)]
pub struct P2pShared {
    pub started: bool,
    pub local_peer_id: Option<String>,
    pub keypair: Option<libp2p::identity::Keypair>,
    pub sender: Option<mpsc::UnboundedSender<SwarmCommand>>,
    pub peers: HashMap<String, PeerInfo>,
    pub listen_addrs: Vec<String>,
    pub relay_configured: bool,
}

#[derive(Default, Clone)]
pub struct P2pState {
    pub inner: Arc<Mutex<P2pShared>>,
}
