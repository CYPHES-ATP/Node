# CYPHES v0.6.5 Claim Sync And Backpressure Hotfix

Release date: 2026-06-30

## Download

- `CYPHES_0.6.5_aarch64.dmg`
- `CYPHES_0.6.5_x64.dmg`

These macOS builds are ad hoc signed and verified locally, but not
Apple-notarized yet.

## What Changed

- Keeps the one-node network model: every CYPHES node can request, work, and
  verify depending on local state and network opportunity.
- Rebroadcasts signed work-unit claims during periodic P2P labor sync and peer
  reconnect healing, before contribution rebroadcasts.
- Preserves BB-01 claim-bound contribution ingest while making missed claim
  prerequisites self-heal instead of stranding later contributions.
- Adds worker backpressure: nodes pause new claimed work after 4 self-authored
  receipts are awaiting independent verification.
- Updates autonomous cockpit messaging so the pause reads as verifier
  backpressure instead of a stalled worker.
- Retains the v0.6.3 audit ingest hardening and v0.6.4 independent verifier
  liveness behavior.

## Verification

```text
npm_config_cache=/tmp/cyphes-npm-cache npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo test --manifest-path src-tauri/Cargo.toml
cargo fmt --manifest-path source-gateway/Cargo.toml --check
cargo test --manifest-path source-gateway/Cargo.toml
codesign --verify --deep --strict --verbose=2 src-tauri/target/aarch64-apple-darwin/release/bundle/macos/CYPHES.app
codesign --verify --deep --strict --verbose=2 src-tauri/target/x86_64-apple-darwin/release/bundle/macos/CYPHES.app
hdiutil verify src-tauri/target/aarch64-apple-darwin/release/bundle/dmg/CYPHES_0.6.5_aarch64.dmg
hdiutil verify src-tauri/target/x86_64-apple-darwin/release/bundle/dmg/CYPHES_0.6.5_x64.dmg
lipo -archs src-tauri/target/aarch64-apple-darwin/release/bundle/macos/CYPHES.app/Contents/MacOS/cyphes-desktop
lipo -archs src-tauri/target/x86_64-apple-darwin/release/bundle/macos/CYPHES.app/Contents/MacOS/cyphes-desktop
```

Rust desktop tests: 46 passed, 1 intentionally ignored live-GitHub fixture test.
Source Gateway tests: 2 passed.
Architectures verified: `arm64` and `x86_64`.

## SHA-256

```text
7e1dd4dd16eecca9204f1feb741b606273cfcf09391a001a01bf6d9a7574ec92  CYPHES_0.6.5_aarch64.dmg
2111ac9aac2841eb6b49dcbb9e27ce745fa641e0ceef92b5700e2b7008e7e4ee  CYPHES_0.6.5_x64.dmg
```
