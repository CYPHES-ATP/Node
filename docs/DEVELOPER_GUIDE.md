# Developer Guide

## Product Boundary

The current product has one job: allow CYPHES nodes to coordinate bounded,
public repository security work through signed ATP events, signed work-unit
contributions, verifier receipts, and portable report/receipt bundles.

Do not add simulated peers, sample jobs, reputation counters, synthetic
responses, global-network labels, or payment claims.

## Code Ownership Map

| Path | Owns |
| --- | --- |
| `src/App.tsx` | Work-order cockpit, Genesis Auto Mode, runtime telemetry, and truthful state labels |
| `src/hooks/useP2P.ts` | Typed calls into the native command boundary |
| `src/store/useCyphesStore.ts` | Ephemeral frontend view state only |
| `src/components/providers/P2PProvider.tsx` | Native events and backend refresh |
| `src/styles/globals.css` | Desktop visual system |
| `src-tauri/src/atp.rs` | ATP data model, proofs, canonicalization, event hashes, transition rules |
| `src-tauri/src/audit_profile.rs` | Repository-audit contract, receipt types, canonical hashes, validation |
| `src-tauri/src/store.rs` | SQLite schema, replay protection, atomic commits, job projections, ACK receipts |
| `src-tauri/src/worker.rs` | Context leases, guarded source access, deterministic audit artifacts |
| `src-tauri/src/bundle.rs` | Artifact Two-compatible receipt export |
| `src-tauri/src/p2p.rs` | mDNS, relay/rendezvous discovery, result delivery, peer synchronization |
| `src-tauri/src/commands.rs` | Product operations exposed to Tauri |
| `src-tauri/src/state.rs` | In-process peer and node runtime state |
| `src-tauri/src/lib.rs` | Native application composition |
| `relay/` | Combined Relay v2/Rendezvous service and network smoke clients |
| `network/` | Default network publication manifest |
| `protocol/targets/` | Genesis guardian target index for public DeFi coverage campaigns |

The Rust backend is authoritative. React may request an operation and render
the returned projection, but it must not manufacture transaction state.

## Frontend

The UI is intentionally split:

- `src/App.tsx` owns the CYPHES cockpit: runtime selection, Genesis Auto Mode,
  Work Orders, per-unit claim/run controls, verification, and report export.
- `src/campaign.tsx` owns `campaign.html`: protocol/admin campaign creation,
  network state, guardian target index visibility, ATP proof logs, and
  developer receipt inspection.
- `src/store/useCyphesStore.ts` holds the backend-confirmed view model only.
- `src/hooks/useP2P.ts` wraps the Tauri command boundary.
- `src/components/providers/P2PProvider.tsx` handles live network events.
- `src/styles/globals.css` carries the CYPHES AMOLED design system.

Campaign creation performs a live lookup against GitHub's public repository API
and resolves the default branch, file URL, or folder URL to an exact commit
before the backend signs and commits the request. The browser preview is
read-only. The legacy localStorage key is used only as one-time migration input.

## Native Backend

The Rust backend:

- loads or creates `~/.cyphes/identity.key`;
- stores ATP state in `~/.cyphes/atp.sqlite3`;
- protects both files with owner-only permissions on Unix;
- canonicalizes ATP payloads with RFC 8785 JCS;
- signs and verifies envelopes with the libp2p Ed25519 identity;
- enforces event hashes, `prev`, nonces, idempotency, and transaction order;
- discovers LAN peers through mDNS and internet peers through signed
  rendezvous registrations;
- connects peers directly or through Circuit Relay v2;
- exposes QUIC, TCP, WebSocket, Identify, Ping, and DCUtR behavior;
- negotiates `/cyphes/atp/0.3` request/response streams;
- commits inbound envelopes before returning an ACK;
- binds each ATP issuer to the authenticated libp2p source;
- synchronizes locally issued envelopes when a peer is discovered;
- verifies requester-signed context leases before work;
- downloads only the pinned GitHub archive and executes no repository code;
- verifies signed results before requester settlement;
- emits and exports the terminal Proof of Cognition.
- stores protocol audit campaigns, work units, claims, contributions,
  verifier decisions, ATP Credit allocations, and final report bundles;
- exposes the Genesis guardian target index;
- enforces Auto Worker runtime limits when the v0.5.4 auto loop runs claimed
  work units.

Current audit transaction uses:

1. requester `DISCOVER`;
2. worker `NEGOTIATE` offer carrying the typed audit contract;
3. requester `NEGOTIATE` selection accepting its canonical `contractHash`.
4. requester `ROUTE` containing repository-read and artifact-write leases;
5. worker bounded activity and signed result;
6. requester zero-value `SETTLE`;
7. worker `ATTEST`.

The public cross-implementation boundary lives in `protocol/schemas/` and
`protocol/fixtures/`.

## Local Data

Default paths:

```text
~/.cyphes/identity.key
~/.cyphes/atp.sqlite3
~/.cyphes/receipts/
```

Set `CYPHES_DATA_DIR` to run an isolated development identity. The database is
configured for WAL mode and foreign-key enforcement. On Unix the identity and
database are restricted to the current user.

## Application Icon

The original CYPHES helmet artwork is retained at:

```text
src-tauri/icons/source/cyphes.png
```

Because platform application icons require a square source, the artwork is
centered without distortion on a square white canvas:

```text
src-tauri/icons/source/cyphes-square.png
```

Regenerate the desktop icon set with:

```bash
npm run tauri icon src-tauri/icons/source/cyphes-square.png -- -o src-tauri/icons
```

The Tauri bundle consumes `icon.icns`, `icon.ico`, and the configured PNG
sizes from `src-tauri/icons/`.

## Inbound Commit Sequence

An inbound request is processed in this order:

1. decode the ATP envelope;
2. verify version and proof;
3. bind issuer to transport peer;
4. calculate the event hash;
5. reject nonce or idempotency replay;
6. verify `prev` against the committed transaction head;
7. enforce the ATP state transition and audit body invariants;
8. commit event, replay records, and job projection in one transaction;
9. return an ACK.

Never move ACK generation before the database commit.

## Explicitly Unavailable

- Offline mailbox and durable global work-order index.
- Real escrow or payment settlement.
- Hardened container or VM isolation for the worker.
- Private GitHub repository authorization.
- Lease revocation, cancellation, dispute, key rotation, and recovery.
- Signed release binaries.

These should appear as unavailable in the product until implemented and tested.

## Verification

```bash
npm run build
(cd src-tauri && cargo fmt --check)
(cd src-tauri && cargo check)
(cd src-tauri && cargo test)
(cd relay && cargo fmt --check && cargo test)
./scripts/verify-atp-l1.sh
```
