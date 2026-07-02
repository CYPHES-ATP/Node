# CYPHES Node v0.7.6

Autonomous network repair release for the v0.7 testnet.

## Assets

- `CYPHES_0.7.6_aarch64.dmg` - macOS Apple Silicon
- `CYPHES_0.7.6_x64.dmg` - macOS Intel
- `CYPHES_0.7.6_x64-setup.exe` - Windows x64
- `SHA256SUMS.txt` - release checksums

## Hotfix Notes

- Starts a fresh isolated ATP testnet namespace: `cyphes-dev-v0.7.6`.
- Uses ATP wire protocol `/cyphes/atp/0.7.6` and rendezvous namespace `cyphes.repository-audit.v0.7.6`.
- Adds narrow inventory resync for active work-unit claims, unverified contributions, and verification bundles.
- Adds stale receipt repair: old submitted receipts rebroadcast with their signed claim before the contribution.
- Adds stale claim repair: abandoned claims expire locally after the signed TTL and reopen work unless a contribution was already submitted.
- Keeps nodes verifier-first by default; model work still starts only after `Run`.
- Expands the bundled Guardian Index from 100 to 142 curated public coverage targets for longer 24-hour testnet runs.

## Verification

Run:

```sh
shasum -a 256 -c SHA256SUMS.txt
```
