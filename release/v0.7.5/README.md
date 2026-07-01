# CYPHES Node v0.7.5

Fresh isolated ATP testnet release for smoother node join/rejoin testing.

## Assets

- `CYPHES_0.7.5_aarch64.dmg` - macOS Apple Silicon
- `CYPHES_0.7.5_x64.dmg` - macOS Intel
- `CYPHES_0.7.5_x64-setup.exe` - Windows x64
- `SHA256SUMS.txt` - release checksums

## Hotfix Notes

- Starts a fresh isolated ATP testnet namespace: `cyphes-dev-v0.7.5`.
- Uses a testnet-scoped SQLite ledger so stale balances and receipts from older testnets do not load into v0.7.5.
- Preserves node identity while isolating ledger data by app/testnet version.
- Keeps verifier/relay-safe startup behavior: nodes join the network without doing model work until `Run` is pressed.
- Repairs the requester self-work flow so locally seeded campaigns still claim work before submitting contributions, while still requiring independent verification for ATP settlement.
- Keeps claim-before-contribution enforcement for inbound and local contribution recording.

## Verification

Run:

```sh
shasum -a 256 -c SHA256SUMS.txt
```
