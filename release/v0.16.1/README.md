# CYPHES v0.16.1 Final Testnet

v0.16.1 is an in-place Final Testnet polish release over the existing network
marker:

```text
cyphes-final-testnet-v0.16.0
```

It does not force a fresh ledger. Existing v0.16.0 Final Testnet nodes can
upgrade in place; startup reconciliation reclassifies stale pending receipts
that lost finality races as superseded instead of leaving them as awaiting an
independent verifier.

## Assets

- `CYPHES_0.16.1_aarch64.dmg` - Apple Silicon macOS
- `CYPHES_0.16.1_x64.dmg` - Intel macOS
- `CYPHES_0.16.1_x64-setup.exe` - Windows x64

macOS builds are ad hoc signed and not Apple-notarized. The Windows setup
build is unsigned and intended for testnet use. Verify every download against
`SHA256SUMS.txt` before installation.

## What Changed

- Adds a superseded receipt lifecycle for unverified submitted receipts whose
  work unit already finalized through a different accepted contribution.
- Excludes superseded or settled-work receipts from worker backpressure and
  verifier-pending counts.
- Adds a bounty-candidate gate: a finding must include concrete location,
  exploit path, impact, and reproduction evidence before it can remain
  reportable.
- Splits ATP quality rewards so parser fallback remains 0.10x, low-evidence
  structured coverage earns 0.20x, normal coverage earns the standard tier, and
  bounty-grade reportable evidence receives the higher finding tier.
- Removes the old settlement/work-cleared progress row from the main cockpit.
- Adds epoch completion percentage beside the Guardian target count.

## Operator Notes

- Nodes join as verifier-only by default.
- Press **Contribute** only when a local LM Studio or Ollama model should do
  audit work.
- Press **Stop worker** to return to verifier-only operation.
- Verified ATP remains receipt-derived: signed contribution, independent signed
  verifier acceptance, and deterministic allocation.
- This release keeps the v0.16.0 Final Testnet database marker by design so old
  stale receipts can be reconciled instead of hidden by a fresh database.

## Verification

- `npm run build`
- `node scripts/assert-genesis-auto-mode.mjs`
- `cargo check --manifest-path src-tauri/Cargo.toml`
- `cargo test --manifest-path src-tauri/Cargo.toml`
- macOS Apple Silicon Tauri worker build
- macOS Intel Tauri worker build
- Windows x64 Tauri worker build through local `cargo-xwin`
- `codesign --verify --deep --strict --verbose=2` for both macOS apps
- `hdiutil verify` for both macOS DMGs
- `lipo -archs` verified `arm64` and `x86_64`
- `file` verified Windows PE x86-64 app and NSIS installer
- `shasum -a 256 -c SHA256SUMS.txt`

## SHA-256

```text
5133eab9440a87418ea1a79a2dadb5884809005a90b1f521a2fcf2c44bb792c5  CYPHES_0.16.1_aarch64.dmg
396d6888fd3fbb5fabd3063434c138531821711bdeadc8be5ef74ae15c7f2d74  CYPHES_0.16.1_x64.dmg
5c52504a71db9d906b7263faa366051483ad3bc52c060da625ded2949bcd46ea  CYPHES_0.16.1_x64-setup.exe
```
