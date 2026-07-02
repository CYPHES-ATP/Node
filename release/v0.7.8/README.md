# CYPHES v0.7.8

Hotfix release for verifier liveness and P2P reconnect hygiene on the current `cyphes-dev-v0.7.7` testnet.

## Changes

- Enforces the pending receipt backpressure cap inside contribution ingest so direct producer paths cannot bypass it.
- Replaces reconnect-time full labor-object replay with inventory-based resync.
- Forces verifier liveness rediscovery when self-authored receipts stay pending and independent verification goes stale.
- Surfaces verifier/peer resync telemetry in the app.

## Assets

- `CYPHES_0.7.8_aarch64.dmg` - macOS Apple Silicon
- `CYPHES_0.7.8_x64.dmg` - macOS Intel

Windows x64 packaging is not included in this release because the MSVC cross-build toolchain was missing the Windows resource compiler (`llvm-rc`). Do not reuse an older Windows installer as v0.7.8.
