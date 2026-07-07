# CYPHES v0.15.3

Active Proof of Cognition testnet release.

v0.15.3 keeps the current `cyphes-dev-v0.7.7` testnet and the
`/cyphes/atp/0.15.1` labor wire for compatibility with the live network.

## Highlights

- Opens new Cognition Proof epochs after the Guardian target pass completes.
- Persists explicit Run mode across restart until Stop is pressed.
- Raises autonomous campaign seeding to 9600 campaigns per day.
- Reduces redundant labor sync by answering inventory with missing-object IDs
  before sending full bundles.
- Requires the v0.15.3 sparse-inventory capability before expensive labor-bundle
  ingest from peers.
- Prefers reachable public or relayed peer routes over stale private routes.
- Requires evidence-backed structured proof output, with one automatic JSON
  repair pass before parser-fallback ATP deductions apply.

## Assets

- `CYPHES_0.15.3_aarch64.dmg` - macOS Apple Silicon
- `CYPHES_0.15.3_x64.dmg` - macOS Intel
- `CYPHES_0.15.3_x64-setup.exe` - Windows x64

## SHA-256

```text
e133d1e07fd5846bb5e3fa3edf79e95a6c35714965560c991fe7478ab2002c3c  CYPHES_0.15.3_aarch64.dmg
c351c5ad0e038348181490852ff33a525457b90d2a559a86bc6f4cb0d0c63deb  CYPHES_0.15.3_x64.dmg
caf1fe9e80123e6be59784e5d292fa28e0c95c76e7ac9fc164ccea69b4a13ed0  CYPHES_0.15.3_x64-setup.exe
```
