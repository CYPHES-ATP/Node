# CYPHES v0.7.1 Fresh Testnet And SQLite Indexes

Release date: 2026-06-30

## Download

- `CYPHES_0.7.1_aarch64.dmg`
- `CYPHES_0.7.1_x64.dmg`
- `CYPHES_0.7.1_x64-setup.exe`

The macOS builds are ad hoc signed and verified locally, but not
Apple-notarized yet. The Windows x64 setup build is an unsigned NSIS installer
cross-built from macOS for testnet use.

## What Changed

- Starts a fresh isolated testnet using ATP wire protocol `/cyphes/atp/0.7.1`.
- Moves public rendezvous discovery to `cyphes.repository-audit.v0.7.1`, so old
  v0.6.x nodes do not join the new testnet through normal discovery.
- Keeps the one-node model: every CYPHES node can request, work, and verify
  depending on local state and network opportunity.
- Adds SQLite indexes for the hot pending queue, claim sync, verifier duty,
  campaign snapshot, credit summary, and delivery lookup paths.
- Retains the v0.6.3 audit ingest hardening, v0.6.4 independent verifier
  liveness fix, and v0.6.5 claim resync/backpressure behavior.

## Recommended Upgrade Procedure

Back up the old network data, keep `identity.key`, then rotate the ATP ledger:

```bash
osascript -e 'tell application "CYPHES" to quit' || true
mkdir -p ~/.cyphes/backups/2026-06-30-v0.6-network-retired
cp -p ~/.cyphes/atp.sqlite3 ~/.cyphes/backups/2026-06-30-v0.6-network-retired/atp.sqlite3
cp -p ~/.cyphes/identity.key ~/.cyphes/backups/2026-06-30-v0.6-network-retired/identity.key
mv ~/.cyphes/atp.sqlite3 ~/.cyphes/atp.v0.6-retired.sqlite3
open /Applications/CYPHES.app
```

## Verification

```text
npm_config_cache=/tmp/cyphes-npm-cache npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo test --manifest-path src-tauri/Cargo.toml
cargo fmt --manifest-path source-gateway/Cargo.toml --check
cargo test --manifest-path source-gateway/Cargo.toml
codesign --verify --deep --strict --verbose=2 src-tauri/target/aarch64-apple-darwin/release/bundle/macos/CYPHES.app
codesign --verify --deep --strict --verbose=2 src-tauri/target/x86_64-apple-darwin/release/bundle/macos/CYPHES.app
hdiutil verify src-tauri/target/aarch64-apple-darwin/release/bundle/dmg/CYPHES_0.7.1_aarch64.dmg
hdiutil verify src-tauri/target/x86_64-apple-darwin/release/bundle/dmg/CYPHES_0.7.1_x64.dmg
lipo -archs src-tauri/target/aarch64-apple-darwin/release/bundle/macos/CYPHES.app/Contents/MacOS/cyphes-desktop
lipo -archs src-tauri/target/x86_64-apple-darwin/release/bundle/macos/CYPHES.app/Contents/MacOS/cyphes-desktop
PATH="$HOME/.local/bin:$HOME/.local/llvm-22.1.8/LLVM-22.1.8-macOS-ARM64/bin:$PATH" \
  npm_config_cache=/tmp/cyphes-npm-cache \
  npm exec tauri -- build --config src-tauri/tauri.worker.conf.json \
  --runner cargo-xwin --target x86_64-pc-windows-msvc --ci
file src-tauri/target/x86_64-pc-windows-msvc/release/cyphes-desktop.exe
file src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/CYPHES_0.7.1_x64-setup.exe
```

Rust desktop tests: 46 passed, 1 intentionally ignored live-GitHub fixture test.
Source Gateway tests: 2 passed.
Architectures verified: macOS `arm64`, macOS `x86_64`, and Windows `x86_64`.
Windows executable verification: PE32+ GUI x86-64 with embedded
`/cyphes/atp/0.7.1` and `cyphes.repository-audit.v0.7.1` strings.

## SHA-256

```text
9c237b24c0da2593d3aaeca72164bde59689af1f0f6a2422a7ef99d9491a3711  CYPHES_0.7.1_aarch64.dmg
5e217c94b963b7fd983ee2c3f170212cf9df2038a8bbbb2b37ca2cc5af170bb5  CYPHES_0.7.1_x64.dmg
3fec67cfeb94725c3d01716dbaba4897a7b96b82931b42e0b4c07bf130a2c3ac  CYPHES_0.7.1_x64-setup.exe
```
