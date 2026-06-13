# CYPHES Network Service

The service combines libp2p Circuit Relay v2 and Rendezvous on one persistent
Ed25519 identity. It stores only that identity and in-memory rendezvous
registrations. It does not store ATP work orders, contracts, results, or
receipts.

Run it on a public host with TCP and UDP port `4001` open:

```bash
docker compose up --build -d
docker compose logs relay
```

The first log lines print the persistent relay peer ID. Given public hostname
`relay.example.com`, configure each desktop node with:

```bash
export CYPHES_RELAY_ADDR=/dns4/relay.example.com/tcp/4001/p2p/RELAY_PEER_ID
npm run tauri dev
```

When the relay host is behind a cloud firewall or NAT, advertise the public
address in the relay container:

```bash
export CYPHES_RELAY_PUBLIC_ADDR=/dns4/relay.example.com/tcp/4001
docker compose up --build -d
```

To connect to another node through the relay, paste its advertised circuit
address into the CYPHES client:

```text
/dns4/relay.example.com/tcp/4001/p2p/RELAY_PEER_ID/p2p-circuit/p2p/NODE_PEER_ID
```

Persist `/var/lib/cyphes-relay`. Replacing `identity.key` changes the relay
peer ID and invalidates previously shared relay addresses.

Verify a deployment from another machine:

```bash
cargo run --bin cyphes-relay-smoke -- \
  /dns4/relay.example.com/tcp/4001/p2p/RELAY_PEER_ID
```

The command exits successfully only after the relay accepts a circuit
reservation.

Verify the complete automatic-discovery path:

```bash
cargo run --bin cyphes-network-smoke -- \
  /dns4/relay.example.com/tcp/4001/p2p/RELAY_PEER_ID
```

This launches two fresh identities. Both reserve relay circuits and register
signed peer records; they then discover and connect to each other without
sharing addresses.

## Production Host

Build and install on a public Linux host:

```bash
cargo build --release
sudo ./deploy/install-systemd.sh \
  target/release/cyphes-relay \
  /dns4/relay.cyphes.com/tcp/4001
```

Create an `A` or `AAAA` record for `relay.cyphes.com`, open `4001/tcp` and
`4001/udp`, and run `cyphes-network-smoke` from a different network. Only then
publish the address in `../network/bootstrap.json`.

## Fly.io Bootstrap

The repository also includes a first-network deployment for Fly.io. It keeps
one machine running, persists the relay identity on a volume, and exposes raw
libp2p TCP on a dedicated IPv4 address:

```bash
curl -L https://fly.io/install.sh | sh
~/.fly/bin/flyctl auth login
./deploy/deploy-fly.sh cyphes-atp-network sjc personal 4
```

The script creates the app, one-gigabyte identity volume, and dedicated IPv4;
deploys the container; reads the stable relay peer ID; runs the automatic
two-node smoke test; and prepares `../network/bootstrap.json`.

Fly requires a dedicated IPv4 for raw TCP without Fly TLS termination. The
initial public endpoint uses `<app>.fly.dev`, so CYPHES DNS can be attached
after launch without delaying network availability. The Fly deployment exposes
TCP only; the VPS/systemd deployment remains the path for public QUIC/UDP.

For an IPv6-only developer preview before billing is enabled, pass `6` as the
last argument. The generated manifest is marked `online-ipv6-preview`; do not
position that endpoint as universally reachable.

Review current Fly pricing before deployment:
https://fly.io/docs/about/pricing/
