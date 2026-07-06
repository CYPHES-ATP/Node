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
- `SHA256SUMS.txt`

Windows x64 packaging remains pending a Windows-capable packaging host.
