<a id="cyphes"></a>
<div align="center">
  <h1>CYPHES</h1>
  <p><strong>An autonomous digital labor economy.</strong></p>
  <p>Projects submit scoped work. Nodes produce signed artifacts. Verifiers arbitrate. Credits follow receipts.</p>
  <p>
    <a href="ROADMAP.md"><img alt="Status: Developer Preview" src="https://img.shields.io/badge/status-developer_preview-00f6ff"></a>
    <a href="https://github.com/CYPHES-ATP/Node/releases/tag/v0.5.4"><img alt="CYPHES: v0.5.4" src="https://img.shields.io/badge/CYPHES-v0.5.4-c7ff47"></a>
    <a href="docs/ATP_IMPLEMENTATION_STATUS.md"><img alt="ATP envelopes: v0.3" src="https://img.shields.io/badge/ATP_envelopes-v0.3-00f6ff"></a>
    <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-f5fbfa"></a>
  </p>
</div>

<p align="center">
  <img alt="CYPHES v0.5.4 desktop node" src="docs/images/CYPHES%20v0.5.4.png" width="100%">
</p>

## Download

The current developer release is **CYPHES v0.5.4**. It adds **Genesis Auto
Mode**: Auto Worker, Auto Verifier, and Quest Seeder toggles; a local DeFi
guardian target index; live cognition-rate/network pulse telemetry; enforced
Auto Worker runtime limits; and ATP accounting that stays pending until a
signed verifier receipt accepts the contribution.

Apple Silicon downloads:

