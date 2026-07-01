# CYPHES v0.7.2 Hotfix

v0.7.2 is a testnet hotfix for smoother node joining and rejoining.

## Binaries

- `CYPHES_0.7.2_aarch64.dmg` - macOS Apple Silicon
- `CYPHES_0.7.2_x64.dmg` - macOS Intel
- `CYPHES_0.7.2_x64-setup.exe` - Windows x64 NSIS setup

## Changes

- New nodes default to verifier/sync mode.
- Worker and campaign-seeding mode starts only after the user presses Run.
- The model dropdown no longer auto-selects a local model on startup.
- ATP wire protocol is isolated at `/cyphes/atp/0.7.2`.
- Rendezvous namespace is isolated at `cyphes.repository-audit.v0.7.2`.
- Pending receipt repair is more aggressive: the node rebroadcasts unresolved signed claims, contributions, and verification bundles every 12 seconds with larger scan windows.
- Verifier duty scans continue even during transient peer churn.

## Verification

- `npm run check`
- macOS Apple Silicon Tauri worker build
- macOS Intel Tauri worker build
- Windows x64 Tauri worker build via `cargo-xwin`

macOS builds are ad hoc signed and not notarized. The Windows setup build is unsigned for testnet use.
