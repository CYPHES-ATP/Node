<a id="cyphes"></a>
<div align="center">
  <h1>CYPHES</h1>
  <p><strong>Proof of Cognition for agentic cyber workers.</strong></p>
  <p>CYPHES coordinates local AI security agents, independent verifier finality, and ATP credits through signed Cognition Proof receipts.</p>
  <p>
    <a href="ROADMAP.md"><img alt="Status: Active Testnet" src="https://img.shields.io/badge/status-active_testnet-00f6ff"></a>
    <a href="ROADMAP.md"><img alt="CYPHES: v0.15.6 testnet" src="https://img.shields.io/badge/CYPHES-v0.15.6_testnet-c7ff47"></a>
    <a href="docs/ATP_IMPLEMENTATION_STATUS.md"><img alt="ATP wire: v0.15.1" src="https://img.shields.io/badge/ATP_wire-v0.15.1-00f6ff"></a>
    <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-f5fbfa"></a>
  </p>
</div>

<p align="center">
  <img alt="CYPHES autonomous node cockpit" src="docs/images/cyphes-v0.15.1-app-final-clean-lean.png" width="100%">
</p>

## Download

The current testnet seed is **CYPHES v0.15.6**. CYPHES is the first
application in a Proof of Cognition network for agentic cyber workers: local AI
nodes perform scoped security labor, independent verifier nodes settle signed
receipts, and ATP credits become the unit of account for verified defense.
Nodes use the CYPHES-operated `source.cyphes.com` gateway first and fall back
to their own GitHub token/direct reads if it is unavailable. v0.15.6 keeps the
current `cyphes-dev-v0.7.7` testnet state and the `/cyphes/atp/0.15.1` labor
wire so existing testnet nodes can resync. It stops the autonomous worker from
spending local model runtime on work units the network has already settled,
releases stale claims before inference, reconciles a lagging local work-unit
status from signed verifications, and lets the quest seeder open new coverage
epochs when the current pool is fully cleared.

Verified ATP remains receipt-derived instead of SQLite-trusted: earned credits
require a signed contribution, a signed acceptance from an independent verifier,
and a deterministic allocation that matches the receipt data. Self-verification
can still test the local loop, but it cannot mint earned ATP.

macOS downloads:

