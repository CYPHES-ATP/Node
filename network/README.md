# CYPHES Network Manifest

`bootstrap.json` is the default network control document fetched by CYPHES
desktop nodes at startup.

The document currently publishes the externally verified CYPHES-operated
developer endpoint. Its `online` status means relay and rendezvous passed the
automatic two-node smoke test over the public dedicated IPv4.

After deployment, publish the stable relay identity:

```bash
./scripts/publish-network-config.sh \
  /dns4/relay.cyphes.com/tcp/4001 \
  RELAY_PEER_ID
```

Use an explicit preview status for an IPv6-only endpoint:

```bash
./scripts/publish-network-config.sh \
  /dns6/relay.cyphes.com/tcp/4001 \
  RELAY_PEER_ID \
  online-ipv6-preview
```

The same infrastructure identity serves Circuit Relay v2 and libp2p
Rendezvous. Existing clients will fetch the updated manifest on their next
start without requiring a new desktop build.

Runtime overrides:

```bash
export CYPHES_RELAY_ADDR=/dns4/host/tcp/4001/p2p/PEER_ID
export CYPHES_RENDEZVOUS_ADDR="$CYPHES_RELAY_ADDR"
export CYPHES_RENDEZVOUS_NAMESPACE=cyphes.repository-audit.v0.7.5
```

Release builds can instead pin the address at compile time with
`CYPHES_DEFAULT_RELAY_ADDR` and `CYPHES_DEFAULT_RENDEZVOUS_ADDR`.
