use std::{fs, path::PathBuf};

use futures::StreamExt;
use libp2p::{
    identify, identity, noise, ping, relay, rendezvous,
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux, Multiaddr, SwarmBuilder,
};

#[derive(NetworkBehaviour)]
struct RelayBehaviour {
    relay: relay::Behaviour,
    rendezvous: rendezvous::server::Behaviour,
    identify: identify::Behaviour,
    ping: ping::Behaviour,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let keypair = load_or_create_identity()?;
    let peer_id = keypair.public().to_peer_id();
    if std::env::args().any(|argument| argument == "--print-peer-id") {
        println!("{peer_id}");
        return Ok(());
    }
    let port = std::env::var("CYPHES_RELAY_PORT")
        .unwrap_or_else(|_| "4001".to_string())
        .parse::<u16>()?;

    let mut swarm = SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_tcp(
            tcp::Config::default().nodelay(true),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_quic()
        .with_behaviour(move |key| RelayBehaviour {
            relay: relay::Behaviour::new(
                peer_id,
                relay::Config {
                    max_circuit_duration: std::time::Duration::from_secs(10 * 60),
                    max_circuit_bytes: 64 * 1024 * 1024,
                    ..relay::Config::default()
                },
            ),
            rendezvous: rendezvous::server::Behaviour::new(
                rendezvous::server::Config::default()
                    .with_max_registration_per_peer(4)
                    .with_max_registration_total(10_000),
            ),
            identify: identify::Behaviour::new(
                identify::Config::new("/cyphes/network/0.2".to_string(), key.public())
                    .with_agent_version("CYPHES Network/0.2.0-dev".to_string()),
            ),
            ping: ping::Behaviour::default(),
        })?
        .build();

    swarm.listen_on(format!("/ip4/0.0.0.0/tcp/{port}").parse::<Multiaddr>()?)?;
    swarm.listen_on(format!("/ip4/0.0.0.0/udp/{port}/quic-v1").parse::<Multiaddr>()?)?;
    let public_addr = std::env::var("CYPHES_RELAY_PUBLIC_ADDR")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.parse::<Multiaddr>())
        .transpose()?;
    if let Some(address) = public_addr.as_ref() {
        swarm.add_external_address(address.clone());
        println!("Advertising public relay address: {address}/p2p/{peer_id}");
    }

    println!("CYPHES relay peer id: {peer_id}");
    println!("Set CYPHES_RELAY_ADDR to a public address ending in /p2p/{peer_id}");
    println!("Rendezvous protocol active at /rendezvous/1.0.0");

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => break,
            event = swarm.select_next_some() => {
                match event {
                    SwarmEvent::NewListenAddr { address, .. } => {
                        if public_addr.is_none() {
                            swarm.add_external_address(address.clone());
                        }
                        println!("Listening on {address}/p2p/{peer_id}");
                    }
                    SwarmEvent::Behaviour(RelayBehaviourEvent::Relay(event)) => {
                        println!("Relay event: {event:?}");
                    }
                    SwarmEvent::Behaviour(RelayBehaviourEvent::Rendezvous(event)) => {
                        log_rendezvous_event(event);
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

fn log_rendezvous_event(event: rendezvous::server::Event) {
    match event {
        rendezvous::server::Event::DiscoverServed {
            enquirer,
            registrations,
        } => {
            println!(
                "Rendezvous discovery served: peer={enquirer} registrations={}",
                registrations.len()
            );
        }
        rendezvous::server::Event::DiscoverNotServed { enquirer, error } => {
            println!("Rendezvous discovery rejected: peer={enquirer} error={error:?}");
        }
        rendezvous::server::Event::PeerRegistered { peer, registration } => {
            println!(
                "Rendezvous peer registered: peer={peer} namespace={} addresses={} ttl={}",
                registration.namespace,
                registration.record.addresses().len(),
                registration.ttl
            );
        }
        rendezvous::server::Event::PeerNotRegistered {
            peer,
            namespace,
            error,
        } => {
            println!(
                "Rendezvous registration rejected: peer={peer} namespace={namespace} error={error:?}"
            );
        }
        rendezvous::server::Event::PeerUnregistered { peer, namespace } => {
            println!("Rendezvous peer unregistered: peer={peer} namespace={namespace}");
        }
        rendezvous::server::Event::RegistrationExpired(registration) => {
            println!(
                "Rendezvous registration expired: peer={} namespace={}",
                registration.record.peer_id(),
                registration.namespace
            );
        }
    }
}

fn load_or_create_identity() -> Result<identity::Keypair, Box<dyn std::error::Error>> {
    let path = identity_path()?;
    if path.exists() {
        let bytes = fs::read(path)?;
        return Ok(identity::Keypair::from_protobuf_encoding(&bytes)?);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let keypair = identity::Keypair::generate_ed25519();
    fs::write(path, keypair.to_protobuf_encoding()?)?;
    Ok(keypair)
}

fn identity_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Ok(data_dir) = std::env::var("CYPHES_RELAY_DATA_DIR") {
        return Ok(PathBuf::from(data_dir).join("identity.key"));
    }
    let home = dirs::home_dir().ok_or("could not resolve home directory")?;
    Ok(home.join(".cyphes-relay").join("identity.key"))
}