- [Download CYPHES v0.15.6](https://github.com/CYPHES-ATP/Node/releases/download/v0.15.6/CYPHES_0.15.6_aarch64.dmg)
- [Download CYPHES v0.15.6 for Intel Macs](https://github.com/CYPHES-ATP/Node/releases/download/v0.15.6/CYPHES_0.15.6_x64.dmg)

Windows download:

- [Download CYPHES v0.15.6 for Windows x64](https://github.com/CYPHES-ATP/Node/releases/download/v0.15.6/CYPHES_0.15.6_x64-setup.exe)

These testnet builds are ad hoc signed but not Apple-notarized yet. After
dragging the app to Applications, Control-click the app, select **Open**, then
confirm **Open**. The Windows x64 setup build is unsigned and intended for
testnet use. Linux users should run from source for now.

Use **CYPHES** to join as a verifier by default. Select a local model and press
**Run** only when you want that node to start local audit work; press **Stop**
to return to verifier-only participation. The separate protocol/admin console remains available from source at
`campaign.html` for manual campaign creation, verification inspection, report
export, and ATP proof logs.

For 24/7 operation, CYPHES reads public GitHub source through
`source.cyphes.com`, where GitHub App credentials live server-side. CYPHES also
caches immutable pinned GitHub source reads locally. Serious node operators can
still configure a local fallback token with `CYPHES_GITHUB_TOKEN`,
`GITHUB_TOKEN`, `~/.cyphes/github.token`, or `githubToken` in
`~/.cyphes/settings.json`, but CYPHES never ships a shared embedded GitHub
token.

The receipt runtime completes one ATP-L1 repository-audit transaction:

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
- v0.5.7 Verified ATP is recomputed from signed contribution and verifier
  receipts. A local SQLite edit cannot create displayed earned ATP unless the
  signed artifacts match the deterministic allocation rules.
- Self-verification and single-node preview loops do not issue earned ATP.
  They remain useful for QA but show as pending/provisional until another ATP
  identity verifies the work.
- v0.6.1 Source Gateway service with server-side GitHub token or GitHub App
  installation-token support, shared read-through cache, ETag/Last-Modified
  revalidation, signed source manifest headers, Dockerfile, and compose file.
- Desktop node GitHub reads use the Source Gateway first and direct GitHub
  fallback second.
- v0.6.2 raises the default autonomous observation cap and model-audit cap to
  2880/day each for long-running testnet participation.
- v0.6.2 applies a deterministic 90% ATP quality deduction to parser-fallback
  contributions with zero structured findings, and shows that deduction in red
  in the live telemetry stream.
- v0.6.3 requires non-requester worker contributions to have an active signed
  work-unit claim before store-level ingest accepts them.
- v0.6.3 hardens verification bundle ingest against reused verification IDs,
  duplicate target verification mutation, and untrusted campaign snapshot
  credits.
- v0.6.4 fixes network verifier liveness by excluding self-authored pending
  receipts from local verifier duty and letting any independent online verifier
  settle eligible remote receipts.
- v0.6.5 rebroadcasts signed work-unit claims during network sync so missed
  claim prerequisites heal before contribution verification, and pauses new
  worker submissions when self-authored pending receipts outrun verifier
  settlement.
- v0.7.14 uses `/cyphes/atp/0.7.14` and
  `cyphes.repository-audit.v0.7.14`, keeps the current `cyphes-dev-v0.7.7`
  testnet state, defaults every app boot to verifier mode until Run is pressed
  in that session, adds Stop to return to verifier-only mode, keeps SQLite
  indexes for pending queue, claim sync, verifier duty, credit summary, and
  campaign snapshot queries, raises the provisional self-pending work queue to
  25 receipts, sends dependency-complete labor bundles for verifier pull, and
  keeps autonomous campaign seeding at 2400/day.
- v0.15.1 uses `/cyphes/atp/0.15.1` and
  `cyphes.repository-audit.v0.15.1`, keeps the current `cyphes-dev-v0.7.7`
  testnet state, signs standardized Cognition Proof packets into each new
  contribution, emits `cognition-proof.json` artifacts, and binds verifier
  settlement to autonomous finality packets so valid work settles immediately
  after independent verification.
- v0.15.2 keeps the same testnet, ATP stream, and rendezvous namespace as
  v0.15.1, but signs new Cognition Proof work through the legacy
  `defenseProof` wire alias/profile and emits both `defense-proof.json` and
  `cognition-proof.json` artifact entries. This is a compatibility hotfix for
  mixed verifier nodes that were rejecting renamed proof packets with
  contribution hash mismatches.
- v0.15.3 keeps the same testnet and ATP wire, persists explicit Run mode until
  Stop is pressed, removes the observation cap as a work-stopper, raises the
  autonomous campaign seed cap to 9600/day, opens new target-completion epochs
  when the current target pass is accepted, answers labor inventory with
  missing-object IDs before sending full bundles, prefers reachable public or
  relayed peer routes over stale private routes, requires the v0.15.3
  sparse-inventory capability before expensive labor-bundle ingest, and
  requires evidence-backed structured Cognition Proof output with one automatic
  JSON repair pass before parser-fallback ATP deductions apply.
- v0.15.4 keeps the same testnet and ATP wire, but adds a cheap duplicate and
  superseded-object preflight before expensive labor-bundle ingest. Known
  contribution IDs, known receipt hashes, repeated worker/work-unit receipts,
  terminal work units, known verification IDs, and already-verified
  contribution targets are skipped before signature/canonical-hash validation.
  Skips are telemetered as `labor_object_bundle_duplicate_skipped` and do not
  mutate credits, work status, or verification state. The cockpit progress bar
  now idles static when settlement is fully cleared to avoid unnecessary desktop
  repaints on verifier nodes. Live cockpit snapshots also skip trusted-credit
  recomputation, while reports and credit summaries still use the full verified
  credit path.
- v0.15.5 keeps the same testnet and ATP wire, and stops the autonomous worker
  from wasting local model runtime on already-settled work units. The store
  derives "settled" from the signed verifications table rather than a trusted
  status column, releases a stale local claim and reconciles a lagging
  work-unit status before any inference runs, and rejects new claims when a
  verification already settles the unit. The auto-worker skips
  snapshot-settled units, treats settled/already-claimed responses as
  skip-and-continue, and falls through when no runnable work remains so the
  quest seeder can open new coverage epochs instead of grinding a drained
  pool. Adds a tag-triggered GitHub Actions release workflow that builds the
  macOS Apple Silicon, macOS Intel, and Windows x64 artifacts on native
  runners.
- v0.15.6 keeps the same testnet and ATP wire, and rolls up network
  robustness and the first fair-launch step. It raises the libp2p response
  read limit to 32 MiB and byte-caps outbound labor bundles so a node that
  fell behind can complete a large catch-up sync instead of truncating at
  2 MiB. It excludes network-superseded contributions from worker
  backpressure so a node that forked off and rejoined resumes instead of
  pausing forever. It adds the Phase 1 fair-work policy: a node cannot claim
  or run work from a campaign it seeded, and a work unit is not claimable
  until a 60-second broadcast window elapses, so the seeder can no longer win
  its own units in the sync gap. All changes are wire-compatible with v0.15.5
  nodes; command-layer fair-work enforcement is for the honest autonomous
  loop, with stake-weighted on-chain assignment planned for the staking phase.
- Main CYPHES UI is centered on the autonomous cockpit: tokens/sec, pending and
  Verified ATP, progress, peers, target metadata, live protocol coverage, and
  receipt-backed event telemetry. Manual work-order controls are intentionally
  removed from the main node app.
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
- Autonomous Guardian Loop for 24/7 participation: verifier duty is on by
  default, while Auto Worker and Quest Seeder stay off until the operator
  presses Run. Run mode persists across restart until Stop is pressed. CYPHES watches Guardian Index v2,
  resolves GitHub commits, avoids duplicate target/commit campaigns within the
  current coverage epoch, starts the next epoch after a full target pass,
  auto-claims open remote work only while work mode is enabled, runs the
  selected local model under the runtime limit, signs contributions, and
  returns verifier receipts/ATP Credit allocations.
- Guardian Index v2 contains 165 structured public coverage targets with
  source signals, category, chains, static TVL/risk rank seed, repo URLs,
  focused paths, docs/security references, in-scope/out-of-scope text,
  criticality, and priority score. It is a bundled seed, not a live bounty or
  payout feed.
- Live network pulse showing active nodes, open work, pending ATP, Verified
  ATP, daily work progress, and local cognition rate. Pending ATP is
  provisional; Verified ATP only changes after accepted independent verifier
  receipts.
- Signed node contributions and signed verifier decisions.
- Standardized Cognition Proof packets for every new paid contribution,
  including target, claim, method, evidence, quality, and settlement metadata.
- Receipt-backed ATP Credits issued only after accepted independent
  verification results.
- Local pinned-source cache for GitHub repository metadata, moving commit
  resolution, immutable commit tree reads, and raw pinned file reads.
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
- No escrow, token transfer, release, refund, or dispute adapter. Verified ATP
  is off-chain receipt-derived accounting only, not a globally canonical
  token balance.
- No OpenClaw/Hermes runtime adapter yet. The current `Run Audit Pipeline` path
  is local-model-only through LM Studio or Ollama.
- No claim that local model output is automatically a valid vulnerability.
  Findings must be backed by signed artifacts and accepted verifier receipts
  before they appear in final reports.
- The Autonomous Guardian Loop does not submit external vulnerability reports,
  contact protocols, claim payouts, or move funds. Human approval is required
  before disclosure, escalation, liquidity-pool settlement, or external
  submission.
- `source.cyphes.com` is live with server-side CYPHES GitHub App credentials,
  but gateway hardening still needs metrics, cache limits, per-node quotas, and
  source manifest hashes embedded directly in contribution receipts.
- Source manifests are signed in gateway response headers, but source manifest
  hashes are not yet embedded directly in contribution receipts.
- No per-node Source Gateway quotas keyed by ATP identity yet.
- No private GitHub authorization.
- No key rotation, recovery, block list, rate-limit UI, or multi-device owner
  identity.
- The macOS testnet installer is downloadable but not Apple-notarized. The
  Windows x64 setup build is unsigned and intended for testnet use. There is
  no Linux binary distribution or automatic updater yet.

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
| `source-gateway/` | `source.cyphes.com` read-through GitHub cache and signed source manifest service |
| `network/` | Remotely updateable default-network manifest |

## Documentation

- [ATP implementation status](docs/ATP_IMPLEMENTATION_STATUS.md)
- [ATP Credit trust model](docs/ATP_CREDIT_TRUST_MODEL.md)
- [Proof of Protection](docs/PROOF_OF_PROTECTION.md)
- [Source Gateway](docs/SOURCE_GATEWAY.md)
- [Join the network](docs/JOIN_NETWORK.md)
- [Audit labor network](docs/AUDIT_LABOR_NETWORK.md)
- [Autonomous Guardian Loop](docs/GENESIS_AUTO_MODE.md)
- [Guardian Index](docs/GUARDIAN_INDEX.md)
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
