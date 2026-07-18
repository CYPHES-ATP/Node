# CYPHES v0.17.0 Mainnet Settlement Rescue

v0.17.0 is a non-mandatory mainnet liveness release. It preserves the existing
`cyphes-final-testnet-v0.16.0` genesis ledger marker, ATP wire compatibility,
receipt format, and forward-only economics. No database reset is required.

## Fixes

- Adds direct settlement rescue for straggler receipts. A node with stale
  submitted receipts advertises the exact contribution IDs to connected peers.
- Lets independent peers immediately verify those exact receipts when they have
  the signed contribution and no existing verification.
- Returns known verification IDs when a peer already has finality for the
  receipt, so the requester can pull the missing verification bundle by ID.
- Reports superseding work-unit finality when another contribution has already
  been verified for the same work unit.
- Pushes missing contribution bundles back to peers that can see the receipt ID
  but lack the signed object.
- Advertises a `settlement_rescue_v1` capability so older compatible nodes can
  stay on the network without being treated as recovery peers.
- Allows live connected peers to carry recovery traffic even when a stale
  dial-failure cooldown would block a fresh dial attempt.

This release does not self-verify receipts, rewrite ATP history, raise reward
caps, or fork the ledger. It is intended to make ordinary join/rejoin behavior
recover stuck receipts without requiring a manual reboot ritual.

## Validation

- `npm run build`
- `node scripts/assert-genesis-auto-mode.mjs`
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check`
- `cargo check --manifest-path src-tauri/Cargo.toml`
- `cargo test --manifest-path src-tauri/Cargo.toml`
- `cargo fmt --manifest-path source-gateway/Cargo.toml --check`
- `cargo test --manifest-path source-gateway/Cargo.toml`
- `cargo fmt --manifest-path relay/Cargo.toml --check`
- `cargo test --manifest-path relay/Cargo.toml`

Regression coverage includes exact-ID settlement rescue, superseding
verification discovery, durable submitted-receipt candidate selection, and
settled-work-unit exclusion from stale repair windows.

## Assets

- `CYPHES_0.17.0_aarch64.dmg`
- `CYPHES_0.17.0_x64.dmg`
- `CYPHES_0.17.0_x64-setup.exe`
- `SHA256SUMS.txt`

## Checksums

```text
15159288e758c09b2c148f551890dee389b7bc1308aded5e7f690f8d8e03dc1a  CYPHES_0.17.0_aarch64.dmg
b9f8175bc614269df6871da91895042e2ae3d2b913de4a849e0b9fb3b35b0ae3  CYPHES_0.17.0_x64.dmg
8ff85432d92f6abb82162bb134055fb4a9aaa4f2b95b37f1910bd425a33a305a  CYPHES_0.17.0_x64-setup.exe
```
