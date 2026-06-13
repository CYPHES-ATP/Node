# CYPHES Roadmap

The roadmap is organized around one verifiable ATP work order rather than
marketplace breadth.

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
- Independent Artifact Two verification of the committed real fixture.
- TCP, WebSocket, QUIC, mDNS, Identify, Ping, Relay v2, Rendezvous, and DCUtR.
- Docker-ready relay/rendezvous service with reservation and automatic
  two-node discovery smoke tests.
- Public dedicated IPv4 relay/rendezvous endpoint at `relay.cyphes.com`.
- Downloadable ad hoc-signed Apple Silicon macOS developer DMG.

## 1. Staging Network

Status: **Partial**

- Deploy a second independent relay/rendezvous endpoint.
- Add DNS endpoint rotation and health telemetry.
- Add signed capability cards and namespace-aware capability matching.
- Persist known peer addresses and retry queued audience-specific delivery.
- Run the full transaction across two machines on different consumer networks.
- Add AutoNAT and verify direct DCUtR upgrades.

## 2. Worker Hardening

Status: **Partial**

- Move checkout and scanners into a hardened process, container, or VM.
- Enforce CPU, memory, disk, process, and wall-clock limits.
- Deny network after source fetch at the operating-system boundary.
- Add language-specific static analysis without executing untrusted build
  scripts.
- Add cancellation, timeout, live revocation, and partial-failure receipts.

## 3. ATP Conformance

Status: **Partial**

- Signed `ADVERTISE` cards.
- Counteroffer, reject, revoke, cancel, expire, and dispute paths.
- Complete reason-code registry and cross-language signed vectors.
- Clock-skew policy, key rotation, owner binding, and recovery.
- Lease attenuation, subleases, and revocation propagation.
- More than one valid terminal sequence in Artifact Two.

## 4. Reliable Market

Status: **Planned**

- Encrypted store-and-forward mailbox.
- Replicated public work-order index with signed expiry.
- Rate limits, block lists, abuse reporting, and resource admission policy.
- Selective receipt disclosure and reputation derived from verified evidence.

## 5. Settlement

Status: **Planned**

- Choose one low-cost reference chain behind an adapter.
- Bind wallet owner and ATP Ed25519 identity.
- Prove escrow or payer authorization before costly work.
- Release, refund, timeout, and dispute against `contractHash`, `eventRoot`, and
  `receiptHash`.
- Keep ATP envelopes, leases, artifacts, and private context off-chain.

## 6. Distribution

Status: **Partial**

- Signed and notarized macOS build.
- Linux packages and Windows installer.
- Reproducible build evidence.
- Automatic update and rollback policy.
- Operational security and incident runbooks.

## Best Contributions Now

- Reproduce the relay-backed two-node transaction on Linux or Windows.
- Harden the worker boundary without changing the receipt profile.
- Add deterministic negative fixtures for invalid leases and worker results.
- Add peer persistence and reliable resend.
- Implement signed `ADVERTISE` cards and rendezvous discovery.
- Improve accessibility and receipt inspection in the desktop client.

See [CONTRIBUTING.md](CONTRIBUTING.md) and
[docs/ATP_IMPLEMENTATION_STATUS.md](docs/ATP_IMPLEMENTATION_STATUS.md).
