# CYPHES v0.16.6 Mainnet Infrastructure Liveness Hotfix

v0.16.6 is a non-mandatory mainnet infrastructure-liveness hotfix. It keeps the
existing `cyphes-final-testnet-v0.16.0` ledger marker, ATP labor wire, receipt
format, and forward-only economics compatible with earlier v0.16.x mainnet
nodes.

This release fixes silent network-dark nodes:

- Adds a libp2p relay/rendezvous watchdog that treats infrastructure links as
  stale after 90 seconds without observable activity.
- Disconnects and redials stale relay/rendezvous links instead of trusting a
  zombie connection until the one-hour idle timeout.
- Persists route failures into `audit_labor_events` as
  `infrastructure_dial_failed`, `infrastructure_connection_recycled`, and
  `peer_dial_failed`.
- Handles libp2p outgoing connection errors explicitly, making failed redials
  visible in telemetry and UI notices.
- Counts actual active peer links in the cockpit instead of local/self state or
  remembered peers.
- Preserves the prior durable verifier queue path, no self-verification, no
  forced receipt clearing, and no database reset.

## Assets

- `CYPHES_0.16.6_aarch64.dmg` - Apple Silicon macOS
- `CYPHES_0.16.6_x64.dmg` - Intel macOS
- `CYPHES_0.16.6_x64-setup.exe` - Windows x64
- `SHA256SUMS.txt` - release checksums

## Validation

- `npm run build`
- `cargo check --manifest-path src-tauri/Cargo.toml`
- `cargo test --manifest-path src-tauri/Cargo.toml`
- `node scripts/assert-genesis-auto-mode.mjs`
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check`
- `cargo fmt --manifest-path source-gateway/Cargo.toml --check`
- `cargo test --manifest-path source-gateway/Cargo.toml`

## Checksums

```text
36fc761a9ecabaf6c5eb7c911b0656ccc07d9d512708d7bf17963935632918db  CYPHES_0.16.6_aarch64.dmg
348b8b6da5ce8ebf396c4fdf9313fd43692d484503fefc7d5cea2544334233a2  CYPHES_0.16.6_x64.dmg
2d130f0c8237181281f88a18b3fd5081d5bceddb848d2a780c53f6739758c5d8  CYPHES_0.16.6_x64-setup.exe
```
