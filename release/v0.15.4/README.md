# CYPHES v0.15.4

Active Proof of Cognition testnet performance hotfix.

v0.15.4 keeps the current `cyphes-dev-v0.7.7` testnet and the
`/cyphes/atp/0.15.1` labor wire for compatibility with the live network.

## Highlights

- Adds cheap duplicate/superseded labor-object preflight before expensive
  signature and canonical-hash verification.
- Skips known contribution IDs, known receipt hashes, repeated
  worker/work-unit receipts, terminal work units, known verification IDs, and
  already-verified contribution targets.
- Emits `labor_object_bundle_duplicate_skipped` telemetry with duplicate and
  superseded counts.
- Does not mutate credits, work status, or verification state on the skip path.
- Quiets the cockpit progress animation when the node is fully settled so idle
  verifier nodes do not keep repainting the desktop UI.
- Uses a lightweight live campaign snapshot for the cockpit so settled
  dashboards do not recompute trusted credits across every historical receipt.
- Keeps v0.15.3 target-completion epochs, sparse inventory, route preference,
  durable Run mode, and structured Cognition Proof enforcement.

## Assets

- `CYPHES_0.15.4_aarch64.dmg` - macOS Apple Silicon
- `CYPHES_0.15.4_x64.dmg` - macOS Intel
- `CYPHES_0.15.4_x64-setup.exe` - Windows x64

## SHA-256

```text
fb8f39be7d62a09836a8938223bd53ad522a654b5f014ad34f2802e09343c462  CYPHES_0.15.4_aarch64.dmg
ee2e42e8cc96b9a1675daba9813d6454e144e80bc6800726a02494a4b3ca8961  CYPHES_0.15.4_x64.dmg
7357c705ad71b86afc41707e44820510603d97c2121f1d0dec47bc7c8d8ef180  CYPHES_0.15.4_x64-setup.exe
```
