# CYPHES Roadmap

CYPHES is moving from one verifiable repository-audit transaction toward a
protocol-facing autonomous audit labor network. The rule stays the same:
coordination, authority, work, verification, reports, and credits must trace to
signed ATP envelopes or portable artifacts.

## Completed Developer-Preview Slice

- Honest native UI with no simulated peers, work, reputation, or payment.
- Persistent Ed25519 identity and SQLite ATP event chain.
- RFC 8785 canonical envelopes, signatures, replay defense, expiry, and
  commit-before-ACK delivery.
- `DISCOVER`, bilateral `NEGOTIATE`, `ROUTE`, zero-value `SETTLE`, and
  worker `ATTEST`.
- Public GitHub requests pinned to exact commits.
- Typed audit contract and canonical contract hash.
- Requester-signed repository-read and artifact-write leases.
- Deterministic worker that executes no repository code.
- Signed worker result and five hashed artifacts.
- Portable Proof of Cognition bundle.
- Local protocol audit campaigns with pinned commits, scoped work units,
  signed contributions, signed verifier decisions, ATP Credit allocation, and
  final report bundle export.
- Local-model v0.4 `Run Audit Pipeline` runtime for LM Studio and Ollama,
  including model discovery, progress, tokens/sec, v0.4 skill hash, input hash,
  output hash, and signed contribution artifacts for each audit pass.
- v0.5 campaign guidance fields: Audit Brief, hashed requester attachments,
  default CYPHES skill-pack metadata, and optional custom `SKILL.md` overlay
  hash included in the effective prompt/input hash.
- Remote campaign broadcast over libp2p request/response.
- Signed work-unit claims with first-claim-wins persistence and contribution
  enforcement so another worker cannot submit against a claimed unit.
- Remote worker flow: claim a campaign work unit, run it with the local model
  on that worker node, and send the signed contribution back to the requester.
- Signed verification-result bundles return accepted/rejected decisions and
  ATP Credit allocations to the contributing worker, with idempotent resend on
  reconnect.
- Desktop operator UI now presents **Work Orders** as the primary surface:
  per-unit status, claimant, contribution count, verifier state, claim buttons,
  and run buttons for claimed work.
- `campaign.html` separates protocol/admin campaign creation, network stats,
  ATP proof logs, receipt trails, protocol events, and developer ATP envelope
  inspection from the worker cockpit.
- Professional audit passes for scope mapping, repository inventory,
  dependency/config review, smart-contract exploit-class review, finding
  validation, and final report synthesis.
- Professional markdown report export with document control, audit pass matrix,
  evidence arbitration, findings register, coverage and negative findings,
  non-reportable/rejected lead appendix, runtime/receipt appendix, and ATP
  Credit allocation summary.
- Independent Artifact Two verification of the committed real fixture.
- TCP, WebSocket, QUIC, mDNS, Identify, Ping, Relay v2, Rendezvous, and DCUtR.
- Docker-ready relay/rendezvous service with reservation and automatic
  two-node discovery smoke tests.
- Public dedicated IPv4 relay/rendezvous endpoint at `relay.cyphes.com`.
- Downloadable ad hoc-signed Apple Silicon macOS developer DMG.

## 1. Audit Labor Network

Status: **Partial**

- Persist remote campaign/work-unit discovery in a durable searchable work-order
  index instead of only online peer broadcast.
- Split verification and challenge handling across independently claimable
  verifier work units.
- Add OpenClaw/Hermes runtime adapters for nodes that want advanced tool
  orchestration beyond the built-in LM Studio/Ollama local model path.
- Store audit skill hashes, runtime descriptors, model identifiers, tool-access
  logs, output hashes, and evidence references in contribution receipts.
- Add verifier-node queues, challenge windows, revision requests, and duplicate
  finding resolution.
- Improve protocol/requester UX for scope templates, file attachment import,
  PDF parsing, final report review, and claim/revision inspection.
- Keep bounty allocation as a signed placeholder until settlement is designed.

## 2. Staging Network

Status: **Partial**

- Deploy a second independent relay/rendezvous endpoint.
- Add DNS endpoint rotation and health telemetry.
- Add signed capability cards and namespace-aware capability matching.
- Persist known peer addresses and retry queued audience-specific delivery.
- Run the full transaction across two machines on different consumer networks.
- Add AutoNAT and verify direct DCUtR upgrades.

## 3. Worker Hardening

Status: **Partial**

- Move checkout and scanners into a hardened process, container, or VM.
- Enforce CPU, memory, disk, process, and wall-clock limits.
- Deny network after source fetch at the operating-system boundary.
- Add language-specific static analysis without executing untrusted build
  scripts.
- Add cancellation, timeout, live revocation, and partial-failure receipts.

## 4. ATP Conformance

Status: **Partial**

- Signed `ADVERTISE` cards.
- Counteroffer, reject, revoke, cancel, expire, and dispute paths.
- Complete reason-code registry and cross-language signed vectors.
- Clock-skew policy, key rotation, owner binding, and recovery.
- Lease attenuation, subleases, and revocation propagation.
- More than one valid terminal sequence in Artifact Two.

## 5. Reliable Market

Status: **Planned**

- Encrypted store-and-forward mailbox.
- Replicated public work-order index with signed expiry.
- Rate limits, block lists, abuse reporting, and resource admission policy.
- Selective receipt disclosure and reputation derived from verified evidence.

## 6. Settlement

Status: **Planned**

- Choose one low-cost reference chain behind an adapter.
- Bind wallet owner and ATP Ed25519 identity.
- Prove escrow or payer authorization before costly work.
- Release, refund, timeout, and dispute against `contractHash`, `eventRoot`, and
  `receiptHash`.
- Keep ATP envelopes, leases, artifacts, and private context off-chain.

## 7. Distribution

Status: **Partial**

- Signed and notarized macOS build.
- Linux packages and Windows installer.
- Reproducible build evidence.
- Automatic update and rollback policy.
- Operational security and incident runbooks.

## Best Contributions Now

- Reproduce the relay-backed two-node transaction on Linux or Windows.
- Improve local model context selection, output validation, multi-pass synthesis,
  and verifier review before adding more runtime providers.
- Connect OpenClaw/Hermes as an advanced adapter while preserving the signed
  contribution and verification receipt shape.
- Add a durable work-order index and reliable resend for campaigns, claims, and
  contributions when peers are not simultaneously online. Verification/credit
  result resend is now implemented for reconnecting workers.
- Harden the worker boundary without changing the receipt profile.
- Add deterministic negative fixtures for invalid leases and worker results.
- Add peer persistence and reliable resend.
- Implement signed `ADVERTISE` cards and rendezvous discovery.
- Improve accessibility and receipt inspection in the desktop client.

See [CONTRIBUTING.md](CONTRIBUTING.md) and
[docs/ATP_IMPLEMENTATION_STATUS.md](docs/ATP_IMPLEMENTATION_STATUS.md).
