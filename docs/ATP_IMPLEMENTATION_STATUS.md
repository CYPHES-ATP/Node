# ATP Implementation Status

This document maps the current CYPHES Audit Node implementation to the ATP
work-order lifecycle.

Last reviewed: June 12, 2026

## Conformance Position

The repository contains an ATP-L0-oriented vertical slice for audit discovery
and bilateral negotiation. It is not a complete ATP implementation.

The current slice proves:

- canonical signed envelopes;
- authenticated peer-to-issuer binding;
- persistent event ordering and replay defense;
- atomic receiver commit before acknowledgement;
- durable audit discovery and worker negotiation.

It does not yet prove work routing, bounded execution, settlement, attestation,
or Proof of Cognition receipt verification.

## Verb Matrix

| ATP verb | Status | Current behavior | Required next work |
| --- | --- | --- | --- |
| `DISCOVER` | Implemented | Requester signs and announces a public repository audit | Capability cards, expiry policy, internet discovery |
| `NEGOTIATE` | Partial | Worker offer and requester selection are signed and chained | Complete contract, counters, rejection, reason codes |
| `ROUTE` | Not implemented | State transition is modeled only | Work session, encrypted descriptors, leases |
| `EXECUTE` | Not implemented | State transition is modeled only | Isolated worker, progress events, artifacts |
| `SETTLE` | Not implemented | UI explicitly reports no payment rail | Zero-value adapter, approval, dispute, escrow adapter |
| `ATTEST` | Not implemented | State transition is modeled only | Receipt bundle, verifier result, final attestation |
| `REJECT` | Kernel only | Generic terminal transition exists | Product commands, schemas, reason codes |
| `REVOKE` | Kernel only | Generic terminal transition exists | Authorization, lease revocation, product commands |

## Envelope and Verification

| Requirement | Status | Implementation |
| --- | --- | --- |
| ATP version field | Implemented | `src-tauri/src/atp.rs` |
| Canonical JSON | Implemented | RFC 8785 JCS through `serde_jcs` |
| Identity signature | Implemented | Persistent libp2p Ed25519 key |
| Issuer/key binding | Implemented | Issuer is derived from the proof public key |
| Transport/issuer binding | Implemented | Inbound issuer must match authenticated peer |
| Event hash | Implemented | SHA-256 over ATP event preimage |
| Previous-event continuity | Implemented | `prev` must equal the committed transaction head |
| Nonce replay defense | Implemented | SQLite uniqueness by issuer and nonce |
| Idempotency defense | Implemented | SQLite uniqueness by issuer and idempotency key |
| Expiry validation | Implemented | RFC3339 expiry rejection when present |
| Clock-skew policy | Not implemented | Needs explicit bounds and fixtures |
| Deterministic reason codes | Partial | ACK has reason fields; registry is incomplete |
| Cross-language fixtures | Not implemented | Needed before claiming interoperability |

## Storage and Delivery

| Requirement | Status | Notes |
| --- | --- | --- |
| Durable identity | Implemented | `~/.cyphes/identity.key` |
| Durable transaction state | Implemented | `~/.cyphes/atp.sqlite3` |
| Atomic event commit | Implemented | SQLite transaction |
| Commit-before-ACK | Implemented | Receiver ACK follows successful commit |
| Peer delivery receipt | Implemented | Stored per event and peer |
| Restart persistence | Implemented | Events, replay state, and projections persist |
| Offline resend | Partial | Locally issued envelopes resend on LAN discovery |
| Internet delivery | Not implemented | LAN mDNS only |

## Product Truth

The desktop client may display only facts represented by committed backend
state:

- a request is local until a peer receipt exists;
- discovery does not imply execution;
- negotiation does not imply settlement;
- compensation is not payment;
- no receipt is shown before independent verification exists.

## Conformance Exit Criteria

Before describing one audit as faithfully fulfilled through ATP, two
independently controlled nodes must complete:

1. `DISCOVER`;
2. bilateral `NEGOTIATE`;
3. `ROUTE` with enforceable leases;
4. isolated `EXECUTE`;
5. zero-value `SETTLE`;
6. independently verified receipt bundle;
7. `ATTEST`;
8. deterministic rejection of tampering, replay, invalid `prev`, lease
   widening, and artifact substitution.

See [ATP_NETWORK_ARCHITECTURE.md](ATP_NETWORK_ARCHITECTURE.md) for the target
architecture and [../ROADMAP.md](../ROADMAP.md) for delivery milestones.
