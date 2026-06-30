# CYPHES v0.6.3 Audit Hotfix

Release date: 2026-06-29

## Download

- `CYPHES_0.6.3_aarch64.dmg`
- `CYPHES_0.6.3_x64.dmg`

These macOS builds are ad hoc signed and verified locally, but not
Apple-notarized yet.

## What Changed

- Fixes BB-01: non-requester worker contributions now require an active signed
  work-unit claim before store-level ingest accepts them.
- Rejects submitted or terminal work units unless the inbound contribution is
  the same idempotent signed contribution replay.
- Preserves requester-owned local campaign execution while keeping remote
  worker ingest claim-bound.
- Fixes BB-04: verification bundle ingest no longer uses `INSERT OR IGNORE` for
  verifications or credit allocations.
- Rejects reused `verification_id` values unless the stored verification row is
  byte-identical and the allocation terms match.
- Adds one-verification-per-contribution enforcement, foreign-key constraints
  for new stores, and row-count assertions before status/credit mutation.
- Filters campaign snapshot credits through the same trust recomputation path
  used by `credit_summary`.

## Verification

```text
npm_config_cache=/tmp/cyphes-npm-cache npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo test --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path source-gateway/Cargo.toml
codesign --verify --deep --strict --verbose=2 src-tauri/target/aarch64-apple-darwin/release/bundle/macos/CYPHES.app
codesign --verify --deep --strict --verbose=2 src-tauri/target/x86_64-apple-darwin/release/bundle/macos/CYPHES.app
hdiutil verify src-tauri/target/aarch64-apple-darwin/release/bundle/dmg/CYPHES_0.6.3_aarch64.dmg
hdiutil verify src-tauri/target/x86_64-apple-darwin/release/bundle/dmg/CYPHES_0.6.3_x64.dmg
lipo -archs src-tauri/target/aarch64-apple-darwin/release/bundle/macos/CYPHES.app/Contents/MacOS/cyphes-desktop
lipo -archs src-tauri/target/x86_64-apple-darwin/release/bundle/macos/CYPHES.app/Contents/MacOS/cyphes-desktop
```

Rust desktop tests: 47 passed, 1 intentionally ignored live-GitHub fixture test.
Source Gateway tests: 2 passed.
Architectures verified: `arm64` and `x86_64`.

## SHA-256

```text
bbff144b7564ac33b6d0ff4ec814f0473c7b2cbe352d07313e8b8ef2ee33acf2  CYPHES_0.6.3_aarch64.dmg
ea05c036104efee1b5f2adb9e3c4e882c5bf10a0a2abbadfe03065d008f8960e  CYPHES_0.6.3_x64.dmg
```
