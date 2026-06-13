# Contributing to CYPHES

CYPHES is an early protocol implementation, not a finished
marketplace. Contributions should strengthen one real ATP work order before
expanding the product surface.

## Start Here

1. Read [README.md](README.md).
2. Run the native client using [docs/INSTALL.md](docs/INSTALL.md).
3. Complete the two-node test in [docs/JOIN_NETWORK.md](docs/JOIN_NETWORK.md).
4. Read [docs/ATP_NETWORK_ARCHITECTURE.md](docs/ATP_NETWORK_ARCHITECTURE.md)
   before changing protocol semantics.
5. Check [ROADMAP.md](ROADMAP.md) and
   [docs/ATP_IMPLEMENTATION_STATUS.md](docs/ATP_IMPLEMENTATION_STATUS.md) for
   work that is ready to be claimed.

## Current Contribution Tracks

### ATP Kernel

- Complete verb schemas and validation.
- Add deterministic reason codes.
- Add expiry and clock-skew fixtures.
- Add cross-language canonicalization and signature fixtures.
- Preserve first-failure determinism.

Primary files:

```text
src-tauri/src/atp.rs
src-tauri/src/store.rs
```

### Network

- Deploy and operate redundant public relay/rendezvous nodes.
- Add signed capability-card advertisement and matching.
- Add AutoNAT and verify direct DCUtR upgrades.
- Persist peer addresses and retry audience-specific delivery.
- Preserve commit-before-ACK behavior.

Primary file:

```text
src-tauri/src/p2p.rs
```

### Audit Execution and Verification

- Extend the signed audit contract without breaking its versioned schemas or fixtures.
- Harden the bounded worker inside an OS-enforced sandbox.
- Add richer analyzers without executing untrusted repository code.
- Add cancellation, revocation, and partial-failure receipts.
- Extend Artifact Two with negative and alternate-terminal fixtures.

### Desktop Client

- Keep the interface limited to backend-confirmed facts.
- Preserve native window behavior and CYPHES design language.
- Improve accessibility and keyboard navigation.
- Add Linux and Windows verification.

### Documentation and Testing

- Reproduce the two-node flow on different operating systems.
- Improve troubleshooting with verified commands and logs.
- Add tamper, replay, disconnect, and restart tests.

## Development Setup

```bash
git clone https://github.com/CYPHES-ATP/Node.git
cd Node
npm install
npm run tauri dev
```

Node.js 20.19+ or 22.12+ and npm 10+ are required.

Run all checks before opening a pull request:

```bash
npm run build
(cd src-tauri && cargo fmt --check)
(cd src-tauri && cargo check)
(cd src-tauri && cargo test)
(cd relay && cargo fmt --check && cargo test)
```

## Pull Requests

- Keep each pull request focused on one protocol or product boundary.
- Explain which state transition or user-visible fact changes.
- Include tests for protocol, storage, or networking behavior.
- Update documentation when commands, paths, schemas, or limits change.
- Include a screenshot for visible desktop changes.
- State what remains unimplemented.

Do not include:

- seeded or simulated peers;
- fake jobs, reputation, receipts, or settlement;
- secrets, identity keys, database files, or private repositories;
- broad refactors unrelated to the stated change.

## Protocol Rules

- ATP objects must be canonicalized before signing.
- The envelope issuer must match the signing identity.
- Inbound state changes must be persisted before ACK.
- Replay state must survive restart.
- Invalid state transitions must fail deterministically.
- The frontend must never become the authority for transaction state.

## Security

Never commit:

```text
~/.cyphes/identity.key
~/.cyphes/atp.sqlite3
~/.cyphes/receipts/
```

Report security issues using [SECURITY.md](SECURITY.md), not a public issue.

Participation is governed by [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
