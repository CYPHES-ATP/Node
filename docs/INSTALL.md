# Install And Test

## Download The Preview

macOS users can download the current packaged v0.7.3 developer DMGs from:

- Apple Silicon: https://github.com/CYPHES-ATP/Node/releases/download/v0.7.3/CYPHES_0.7.3_aarch64.dmg
- Intel: https://github.com/CYPHES-ATP/Node/releases/download/v0.7.3/CYPHES_0.7.3_x64.dmg

Windows users can download the current packaged v0.7.3 x64 NSIS setup build
from:

- Windows x64: https://github.com/CYPHES-ATP/Node/releases/download/v0.7.3/CYPHES_0.7.3_x64-setup.exe

Drag the app to Applications. These builds are ad hoc signed but not
Apple-notarized yet, so Control-click the app, select **Open**, then confirm
**Open** the first time. The Windows setup build is unsigned and intended for
testnet use. Linux binary distributions are not available yet.

The current source tree is v0.7.3. Run from source to test Verified ATP
independent-verifier enforcement, the isolated testnet hotfixes, the separate
`campaign.html` admin console, and the local pinned-source GitHub cache.

- **CYPHES** opens into the autonomous guardian cockpit. Select a local LM
  Studio or Ollama model and the node watches targets, creates non-duplicate
  work, auto-claims remote work, runs bounded audit skill passes, and receives
  receipt-backed ATP Credits after verifier acceptance.
- The separate `campaign.html` admin/protocol console is available from source
  for manual campaign creation, verification inspection, ATP proof logs, and
  final report export.
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

v0.6.2 also caches immutable pinned GitHub tree and raw-file reads under
`~/.cyphes/source-cache/github/`. For public-scale 24/7 operation, CYPHES nodes
read through the live Source Gateway at `source.cyphes.com`, where GitHub App
credentials stay server-side.

For local Source Gateway QA:

```bash
export CYPHES_SOURCE_GATEWAY_URL=http://127.0.0.1:8080
cargo run --manifest-path source-gateway/Cargo.toml
```

When unset, CYPHES nodes try `https://source.cyphes.com` first, then the Fly
seed fallback at `https://cyphes-source-gateway.fly.dev`, then direct GitHub
reads if gateways are unavailable.

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
