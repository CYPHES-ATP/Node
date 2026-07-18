# CYPHES v0.16.7 Mainnet Peer Discovery Hotfix

v0.16.7 is a non-mandatory mainnet peer-discovery pressure hotfix. It keeps the
existing `cyphes-final-testnet-v0.16.0` ledger marker, ATP labor wire, receipt
format, and forward-only economics compatible with earlier v0.16.x mainnet
nodes.

This release fixes a network-dark edge case observed after v0.16.6 made active
peer links stricter:

- Makes rendezvous discovery dials respect the existing peer failure cooldown.
- Stops stale rendezvous entries from being re-dialed every discovery tick.
- Caps each rendezvous discovery pass to a small number of peer dial attempts,
  smoothing reconnect pressure instead of spiking the public relay.
- Reduces relay circuit pressure when old/offline peer identities remain in
  discovery results.
- Raises the CYPHES relay service budget above libp2p demo defaults for
  reservations and circuits.
- Preserves verifier integrity: no self-verification, no forced receipt
  clearing, no database reset, and no ATP rewrite.

## Assets

- `CYPHES_0.16.7_aarch64.dmg` - Apple Silicon macOS
- `CYPHES_0.16.7_x64.dmg` - Intel macOS
- `CYPHES_0.16.7_x64-setup.exe` - Windows x64
- `SHA256SUMS.txt` - release checksums

## Validation

- `npm run build`
- `cargo check --manifest-path src-tauri/Cargo.toml`
- `cargo test --manifest-path src-tauri/Cargo.toml`
- `node scripts/assert-genesis-auto-mode.mjs`
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check`
- `cargo fmt --manifest-path relay/Cargo.toml --check`
- `cargo check --manifest-path relay/Cargo.toml`
- `cargo fmt --manifest-path source-gateway/Cargo.toml --check`
- `cargo test --manifest-path source-gateway/Cargo.toml`

## Checksums

```text
ee7a0a1b779f2f9c9de3c30a846830578690b1fa270a6b99ccae06145f3ef08b  CYPHES_0.16.7_aarch64.dmg
831633cc0f9130f806a14beca21a57fe17d172bf4685a077b1b61d93962d565e  CYPHES_0.16.7_x64.dmg
```
