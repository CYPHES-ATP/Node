# CYPHES v0.7.11

v0.7.11 is a network-liveness hotfix for the ATP verifier testnet.

## What changed

- Keeps the current `cyphes-dev-v0.7.7` SQLite testnet so existing stuck receipts can be repaired in place.
- Moves the wire protocol to `/cyphes/atp/0.7.11`.
- Moves the rendezvous namespace to `cyphes.repository-audit.v0.7.11`.
- Removes the periodic full-campaign sync flood.
- Batches stale receipt repair into one deduplicated labor-object bundle per repair interval.
- Adds outbound repair backpressure so nodes do not create retry storms while peers are slow.
- Allows contribution replay to repair a local submitted work-unit shell when the contribution row is missing.
- Keeps the pending self-receipt cap at `1` so any remaining liveness failure is visible quickly.

## Verification

- `cargo fmt && cargo test`
- `npm run build`
- macOS Apple Silicon DMG build
- macOS Intel DMG build

## Assets

| File | SHA-256 |
| --- | --- |
| `CYPHES_0.7.11_aarch64.dmg` | `20e052ba64edb9867cbe02bd6ec7c42b726f0a8c13179b96009cfbb597dfc0b0` |
| `CYPHES_0.7.11_x64.dmg` | `66ae4e253d589b0869d8f7f192561d4a3c60f0ed26a953b061779709d4859f71` |
