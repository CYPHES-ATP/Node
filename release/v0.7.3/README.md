# CYPHES Node v0.7.3

Hotfix release for the public ATP testnet.

## Assets

- `CYPHES_0.7.3_aarch64.dmg` - macOS Apple Silicon
- `CYPHES_0.7.3_x64.dmg` - macOS Intel
- `CYPHES_0.7.3_x64-setup.exe` - Windows x64
- `SHA256SUMS.txt` - release checksums

## Hotfix Notes

- Nodes boot into verifier/relay mode by default.
- Work generation is session-gated behind the UI `Run` action.
- Persisted v0.7.2 auto-worker and quest-seeder settings are ignored on v0.7.3 boot.
- ATP protocol and rendezvous namespace are bumped to v0.7.3 so older testnet nodes do not silently mix with this hotfix network.

## Verification

Run:

```sh
shasum -a 256 -c SHA256SUMS.txt
```
