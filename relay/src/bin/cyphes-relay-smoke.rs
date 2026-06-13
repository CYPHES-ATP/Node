use std::time::Duration;

use futures::StreamExt;
use libp2p::{
    identify, identity, noise, ping, relay,
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux, Multiaddr, SwarmBuilder,
};

#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "ClientEvent")]
struct ClientBehaviour {
    relay: relay::client::Behaviour,
    identify: identify::Behaviour,
    ping: ping::Behaviour,
}

#[allow(dead_code)]
#[derive(Debug)]
enum ClientEvent {
    Relay(relay::client::Event),
    Identify(identify::Event),
    Ping(ping::Event),
}

impl From<relay::client::Event> for ClientEvent {
    fn from(event: relay::client::Event) -> Self {
        Self::Relay(event)
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let relay_addr = std::env::args()
        .nth(1)
        .ok_or("usage: cyphes-relay-smoke <relay-multiaddr>")?
        .parse::<Multiaddr>()?;
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
            identify: identify::Behaviour::new(identify::Config::new(
                "/cyphes/relay-smoke/0.1".to_string(),
                key.public(),
            )),
            ping: ping::Behaviour::default(),
        })?
        .build();

    let relay_peer_id = relay_addr
        .iter()
        .find_map(|protocol| match protocol {
            libp2p::multiaddr::Protocol::P2p(peer_id) => Some(peer_id),
            _ => None,
        })
        .ok_or("relay multiaddress must end with /p2p/RELAY_PEER_ID")?;
    swarm.dial(relay_addr.clone())?;

    let accepted = tokio::time::timeout(Duration::from_secs(20), async {
        let mut listener_started = false;
        loop {
            match swarm.select_next_some().await {
                SwarmEvent::Behaviour(ClientEvent::Relay(event)) => {
                    println!("Relay client event: {event:?}");
                    if let relay::client::Event::ReservationReqAccepted { relay_peer_id, .. } =
                        event
                    {
                        return Ok::<_, String>(relay_peer_id);
                    }
                }
                SwarmEvent::NewListenAddr { address, .. } => {
                    println!("Relay circuit address: {address}");
                }
                SwarmEvent::ConnectionEstablished {
                    peer_id: connected, ..
                } if connected == relay_peer_id && !listener_started => {
                    let mut circuit = relay_addr.clone();
                    circuit.push(libp2p::multiaddr::Protocol::P2pCircuit);
                    swarm
                        .listen_on(circuit)
                        .map_err(|error| format!("could not start relay listener: {error}"))?;
                    listener_started = true;
                }
                SwarmEvent::OutgoingConnectionError { error, .. } => {
                    return Err(format!("relay connection failed: {error}"));
                }
                SwarmEvent::ListenerError { error, .. } => {
                    return Err(format!("relay listener failed: {error}"));
                }
                _ => {}
            }
        }
    })
    .await
    .map_err(|_| "timed out waiting for relay reservation")??;

    println!("Relay reservation accepted by {accepted} for node {peer_id}");
    Ok(())
}
