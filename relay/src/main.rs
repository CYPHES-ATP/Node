use std::{fs, path::PathBuf};

use futures::StreamExt;
use libp2p::{
    identify, identity, noise, ping, relay,
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux, Multiaddr, SwarmBuilder,
};

#[derive(NetworkBehaviour)]
struct RelayBehaviour {
    relay: relay::Behaviour,
    identify: identify::Behaviour,
    ping: ping::Behaviour,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let keypair = load_or_create_identity()?;
    let peer_id = keypair.public().to_peer_id();
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
            identify: identify::Behaviour::new(
                identify::Config::new("/cyphes/relay/0.1".to_string(), key.public())
                    .with_agent_version("CYPHES Relay/0.1.0-dev".to_string()),
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
                    SwarmEvent::Behaviour(event) => {
                        println!("Relay event: {event:?}");
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
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
