# Join the CYPHES Network

## Current Network Model

CYPHES nodes can connect in three real ways:

1. automatic mDNS discovery on one LAN;
2. automatic libp2p Rendezvous discovery through a configured internet relay;
3. direct or relayed multiaddress dialing as a fallback.

There is no central work-order database. Each participant verifies signed ATP
messages and commits its own SQLite event chain.

The default network manifest is fetched from `network/bootstrap.json`. It
currently points to the externally verified CYPHES-operated IPv4 developer
endpoint. A node with ordinary internet access can use it without manually
entering another node's address.

## Install

```bash
git clone https://github.com/CYPHES-ATP/Node.git
cd Node
npm install
npm run tauri dev
```

Each node creates:

```text
~/.cyphes/identity.key
~/.cyphes/atp.sqlite3
~/.cyphes/receipts/
```

`identity.key` is the node's signing authority. Never share it.

## Connect On A LAN

Start the app on two computers on the same broadcast network. mDNS should
populate the connected-peer count automatically. Guest Wi-Fi may isolate
clients.

## Connect Through A Relay

On a public Linux host:

```bash
cd relay
export CYPHES_RELAY_PUBLIC_ADDR=/dns4/relay.example.com/tcp/4001
docker compose up --build -d
docker compose logs relay
```

Open `4001/tcp` and `4001/udp`. Persist the relay data volume so the relay peer
ID does not change.

Verify the deployment from another machine:

```bash
cargo run --manifest-path relay/Cargo.toml \
  --bin cyphes-relay-smoke -- \
  /dns4/relay.example.com/tcp/4001/p2p/RELAY_PEER_ID
```

Start each desktop node with:

```bash
export CYPHES_RELAY_ADDR=/dns4/relay.example.com/tcp/4001/p2p/RELAY_PEER_ID
npm run tauri dev
```

The node reserves and advertises a circuit address:

```text
/dns4/relay.example.com/tcp/4001/p2p/RELAY_PEER_ID/p2p-circuit/p2p/NODE_PEER_ID
```

Both nodes register signed circuit addresses in the same rendezvous namespace
and discover each other automatically. Manual address entry remains available
if rendezvous is unavailable. The nodes authenticate each other end to end.
The relay sees transport metadata and encrypted bytes but cannot create valid
ATP events.

Verify the automatic path before publishing an endpoint:

```bash
cargo run --manifest-path relay/Cargo.toml \
  --bin cyphes-network-smoke -- \
  /dns4/relay.example.com/tcp/4001/p2p/RELAY_PEER_ID
```

For a managed first endpoint, use the Fly.io deployment:

```bash
cd relay
~/.fly/bin/flyctl auth login
./deploy/deploy-fly.sh cyphes-atp-network sjc personal 4
```

This provisions an always-on TCP relay/rendezvous node with a persistent
identity, verifies automatic discovery, and prepares the default manifest.
The script does not commit the manifest for you.

## Complete One Audit

1. Requester posts a public GitHub repository.
2. Worker selects **Offer to audit**.
3. Requester selects **Select worker**.
4. Requester selects **Issue context lease**.
5. Worker selects **Run bounded audit**.
6. Requester waits for the signed result and selects
   **Approve verified result**.
7. Worker automatically emits `ATTEST`.
8. Both nodes show `ATTESTED` and export a receipt under
   `~/.cyphes/receipts/<transaction-id>/`.

The proposed USDC amount is not transferred. The current contract settles at
zero value.

## State Meaning

| State | Meaning |
| --- | --- |
| `DISCOVERED` | Request is signed and committed |
| `NEGOTIATING` | Worker offer is committed |
| `NEGOTIATED` | Requester selected the exact contract hash |
| `ROUTED` | Worker has verified active requester-signed leases |
| `ROUTED` plus result hash | Signed worker result is stored and verified |
| `SETTLED` | Requester approved the verified result at zero value |
| `ATTESTED` | Worker receipt is committed and a bundle is exported |

## Two Identities On One Machine

```bash
# First node
CYPHES_DATA_DIR=/tmp/cyphes-requester npm run tauri dev

# Second node, after the binary is built
CYPHES_DATA_DIR=/tmp/cyphes-worker src-tauri/target/debug/cyphes-desktop
```

Use separate data directories. One identity must not run as both parties.

## Troubleshooting

**Relay reservation fails**

- Confirm the relay advertises `CYPHES_RELAY_PUBLIC_ADDR`.
- Confirm the relay peer ID in the client address matches the log.
- Open both TCP and UDP port `4001`.
- Run `cyphes-relay-smoke` from outside the relay host.

**Nodes connect but do not exchange a work order**

- Confirm both support `/cyphes/atp/0.3`.
- Keep both online; offline mailbox delivery is not implemented.
- Confirm the target multiaddress ends with the counterparty node peer ID.
- Check the client notice for signature, `prev`, expiry, or lease rejection.

**Audit execution fails**

- Confirm the contract and lease have not expired.
- Confirm the pinned GitHub archive remains publicly downloadable.
- The worker rejects archives over 100 MiB, unsafe paths, links, and more than
  25,000 scanned files.

**Reset a development identity**

```bash
mv ~/.cyphes ~/.cyphes.backup
```

The replacement node has a new ATP identity and no authority over old
transactions.
