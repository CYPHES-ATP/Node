# Developer Guide

## Product Boundary

The current product has one job: allow two CYPHES nodes on the same LAN to
negotiate a public GitHub repository audit through signed, durable ATP events.

Do not add simulated peers, sample jobs, reputation counters, synthetic
responses, global-network labels, or payment claims.

## Code Ownership Map

| Path | Owns |
| --- | --- |
| `src/App.tsx` | User workflow and truthful state labels |
| `src/hooks/useP2P.ts` | Typed calls into the native command boundary |
| `src/store/useCyphesStore.ts` | Ephemeral frontend view state only |
| `src/components/providers/P2PProvider.tsx` | Native events and backend refresh |
| `src/styles/globals.css` | Desktop visual system |
| `src-tauri/src/atp.rs` | ATP data model, proofs, canonicalization, event hashes, transition rules |
| `src-tauri/src/store.rs` | SQLite schema, replay protection, atomic commits, job projections, ACK receipts |
| `src-tauri/src/p2p.rs` | Identity file, swarm, mDNS, request/response, peer synchronization |
| `src-tauri/src/commands.rs` | Product operations exposed to Tauri |
| `src-tauri/src/state.rs` | In-process peer and node runtime state |
| `src-tauri/src/lib.rs` | Native application composition |

The Rust backend is authoritative. React may request an operation and render
the returned projection, but it must not manufacture transaction state.

## Frontend

The UI is intentionally small:

- `src/App.tsx` owns the repository form and request list.
- `src/store/useCyphesStore.ts` holds the backend-confirmed view model only.
- `src/hooks/useP2P.ts` wraps the Tauri command boundary.
- `src/components/providers/P2PProvider.tsx` handles live network events.
- `src/styles/globals.css` carries the CYPHES AMOLED design system.

Repository creation performs a live lookup against GitHub's public repository
API before the backend signs and commits a request. The browser preview is
read-only. The legacy localStorage key is used only as one-time migration input.

## Native Backend

The Rust backend:

- loads or creates `~/.cyphes/identity.key`;
- stores ATP state in `~/.cyphes/atp.sqlite3`;
- protects both files with owner-only permissions on Unix;
- canonicalizes ATP payloads with RFC 8785 JCS;
- signs and verifies envelopes with the libp2p Ed25519 identity;
- enforces event hashes, `prev`, nonces, idempotency, and transaction order;
- discovers LAN peers through mDNS;
- negotiates `/cyphes/atp/0.3` request/response streams;
- commits inbound envelopes before returning an ACK;
- binds each ATP issuer to the authenticated libp2p source;
- synchronizes locally issued envelopes when a peer is discovered.

Current audit negotiation uses:

1. requester `DISCOVER`;
2. worker `NEGOTIATE` offer;
3. requester `NEGOTIATE` worker selection.

## Local Data

Default paths:

```text
~/.cyphes/identity.key
~/.cyphes/atp.sqlite3
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

- Internet relay or bootstrap.
- Escrow or payment settlement.
- Repository cloning and audit execution.
- ATP routing, execution, settlement, attestation, leases, and receipt bundles.
- Private GitHub repository authorization.

These should appear as unavailable in the product until implemented and tested.

## Verification

```bash
npm run build
(cd src-tauri && cargo fmt --check)
(cd src-tauri && cargo check)
(cd src-tauri && cargo test)
```
