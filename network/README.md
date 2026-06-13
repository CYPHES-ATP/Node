# CYPHES Network Manifest

`bootstrap.json` is the default network control document fetched by CYPHES
desktop nodes at startup.

The document is intentionally committed with null infrastructure addresses
until a CYPHES-operated public host has passed the external smoke test. This
keeps released clients honest: they remain usable on a LAN and by explicit
multiaddress, but do not claim to be connected to a nonexistent service.

After deployment, publish the stable relay identity:

```bash
./scripts/publish-network-config.sh \
  /dns4/relay.cyphes.com/tcp/4001 \
  RELAY_PEER_ID
```

The same infrastructure identity serves Circuit Relay v2 and libp2p
Rendezvous. Existing clients will fetch the updated manifest on their next
start without requiring a new desktop build.

Runtime overrides:

```bash
export CYPHES_RELAY_ADDR=/dns4/host/tcp/4001/p2p/PEER_ID
export CYPHES_RENDEZVOUS_ADDR="$CYPHES_RELAY_ADDR"
export CYPHES_RENDEZVOUS_NAMESPACE=cyphes.repository-audit.v0.1
```

Release builds can instead pin the address at compile time with
`CYPHES_DEFAULT_RELAY_ADDR` and `CYPHES_DEFAULT_RENDEZVOUS_ADDR`.
