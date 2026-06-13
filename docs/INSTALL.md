# Install And Test

The current distribution path is source-only. There are no signed release
binaries yet.

## Native Development

```bash
npm install
npm run tauri dev
```

## Browser Preview

```bash
npm run dev
```

The browser preview is visual-only. Signing, SQLite, execution, and networking
require the Tauri app.

## ATP-L1 Proof

With Artifact Two checked out beside this repository:

```bash
./scripts/verify-atp-l1.sh
```

This runs the ignored network integration test against the pinned
`octocat/Hello-World` archive and verifies the resulting receipt bundle.

Verify the committed bundle without network access:

```bash
python3 ../Artifact-Two/tools/verify_atp_bundle.py \
  protocol/fixtures/atp-l1-repository-audit.valid
```

## Relay And Automatic Discovery

```bash
cd relay
cargo test
docker compose up --build
```

Verify two-node rendezvous discovery:

```bash
cargo run --bin cyphes-network-smoke -- \
  /ip4/127.0.0.1/tcp/4001/p2p/RELAY_PEER_ID
```

See [JOIN_NETWORK.md](JOIN_NETWORK.md) for public deployment, default-network
publication, and the complete two-node UI flow.

## Build Checks

```bash
npm run build
(cd src-tauri && cargo fmt --check && cargo test)
(cd relay && cargo fmt --check && cargo test)
```
