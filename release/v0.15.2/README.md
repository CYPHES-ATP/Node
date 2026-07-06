# CYPHES v0.15.2

Compatibility hotfix for the live `cyphes-dev-v0.7.7` verifier testnet.

v0.15.2 keeps the v0.15.1 ATP stream and rendezvous namespace while restoring
legacy signed proof wire compatibility. New work is still presented as
Cognition Proof work in the app, but contributions serialize through the
legacy `defenseProof` wire alias/profile so mixed verifier nodes can validate
the same canonical contribution hash.

Assets:

- `CYPHES_0.15.2_aarch64.dmg`
- `CYPHES_0.15.2_x64.dmg`
- `CYPHES_0.15.2_x64-setup.exe`
- `SHA256SUMS.txt`

The macOS builds are ad hoc signed and verified locally, but not
Apple-notarized yet. The Windows x64 setup build is an unsigned NSIS installer
cross-built from macOS for verifier testnet use.
