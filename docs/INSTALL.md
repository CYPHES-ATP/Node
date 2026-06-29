# Install And Test

## Download The macOS Preview

Apple Silicon users can download the current packaged v0.5.6 developer DMGs
from:

- https://github.com/CYPHES-ATP/Node/releases/download/v0.5.6/CYPHES-v0.5.6-aarch64.dmg
- https://github.com/CYPHES-ATP/Node/releases/download/v0.5.6/CYPHES-Partner-v0.5.6-aarch64.dmg

Drag the app to Applications. These builds are ad hoc signed but not
Apple-notarized yet, so Control-click the app, select **Open**, then confirm
**Open** the first time. Windows and Linux binary distributions are not
available yet.

The current source tree is v0.6.1. Run from source to test Verified ATP
independent-verifier enforcement, Source Gateway fallback, and the local
pinned-source GitHub cache before a packaged v0.6.1 DMG is cut.

- **CYPHES** opens into the autonomous guardian cockpit. Select a local LM
  Studio or Ollama model and the node watches targets, creates non-duplicate
  work, auto-claims remote work, runs bounded audit skill passes, and receives
  receipt-backed ATP Credits after verifier acceptance.
- **CYPHES Partner** is the admin/protocol console for manual campaign
  creation, verification inspection, ATP proof logs, and final report export.
- The Autonomous Guardian Loop does not submit external reports or claim
  payouts; ATP Credits become earned only after accepted verifier receipts.

## GitHub Access For 24/7 Runs

CYPHES reads public GitHub repositories to resolve pinned commits and gather
read-only audit context. Unauthenticated GitHub requests are limited per public
IP, so multi-node home QA can exhaust the quota quickly.

For higher quota, configure a local GitHub token on each serious node using one
of:

```bash
export CYPHES_GITHUB_TOKEN=github_pat_...
printf '%s' 'github_pat_...' > ~/.cyphes/github.token
```

The app does not include a shared CYPHES GitHub token. A shared token in a DMG
would be public the moment the app ships.

v0.6.1 also caches immutable pinned GitHub tree and raw-file reads under
`~/.cyphes/source-cache/github/`. This reduces repeat quota burn for the same
repo/commit/path, but it is not a substitute for deploying the CYPHES Source
Gateway at `source.cyphes.com` for public-scale 24/7 operation.

For local Source Gateway QA:

```bash
export CYPHES_SOURCE_GATEWAY_URL=http://127.0.0.1:8080
cargo run --manifest-path source-gateway/Cargo.toml
```

When unset, CYPHES nodes try `https://source.cyphes.com` first and fall back to
direct GitHub reads if the gateway is unavailable.

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