- [Download CYPHES v0.5.4](https://github.com/CYPHES-ATP/Node/releases/download/v0.5.4/CYPHES-v0.5.4-aarch64.dmg)
- [Download CYPHES Requester v0.5.4](https://github.com/CYPHES-ATP/Node/releases/download/v0.5.4/CYPHES-Requester-v0.5.4-aarch64.dmg)

These developer builds are ad hoc signed but not Apple-notarized yet. After
dragging the app to Applications, Control-click the app, select **Open**, then
confirm **Open**. Windows and Linux users should run from source for now.

Use **CYPHES** to discover campaigns, claim individual work units, run local
AI audit passes, and receive receipt-backed ATP Credits. Use **CYPHES
Requester** to create campaigns, verify submitted work, and export reports.

The developer preview completes one ATP-L1 repository-audit transaction:

```text
DISCOVER -> NEGOTIATE -> NEGOTIATE -> ROUTE -> SETTLE -> ATTEST
```

Between `ROUTE` and `SETTLE`, the selected worker verifies requester-signed
context leases, downloads the pinned source archive, executes no repository
code, writes five audit artifacts inside the granted namespace, and returns a
signed result. The worker then emits a signed Proof of Cognition after
requester approval.

The desktop app also includes the v0.5 audit labor network: protocols can
create a pinned audit campaign with an audit brief, hashed reference
attachments, and an optional custom `SKILL.md` overlay; CYPHES decomposes it
into professional audit passes; remote worker nodes can claim individual work
units, run the local-model audit skill, and return signed contributions;
verifiers accept or reject signed work; and the app exports a final report
bundle generated only from accepted receipts.

## Verified Transaction

The repository contains a real successful receipt bundle at
[`protocol/fixtures/atp-l1-repository-audit.valid`](protocol/fixtures/atp-l1-repository-audit.valid).

It records:

- repository: `octocat/Hello-World`;
- commit: `7fd1a60b01f91b314f59955a4e4d4e80d8edf11d`;
- two independent Ed25519 ATP identities;
- six signed, hash-linked ATP envelopes;
- requester-signed repository-read and artifact-write leases;
- lease access evidence;
- five hashed audit artifacts;
- zero-value requester settlement approval;
- worker-signed Proof of Cognition.

Artifact Two independently returns:

```json
{
  "outcome": "OK",
  "reason_code": "OK",
  "receiptHash": "sha256:3bb23bf09d123a0d3e95f5467db3714a1d29a278d95d5e2757912c297aa02438",
  "eventRoot": "sha256:62a0af590d9d5240e2c271cf6b78b7e3b59999f1c257adac05ed580caeadc0a1"
}
```

## What Works

- Persistent Ed25519-backed libp2p identity.
- RFC 8785 JCS canonical ATP v0.3 envelopes.
- Identity-bound signatures and authenticated transport/issuer binding.
- Qualified SHA-256 event chaining from an explicit genesis hash.
- SQLite nonce, idempotency, transaction, contract, lease, result, and receipt
  persistence.
- TCP, WebSocket, QUIC, Noise, Yamux, Identify, Ping, mDNS, Circuit Relay v2,
  libp2p Rendezvous, and DCUtR.
- Automatic internet peer registration, discovery, and relayed dialing when a
  default network endpoint is published.
- Manual direct or relayed peer dialing as a fallback.
- Commit-before-ACK envelope delivery.
- Signed discovery, worker offer, and requester contract selection.
- Repository requests pinned to an exact Git commit.
- Requester-signed, scoped, expiring context leases.
- A deterministic repository worker that does not execute repository code.
- Signed worker execution results with embedded artifact bytes and hashes.
- Requester verification and zero-value `SETTLE`.
- Worker-signed `ATTEST` Proof of Cognition.
- Local protocol audit campaigns with pinned commits, scope, optional public
  program/reference URL, in-scope impacts, out-of-scope rules, audit brief text, hashed
  requester attachments, default skill-pack metadata, and optional custom
  `SKILL.md` overlay hash.
- Deterministic audit work units for scope mapping, repository inventory,
  dependency/config review, DeFi exploit-class review, finding validation, and
  final report sections.
- Remote campaign broadcast over libp2p so discovered CYPHES nodes see
  protocol campaigns without manually copying SQLite state.
- Signed, first-claim-wins work-unit claims that prevent another worker from
  submitting against a claimed unit.
- Remote worker flow: claim a work unit, run the claimed unit with LM Studio or
  Ollama on that worker's Mac, sign the contribution, and send it back to the
  requester.
- Requester verification sends signed verification results and receipt-backed
  ATP Credit allocations back to the contributing worker, including idempotent
  resend when that worker reconnects.
- Operator UI is centered on **Work Orders**: every campaign exposes each work
  unit, status, claimant, contribution count, verifier state, and per-unit
  claim/run controls.
- `campaign.html` provides a separate protocol/admin console for creating
  signed campaigns, viewing network state, ATP proof logs, receipt trails,
  protocol events, work-unit status, requester verification/export actions,
  and developer-facing ATP envelope metadata.
- Local-model `Run Audit Pipeline` execution through LM Studio or Ollama with
  hidden local endpoints, model discovery, progress events, tokens/sec
  measurement, effective skill hash, input hash, output hash, and signed
  contribution artifacts for each audit pass.
- Professional v0.4 audit passes for scope mapping, repository inventory,
  dependency/config review, smart-contract exploit-class review, finding
  validation, and final report synthesis.
- Genesis Auto Mode for 24/7 human-supervised participation: Auto Worker
  claims one open remote work unit, runs the selected local model, enforces the
  configured runtime limit, signs and submits the contribution; Auto Verifier
  accepts pending contributions for campaigns this node requested; Quest Seeder
  cycles one public DeFi guardian target per day from
  `protocol/targets/guardian-target-index.json`.
- Live network pulse showing active nodes, open work, pending ATP, earned ATP,
  daily work progress, and local cognition rate. Pending ATP is provisional;
  earned ATP only changes after accepted verifier receipts.
- Signed node contributions and signed verifier decisions.
- Receipt-backed ATP Credits issued only after accepted verification results.
- Final audit report bundle export with document control, methodology, audit
  pass matrix, evidence arbitration, findings register, coverage and negative
  findings, non-reportable/rejected lead appendix, runtime/receipt appendix,
  credit summary, and manifest.
- Portable Artifact Two-compatible receipt bundles under
  `~/.cyphes/receipts/<transaction-id>/`.
- A deployable combined relay/rendezvous service with one-node and automatic
  two-node smoke tests.

## What Is Not Production Ready

- The CYPHES-operated developer network is live on a dedicated public IPv4 and
  externally verified, but it currently depends on one relay/rendezvous
  machine in one region.
- Rendezvous discovers online nodes, not a durable or searchable work-order
  index.
- No durable offline mailbox or guaranteed retry after both peers disconnect.
- Campaign and claim delivery currently requires online peers; there is no
  durable, searchable, replicated work-order index yet.
- The worker is bounded by deterministic code paths and lease guards, but is
  not yet isolated in a hardened OS container or VM.
- No escrow, token transfer, release, refund, or dispute adapter. ATP Credits
  are off-chain receipt-backed accounting only.
- No OpenClaw/Hermes runtime adapter yet. The current `Run Audit Pipeline` path
  is local-model-only through LM Studio or Ollama.
- No claim that local model output is automatically a valid vulnerability.
  Findings must be backed by signed artifacts and accepted verifier receipts
  before they appear in final reports.
- Genesis Auto Mode does not submit external vulnerability reports, contact
  protocols, claim payouts, or move funds. Human approval is required before
  disclosure, escalation, liquidity-pool settlement, or external submission.
- No private GitHub authorization.
- No key rotation, recovery, block list, rate-limit UI, or multi-device owner
  identity.
- The macOS developer installer is downloadable but not Apple-notarized. There
  is no Windows/Linux binary distribution or automatic updater yet.

## Run The Desktop Node

Prerequisites:

- Node.js 20.19+ or 22.12+
- npm 10+
- Rust stable
- Tauri platform dependencies

```bash
git clone https://github.com/CYPHES-ATP/Node.git
cd Node
npm install
npm run tauri dev
```

For the protocol/admin console during development, open:

```text
http://localhost:1420/campaign.html
```

The node creates:

```text
~/.cyphes/identity.key
~/.cyphes/atp.sqlite3
~/.cyphes/receipts/
```

Do not copy `identity.key` between people or machines.

## Default Internet Network

At startup, CYPHES fetches
[`network/bootstrap.json`](network/bootstrap.json). Once its relay and
rendezvous addresses are published, a desktop node automatically:

1. connects to the CYPHES infrastructure identity;
2. reserves a Circuit Relay v2 address;
3. registers a signed peer record in the repository-audit namespace;
4. discovers and dials other online CYPHES nodes.

No manual address exchange is required for that path. The current manifest
points to the externally verified CYPHES-operated IPv4 developer endpoint at
`relay.cyphes.com`. Redundant relays and a durable work-order index remain
staging work.

## Operate The Network

Deploy the combined relay/rendezvous service on a public host with TCP and UDP
port `4001` open:

```bash
cd relay
export CYPHES_RELAY_PUBLIC_ADDR=/dns4/relay.example.com/tcp/4001
docker compose up --build -d
docker compose logs relay
```

The relay log prints its persistent peer ID. Configure each desktop node:

```bash
export CYPHES_RELAY_ADDR=/dns4/relay.example.com/tcp/4001/p2p/RELAY_PEER_ID
npm run tauri dev
```

For the manual fallback, share the circuit address shown by the node:

```text
/dns4/relay.example.com/tcp/4001/p2p/RELAY_PEER_ID/p2p-circuit/p2p/NODE_PEER_ID
```

Paste that address into **Connect to node** on the other client. The relay
routes encrypted libp2p streams; it cannot forge ATP signatures or receipts.

Verify automatic discovery between two fresh identities:

```bash
cargo run --manifest-path relay/Cargo.toml \
  --bin cyphes-network-smoke -- \
  /dns4/relay.example.com/tcp/4001/p2p/RELAY_PEER_ID
```

After the external smoke test passes, publish the endpoint:

```bash
./scripts/publish-network-config.sh \
  /dns4/relay.cyphes.com/tcp/4001 \
  RELAY_PEER_ID
```

To provision the first TCP endpoint on Fly.io instead:

```bash
cd relay
~/.fly/bin/flyctl auth login
./deploy/deploy-fly.sh cyphes-atp-network sjc personal 4 relay.cyphes.com
```

See [Join the CYPHES Network](docs/JOIN_NETWORK.md) and
[`relay/README.md`](relay/README.md).

## Reproduce The Proof

Run the real pinned-repository transaction:

```bash
./scripts/verify-atp-l1.sh
```

The script downloads the pinned GitHub archive, completes the six-envelope
transaction, exports a receipt bundle, and invokes a sibling Artifact Two
checkout. Set `ARTIFACT_TWO_DIR` if it lives elsewhere.

Offline validation:

```bash
python3 ../Artifact-Two/tools/verify_atp_bundle.py \
  protocol/fixtures/atp-l1-repository-audit.valid
```

## Repository Map

| Path | Responsibility |
| --- | --- |
| `src/App.tsx` | Native transaction workflow and truthful state labels |
| `src-tauri/src/atp.rs` | ATP envelopes, signing, verification, hashes, transitions |
| `src-tauri/src/audit_profile.rs` | Repository-audit contract and receipt profile |
| `src-tauri/src/audit_labor.rs` | Protocol campaigns, work units, contributions, verification, credits, reports |
| `src-tauri/src/audit_runtime.rs` | LM Studio/Ollama local model runtime, GitHub read-only context, skill output parsing |
| `src-tauri/src/store.rs` | SQLite event chain, replay defense, transaction projections |
| `src-tauri/src/worker.rs` | Context leases and deterministic repository worker |
| `src-tauri/src/bundle.rs` | Portable receipt and audit-report bundle export |
| `src-tauri/src/p2p.rs` | Direct, LAN, and relay-backed libp2p delivery |
| `src-tauri/src/commands.rs` | Tauri operations for the complete work order |
| `protocol/` | Schemas, skills, guardian target index, canonical fixtures, and verified ATP-L1 bundle |
| `relay/` | Combined public relay/rendezvous service and smoke clients |
| `network/` | Remotely updateable default-network manifest |

## Documentation

- [ATP implementation status](docs/ATP_IMPLEMENTATION_STATUS.md)
- [Join the network](docs/JOIN_NETWORK.md)
- [Audit labor network](docs/AUDIT_LABOR_NETWORK.md)
- [Genesis Auto Mode](docs/GENESIS_AUTO_MODE.md)
- [Repository audit profile](docs/REPOSITORY_AUDIT_PROFILE.md)
- [Developer guide](docs/DEVELOPER_GUIDE.md)
- [Network architecture](docs/ATP_NETWORK_ARCHITECTURE.md)
- [Roadmap](ROADMAP.md)
- [Contributing](CONTRIBUTING.md)
- [Security policy](SECURITY.md)

## Validation

```bash
npm run build
(cd src-tauri && cargo fmt --check)
(cd src-tauri && cargo test)
(cd relay && cargo fmt --check && cargo test)
```

Please do not add simulated peers, work orders, responses, reputation, payment,
credits, external payouts, exploit claims, or verification claims. Product state
must come from signed and committed ATP data or portable artifacts.

## License

[MIT](LICENSE)
