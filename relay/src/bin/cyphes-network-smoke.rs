use std::time::Duration;

use futures::StreamExt;
use libp2p::{
    identify, identity, noise, ping, relay, rendezvous,
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux, Multiaddr, PeerId, Swarm, SwarmBuilder,
};

const NAMESPACE: &str = "cyphes.repository-audit.v0.1";

#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "ClientEvent")]
struct ClientBehaviour {
    relay: relay::client::Behaviour,
    rendezvous: rendezvous::client::Behaviour,
    identify: identify::Behaviour,
    ping: ping::Behaviour,
}

#[allow(dead_code)]
#[derive(Debug)]
enum ClientEvent {
    Relay(relay::client::Event),
    Rendezvous(rendezvous::client::Event),
    Identify(identify::Event),
    Ping(ping::Event),
}

impl From<relay::client::Event> for ClientEvent {
    fn from(event: relay::client::Event) -> Self {
        Self::Relay(event)
    }
}

impl From<rendezvous::client::Event> for ClientEvent {
    fn from(event: rendezvous::client::Event) -> Self {
        Self::Rendezvous(event)
    }
}

impl From<identify::Event> for ClientEvent {
    fn from(event: identify::Event) -> Self {
        Self::Identify(event)
    }
}

impl From<ping::Event> for ClientEvent {
    fn from(event: ping::Event) -> Self {
        Self::Ping(event)
    }
}

struct TestNode {
    name: &'static str,
    peer_id: PeerId,
    swarm: Swarm<ClientBehaviour>,
    listener_started: bool,
    registration_requested: bool,
    registered: bool,
    discovered_counterparty: bool,
    dial_requested: bool,
    connected_counterparty: bool,
}

impl TestNode {
    fn new(name: &'static str, relay_addr: &Multiaddr) -> Result<Self, Box<dyn std::error::Error>> {
        let keypair = identity::Keypair::generate_ed25519();
        let peer_id = keypair.public().to_peer_id();
        let mut swarm = SwarmBuilder::with_existing_identity(keypair)
            .with_tokio()
            .with_tcp(
                tcp::Config::default().nodelay(true),
                noise::Config::new,
                yamux::Config::default,
            )?
            .with_quic()
            .with_relay_client(noise::Config::new, yamux::Config::default)?
            .with_behaviour(move |key, relay| ClientBehaviour {
                relay,
                rendezvous: rendezvous::client::Behaviour::new(key.clone()),
                identify: identify::Behaviour::new(identify::Config::new(
                    "/cyphes/network-smoke/0.1".to_string(),
                    key.public(),
                )),
                ping: ping::Behaviour::default(),
            })?
            .build();
        swarm.dial(relay_addr.clone())?;
        Ok(Self {
            name,
            peer_id,
            swarm,
            listener_started: false,
            registration_requested: false,
            registered: false,
            discovered_counterparty: false,
            dial_requested: false,
            connected_counterparty: false,
        })
    }

