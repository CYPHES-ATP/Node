# CYPHES v0.15.7

Stable rolling upgrade for the active Proof of Cognition testnet.

v0.15.7 keeps the existing `cyphes-dev-v0.7.7` testnet state and the
`/cyphes/atp/0.15.1` labor wire. It is intended for all current nodes.

## What Changed

- Preserves the v0.15.4 duplicate/superseded labor-object preflight.
- Releases stale local claims when signed independent verifier receipts prove a
  work unit already settled.
- Excludes superseded self-authored receipts from worker backpressure so old
  catch-up objects do not halt new work.
- Raises libp2p response capacity for reconnect/catch-up sync.
- Byte-caps labor bundles so large catch-up rounds paginate instead of flooding
  peers.
- Keeps verifier-first startup, explicit Run/Stop work mode, target-completion
  epochs, sparse inventory, reachable-route preference, and structured
  Cognition Proof enforcement.
- Does not include the experimental fair-work or no-self-dealing rule changes.

## Assets

- `CYPHES_0.15.7_aarch64.dmg` - macOS Apple Silicon
- `CYPHES_0.15.7_x64.dmg` - macOS Intel
- `CYPHES_0.15.7_x64-setup.exe` - Windows x64

macOS builds are ad hoc signed and not Apple-notarized. The Windows setup build
is unsigned and intended for testnet use.

## Verification

- `npm run build`
- `node scripts/assert-genesis-auto-mode.mjs`
- `cargo check --manifest-path src-tauri/Cargo.toml`
- `cargo test --manifest-path src-tauri/Cargo.toml`
- `cargo test --manifest-path source-gateway/Cargo.toml`
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
3c0a621e98a90848eef9efaaaaf5f9ee7ed2cc539e0d28a835071ff8e77d26c0  CYPHES_0.15.7_aarch64.dmg
ff45562cd960c19cc9933903706b024465c6022bf0754574e5520d9c4b5aa1c2  CYPHES_0.15.7_x64.dmg
ba1ae887504ab31acd21beacabbea7143017bc83008aefe04976a304153702c3  CYPHES_0.15.7_x64-setup.exe
```
