# CYPHES Node v0.7.4

Fresh isolated ATP testnet release for sustained autonomous network testing.

## Assets

- `CYPHES_0.7.4_aarch64.dmg` - macOS Apple Silicon
- `CYPHES_0.7.4_x64.dmg` - macOS Intel
- `CYPHES_0.7.4_x64-setup.exe` - Windows x64
- `SHA256SUMS.txt` - release checksums

## Hotfix Notes

- Raises the autonomous campaign seed cap from 24/day to 2400/day.
- Keeps the 2880/day Guardian observation and local model work-unit caps.
- Bumps ATP protocol and rendezvous namespace to v0.7.4 for a fresh isolated testnet.
- Keeps v0.7.3 verifier-safe boot behavior: nodes start in verifier/relay mode until `Run` is pressed.

## Verification

Run:

```sh
shasum -a 256 -c SHA256SUMS.txt
```
