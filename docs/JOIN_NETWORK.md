# Join the CYPHES Network

## What Joining Means Today

CYPHES Audit Node is currently a LAN-only developer network. There is no public
bootstrap server, relay, hosted marketplace, account system, or downloadable
signed release.

A node can currently:

1. create a persistent signing identity;
2. discover another node on the same LAN using mDNS;
3. sign and commit an ATP audit request;
4. send that request directly to the discovered peer;
5. receive an ACK only after the peer verifies and commits it;
6. exchange a worker offer and requester selection as signed ATP events.

## Requirements

- Both computers must be on the same local network and broadcast domain.
- Local firewall rules must allow the Tauri application and local peer traffic.
- Node.js 20.19+ or 22.12+, npm 10+, Rust stable, and Tauri platform
  dependencies.
- The current verified desktop path is macOS.

## Install From Source

```bash
git clone https://github.com/CYPHES-ATP/Node.git
cd Node
npm install
npm run tauri dev
```

On first launch the node creates:

```text
~/.cyphes/identity.key
~/.cyphes/atp.sqlite3
```

The identity key is the node's signing authority. Do not copy it to another
person, commit it, or use the same file for two simultaneous nodes.

## Join From Two Computers

1. Clone and start the app on both computers.
2. Confirm each app changes from `0 LAN peers` to `1 LAN peer`.
3. On the requester, enter a public GitHub repository URL and compensation.
4. Select **Sign and post request**.
5. Confirm the requester initially shows a locally signed state.
6. Confirm the worker receives the same repository request.
7. Confirm the requester shows `1 peer receipt`.
8. On the worker, select **Offer to audit**.
9. Confirm the requester shows the worker offer.
10. On the requester, select **Select worker**.
11. Confirm both nodes show `NEGOTIATED`.

No payment is transferred.

## Run Two Identities on One Machine

Start the primary application:

```bash
npm run tauri dev
```

After the Rust binary has been built, start a second identity from another
terminal:

```bash
mkdir -p /tmp/cyphes-peer-2
cd src-tauri
CYPHES_DATA_DIR=/tmp/cyphes-peer-2 target/debug/cyphes-desktop
```

The second node stores its identity and database under
`/tmp/cyphes-peer-2`. Use a persistent directory instead of `/tmp` if the
identity should survive a restart.

## How to Read the UI

| UI state | Meaning |
| --- | --- |
| `Signed + SQLite` | The native backend owns signed ATP state in SQLite |
| `0 LAN peers` | No other node is currently discovered |
| `SIGNED LOCALLY, NO PEER RECEIPT` | Local commit succeeded; no peer has acknowledged it |
| `1 PEER RECEIPT` | One peer verified and committed the event, then returned an ACK |
| `DISCOVERED` | A valid ATP `DISCOVER` event is committed |
| `NEGOTIATING` | A valid worker offer is committed |
| `NEGOTIATED` | The requester selected the offered worker |
| `Payment rail: Not connected` | Compensation is a term only |

## Troubleshooting

### Nodes Stay at Zero Peers

- Confirm both nodes are on the same LAN.
- Avoid guest Wi-Fi networks that isolate clients.
- Check macOS firewall prompts and allow the application.
- Confirm only one node is using each identity/data directory.
- Restart both nodes after changing network interfaces.

### Request Is Signed but Has No Receipt

- Confirm the peer count is non-zero.
- Keep both nodes running long enough for discovery and resend.
- Check the receiving node's database exists and is writable.
- Confirm both builds support `/cyphes/atp/0.3`.

### Reset a Development Identity

Stop the node and move the data directory instead of deleting it immediately:

```bash
mv ~/.cyphes ~/.cyphes.backup
```

The next launch creates a new identity and empty database. The new node is not
the same ATP issuer as the old node.

## Join Development

The next network milestone is public internet reachability through bootstrap,
rendezvous, Relay v2, AutoNAT, and direct upgrade. The next protocol milestone
is a complete repository-audit work order through verification and attestation.

See [CONTRIBUTING.md](../CONTRIBUTING.md) for contribution tracks and
[ATP_NETWORK_ARCHITECTURE.md](ATP_NETWORK_ARCHITECTURE.md) for the implementation
roadmap.
