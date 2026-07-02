# CYPHES v0.7.12

v0.7.12 is a 24/7 network-liveness hotfix for the ATP verifier testnet.

## What changed

- Keeps the current `cyphes-dev-v0.7.7` SQLite testnet so existing stuck receipts can clear in place.
- Moves the wire protocol to `/cyphes/atp/0.7.12`.
- Moves the rendezvous namespace to `cyphes.repository-audit.v0.7.12`.
- Fixes the two-node split-brain case where each node submitted local work for the same work unit and then rejected the peer receipt as already submitted.
- Adds a peer-sync contribution ingest path that accepts a different worker's valid signed receipt for the same work unit.
- Keeps local worker backpressure at `1` pending self-authored receipt, while allowing peer-sync ingest to accept valid remote receipts for verifier settlement.
- Prevents duplicate submissions from the same worker for the same work unit.
- Preserves the independent-verifier rule: self-verification still earns no ATP.

## Verification

- `cargo fmt && cargo test`
- `npm run build`
- macOS Apple Silicon DMG build
- macOS Intel DMG build

## Assets

| File | SHA-256 |
| --- | --- |
| `CYPHES_0.7.12_aarch64.dmg` | `6c53c394c0d0c02f7bded7a0b3f1f1bd493b5b8f3490f001c889bdb6b23b0a6c` |
| `CYPHES_0.7.12_x64.dmg` | `13747a5e27cad4614c26f3c61206be151186e925b33a340709ec0420140f7c57` |
