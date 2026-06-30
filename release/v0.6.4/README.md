# CYPHES v0.6.4 Network Liveness Hotfix

Release date: 2026-06-30

## Download

- `CYPHES_0.6.4_aarch64.dmg`
- `CYPHES_0.6.4_x64.dmg`

These macOS builds are ad hoc signed and verified locally, but not
Apple-notarized yet.

## What Changed

- Fixes the v0.6.3 verifier-duty liveness regression where a node's own pending
  signed contribution counted as local verifier work and blocked new claims or
  worker execution.
- Keeps Verified ATP independent-verifier enforcement intact: self-authored
  pending receipts still wait for a different node, but no longer stall the
  local worker loop.
- Removes deterministic verifier assignment from the P2P auto-verifier loop, so
  any independent online verifier can settle an eligible remote receipt instead
  of stranding it behind a stale assigned peer.
- Updates frontend telemetry to distinguish independently verifiable pending
  receipts from self-authored pending receipts.
- Retains the v0.6.3 BB-01 and BB-04 audit ingest hardening.

## Verification

```text
npm_config_cache=/tmp/cyphes-npm-cache npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo test --manifest-path src-tauri/Cargo.toml
cargo fmt --manifest-path source-gateway/Cargo.toml --check
cargo test --manifest-path source-gateway/Cargo.toml
codesign --verify --deep --strict --verbose=2 src-tauri/target/aarch64-apple-darwin/release/bundle/macos/CYPHES.app
codesign --verify --deep --strict --verbose=2 src-tauri/target/x86_64-apple-darwin/release/bundle/macos/CYPHES.app
hdiutil verify src-tauri/target/aarch64-apple-darwin/release/bundle/dmg/CYPHES_0.6.4_aarch64.dmg
hdiutil verify src-tauri/target/x86_64-apple-darwin/release/bundle/dmg/CYPHES_0.6.4_x64.dmg
lipo -archs src-tauri/target/aarch64-apple-darwin/release/bundle/macos/CYPHES.app/Contents/MacOS/cyphes-desktop
lipo -archs src-tauri/target/x86_64-apple-darwin/release/bundle/macos/CYPHES.app/Contents/MacOS/cyphes-desktop
```

Rust desktop tests: 46 passed, 1 intentionally ignored live-GitHub fixture test.
Source Gateway tests: 2 passed.
Architectures verified: `arm64` and `x86_64`.

## SHA-256

```text
76ec288ce4d94c936afe30d28868140fa684ba34d392b1b9dfa8431fa979b74d  CYPHES_0.6.4_aarch64.dmg
d0c6777b8f3b261a7d247436ae06e8ac44be5c8ef95cff5d41d511818df0e09d  CYPHES_0.6.4_x64.dmg
```
