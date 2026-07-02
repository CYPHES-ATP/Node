# CYPHES v0.7.9

Transport hotfix for the current `cyphes-dev-v0.7.7` testnet.

## Changes

- Adds relay-circuit dialing for peers discovered through rendezvous.
- Keeps advertised direct/private peer addresses, but also dials the canonical relay route so verifier traffic can move when direct LAN/private addresses fail.
- Preserves the current testnet database and protocol namespace; no network reset is required.

## Assets

- `CYPHES_0.7.9_aarch64.dmg` - macOS Apple Silicon
- `CYPHES_0.7.9_x64.dmg` - macOS Intel

Windows x64 packaging remains unavailable from this local build machine because the MSVC cross-build toolchain is missing `llvm-rc`.
