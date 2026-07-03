# CYPHES Node v0.7.13

24/7 liveness and release-surface cleanup for the current ATP verifier testnet.

## Assets

- `CYPHES_0.7.13_aarch64.dmg` - macOS Apple Silicon
- `CYPHES_0.7.13_x64.dmg` - macOS Intel
- `CYPHES_0.7.13_x64-setup.exe` - Windows x64
- `SHA256SUMS.txt` - release checksums

## Hotfix Notes

- Keeps the current `cyphes-dev-v0.7.7` SQLite testnet state.
- Moves the ATP wire protocol to `/cyphes/atp/0.7.13`.
- Moves rendezvous discovery to `cyphes.repository-audit.v0.7.13`.
- Nodes boot as verifier/sync participants by default.
- Run enables local model work and campaign seeding for the current session.
- Stop returns the node to verifier-only mode without shutting down sync or verifier duties.
- Persisted settings cannot auto-resume local model work or quest seeding after restart.
- Peer fanout is bounded and outbound failures apply per-peer cooldowns to reduce outbound-stream storms.
- Public README and install docs now point to current macOS and Windows assets.

## Verification

- `npm run check`
- macOS Apple Silicon Tauri worker build
- macOS Intel Tauri worker build
- Windows x64 Tauri worker build through the local `cargo-xwin` toolchain
- `hdiutil verify` for both macOS DMGs
- `shasum -a 256 -c SHA256SUMS.txt`

macOS builds are ad hoc signed and not notarized. The Windows setup build is unsigned for testnet use.
