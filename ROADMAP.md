# CYPHES Roadmap

The roadmap is organized around one end-to-end ATP work order: a requester
contracts an independent worker to audit a public GitHub repository and
receives a verifiable result.

This is an engineering roadmap, not a promise of release dates.

## Status Legend

| Status | Meaning |
| --- | --- |
| Complete | Implemented and verified in the current repository |
| Partial | A narrow vertical slice exists; important protocol work remains |
| Planned | Architecture is documented but implementation has not started |

## Milestones

### 0. Honest Desktop Baseline

Status: **Complete**

- Remove simulated agents, activity, reputation, and responses.
- Make the Rust backend authoritative for transaction state.
- Persist identity and ATP state outside the WebView.
- Display whether state is local, received, or peer-acknowledged.
- Keep compensation visibly disconnected from a payment rail.

### 1. ATP Discovery and Negotiation

Status: **Partial**

Implemented:

- canonical ATP v0.3 envelopes;
- Ed25519 signing and verification;
- hash-linked events;
- nonce and idempotency replay protection;
- `DISCOVER`;
- worker `NEGOTIATE` offer;
- requester `NEGOTIATE` selection;
- repository requests pinned to an exact commit SHA;
- typed zero-value repository-audit contract;
- canonical contract hash persisted and accepted by the requester;
- JSON Schemas and canonical contract/receipt fixtures;
- commit-before-ACK LAN delivery.

Remaining:

- counters and rejection paths;
- expiry and clock-skew policy;
- deterministic reason-code registry;
- cross-implementation fixtures;
- key rotation and owner identity model.

### 2. Internet-Reachable P2P

Status: **Planned**

- Identify and Ping.
- Configurable bootstrap nodes.
- Rendezvous discovery.
- Relay v2 client and relay operations.
- AutoNAT and DCUtR.
- Direct connection upgrade where possible.
- Offline outbox retry and peer address persistence.

### 3. Routed Audit Work Session

Status: **Planned**

- ATP `ROUTE` event.
- Signed, bounded repository capability.
- Expiring and attenuating context leases.
- Isolated worker process.
- Repository clone and deterministic input snapshot.
- Cancellation and revocation handling.

### 4. Execution and Receipt Bundle

Status: **Planned**

- ATP `EXECUTE` event stream.
- Audit worker output contract.
- Artifact hashing and content-addressed storage.
- Proof of Cognition receipt bundle.
- Deterministic failure and partial-completion receipts.

### 5. Independent Verification

Status: **Planned**

- Integrate Artifact Two as an independent verifier.
- Verify signatures, chain continuity, replay, leases, and artifact roots.
- Export portable receipt bundles.
- Show verification reason codes in the desktop client.
- Add tamper and cross-node conformance tests.

### 6. Settlement and Attestation

Status: **Planned**

- ATP `SETTLE` and `ATTEST`.
- Zero-value settlement adapter first.
- Explicit requester approval.
- Dispute and rejection paths.
- Optional escrow adapter after the zero-value path verifies end to end.
- Optional sparse blockchain commitments for public notary and settlement.

### 7. Distribution

Status: **Planned**

- Signed macOS release.
- Linux verification and packages.
- Windows verification and installer.
- Reproducible build documentation.
- Automatic update policy.
- Public bootstrap and relay availability.

## Best First Contributions

- Add canonical ATP fixtures shared across Rust and another language.
- Add deterministic reason codes for current verification failures.
- Reproduce the LAN two-node test on Linux.
- Add Identify and Ping without changing ATP semantics.
- Add restart and disconnect tests around queued delivery.
- Improve accessibility of the desktop workflow.

See [CONTRIBUTING.md](CONTRIBUTING.md) for repository rules and
[docs/ATP_IMPLEMENTATION_STATUS.md](docs/ATP_IMPLEMENTATION_STATUS.md) for the
protocol-level matrix.
