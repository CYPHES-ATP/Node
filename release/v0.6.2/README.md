# CYPHES v0.6.2 Testnet Seed

Release date: 2026-06-29

## Download

- `CYPHES_0.6.2_aarch64.dmg`

This Apple Silicon macOS build is ad hoc signed and verified locally, but not
Apple-notarized yet.

## What Changed

- Raises the autonomous Guardian observation cap to 2880/day.
- Raises the autonomous local-model audit cap to 2880/day.
- Automatically upgrades older hidden local settings that were still capped at
  24/day.
- Adds deterministic ATP scoring quality control for parser-fallback output:
  unstructured model notes with zero structured findings now earn only 10% of
  the normal worker/verifier ATP allocation.
- Shows parser-fallback ATP deductions as red cockpit telemetry while keeping
  the signed fallback artifact available for review.
- Records parser-fallback metadata in `runtime.json`: parser fallback state,
  structured finding count, reportable finding count, and credit quality
  multiplier.

## Verification

```text
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo test --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path source-gateway/Cargo.toml
codesign --verify --deep --strict --verbose=2 CYPHES.app
hdiutil verify CYPHES_0.6.2_aarch64.dmg
```

Rust desktop tests: 42 passed, 1 intentionally ignored live-GitHub fixture test.
Source Gateway tests: 2 passed.

## SHA-256

```text
1d38398e9585b5badcbd19bfdccd657481a31134db9932d9ae67fc234a82cce5  CYPHES_0.6.2_aarch64.dmg
```