    fn handle(
        &mut self,
        event: SwarmEvent<ClientEvent>,
        relay_addr: &Multiaddr,
        relay_peer_id: PeerId,
        counterparty: PeerId,
    ) -> Result<(), String> {
        match event {
            SwarmEvent::ConnectionEstablished { peer_id, .. }
                if peer_id == relay_peer_id && !self.listener_started =>
            {
                let mut circuit = relay_addr.clone();
                circuit.push(libp2p::multiaddr::Protocol::P2pCircuit);
                self.swarm
                    .listen_on(circuit)
                    .map_err(|error| format!("{} could not reserve circuit: {error}", self.name))?;
                self.listener_started = true;
            }
            SwarmEvent::ConnectionEstablished { peer_id, .. } if peer_id == counterparty => {
                self.connected_counterparty = true;
                println!(
                    "{} connected automatically to counterparty {}",
                    self.name, counterparty
                );
            }
            SwarmEvent::NewListenAddr { address, .. }
                if address
                    .iter()
                    .any(|protocol| protocol == libp2p::multiaddr::Protocol::P2pCircuit) =>
            {
                self.swarm.add_external_address(address);
                if !self.registration_requested {
                    let namespace = rendezvous::Namespace::from_static(NAMESPACE);
                    self.swarm
                        .behaviour_mut()
                        .rendezvous
                        .register(namespace, relay_peer_id, None)
                        .map_err(|error| {
                            format!("{} could not register with rendezvous: {error}", self.name)
                        })?;
                    self.registration_requested = true;
                }
            }
            SwarmEvent::Behaviour(ClientEvent::Rendezvous(
                rendezvous::client::Event::Registered { .. },
            )) => {
                self.registered = true;
                println!("{} registered signed relay address", self.name);
                self.discover(relay_peer_id);
            }
            SwarmEvent::Behaviour(ClientEvent::Rendezvous(
                rendezvous::client::Event::Discovered { registrations, .. },
            )) => {
                for registration in registrations {
                    if registration.record.peer_id() != counterparty {
                        continue;
                    }
                    self.discovered_counterparty = true;
                    println!("{} discovered counterparty {}", self.name, counterparty);
                    if !self.swarm.is_connected(&counterparty) && !self.dial_requested {
                        let address = registration
                            .record
                            .addresses()
                            .first()
                            .ok_or_else(|| {
                                format!("{} discovered a peer without an address", self.name)
                            })?
                            .clone();
                        self.swarm.dial(address).map_err(|error| {
                            format!("{} could not dial peer: {error}", self.name)
                        })?;
                        self.dial_requested = true;
                    }
                }
            }
            SwarmEvent::Behaviour(ClientEvent::Rendezvous(
                rendezvous::client::Event::RegisterFailed { error, .. },
            )) => return Err(format!("{} registration failed: {error:?}", self.name)),
            SwarmEvent::Behaviour(ClientEvent::Rendezvous(
                rendezvous::client::Event::DiscoverFailed { error, .. },
            )) => return Err(format!("{} discovery failed: {error:?}", self.name)),
            SwarmEvent::OutgoingConnectionError {
                peer_id: Some(peer_id),
                error,
                ..
            } if peer_id == relay_peer_id => {
                return Err(format!(
                    "{} connection to {peer_id} failed: {error}",
                    self.name
                ));
            }
            SwarmEvent::OutgoingConnectionError {
                peer_id: Some(peer_id),
                error,
                ..
            } if peer_id == counterparty => {
                self.dial_requested = false;
                println!(
                    "{} automatic dial to {} will retry: {}",
                    self.name, counterparty, error
                );
            }
            _ => {}
        }
        Ok(())
    }

    fn discover(&mut self, relay_peer_id: PeerId) {
        self.swarm.behaviour_mut().rendezvous.discover(
            Some(rendezvous::Namespace::from_static(NAMESPACE)),
            None,
            Some(100),
            relay_peer_id,
        );
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let relay_addr = std::env::args()
        .nth(1)
        .ok_or("usage: cyphes-network-smoke <relay-multiaddr>")?
        .parse::<Multiaddr>()?;
    let relay_peer_id = relay_addr
        .iter()
        .find_map(|protocol| match protocol {
            libp2p::multiaddr::Protocol::P2p(peer_id) => Some(peer_id),
            _ => None,
        })
        .ok_or("relay multiaddress must end with /p2p/RELAY_PEER_ID")?;

    let mut requester = TestNode::new("requester", &relay_addr)?;
    let mut worker = TestNode::new("worker", &relay_addr)?;
    let requester_peer_id = requester.peer_id;
    let worker_peer_id = worker.peer_id;
    let mut discovery_tick = tokio::time::interval(Duration::from_secs(2));

    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            tokio::select! {
                event = requester.swarm.select_next_some() => {
                    requester.handle(event, &relay_addr, relay_peer_id, worker_peer_id)?;
                }
                event = worker.swarm.select_next_some() => {
                    worker.handle(event, &relay_addr, relay_peer_id, requester_peer_id)?;
                }
                _ = discovery_tick.tick() => {
                    if requester.registered {
                        requester.discover(relay_peer_id);
                    }
                    if worker.registered {
                        worker.discover(relay_peer_id);
                    }
                }
            }

            if requester.registered
                && worker.registered
                && (requester.discovered_counterparty || worker.discovered_counterparty)
                && (requester.connected_counterparty || worker.connected_counterparty)
            {
                return Ok::<(), String>(());
            }
        }
    })
    .await
    .map_err(|_| "timed out waiting for automatic rendezvous connection")??;

    println!(
        "Automatic CYPHES discovery passed: {} <-> {}",
        requester_peer_id, worker_peer_id
    );
    Ok(())
}
