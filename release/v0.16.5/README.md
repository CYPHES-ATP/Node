# CYPHES v0.16.5 Mainnet Verifier Liveness Hotfix

v0.16.5 is a non-mandatory mainnet verifier-liveness hotfix. It keeps the
existing `cyphes-final-testnet-v0.16.0` ledger marker, ATP labor wire, receipt
format, and forward-only economics compatible with earlier v0.16.x mainnet
nodes.

This release fixes a verifier-first liveness issue:

- Adds a durable backend verifier pass that selects pending remote submitted
  receipts directly from the local store.
- Lets verifier-only nodes clear queued receipts even when the frontend has not
  hydrated every campaign snapshot.
- Keeps claim and contribution enforcement strict; workers still need signed
  claims and independent verification for ATP settlement.
- Renames the cockpit metric from Active nodes to Active links because the value
  counts current libp2p links, not the full discovered-node census.
- Preserves the prior worker claim refresh fix and aggregate dashboard
  performance path.

## Assets

- `CYPHES_0.16.5_aarch64.dmg` - Apple Silicon macOS
- `CYPHES_0.16.5_x64.dmg` - Intel macOS
- `CYPHES_0.16.5_x64-setup.exe` - Windows x64
- `SHA256SUMS.txt` - release checksums

## Validation

- `npm run build`
- `cargo check --manifest-path src-tauri/Cargo.toml`
- `cargo test --manifest-path src-tauri/Cargo.toml`
- `cargo test --manifest-path src-tauri/Cargo.toml network_verification_candidates_exclude_self_and_clear_after_verification`
- `node scripts/assert-genesis-auto-mode.mjs`
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check`
- `cargo fmt --manifest-path source-gateway/Cargo.toml --check`
- `cargo test --manifest-path source-gateway/Cargo.toml`

## Checksums

```text
48e62cb0b6d405eb68d56221698f08cbe6a48d4746adf32d31a4f7733083b3cb  CYPHES_0.16.5_aarch64.dmg
98bb6c1e4133aa36a465ed4d31efae68ffc80668031a14f5f38b601f901b80f7  CYPHES_0.16.5_x64.dmg
0250ef7e14e8d196d6c31bcf40f726f1b4e5beb8f4958f9c4f2cfd3c94826ad3  CYPHES_0.16.5_x64-setup.exe
```
