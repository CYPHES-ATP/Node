# CYPHES v0.16.8 Mainnet Relay Recovery Hotfix

v0.16.8 is a non-mandatory mainnet relay/rejoin hotfix. It keeps the existing
`cyphes-final-testnet-v0.16.0` ledger marker, ATP labor wire, receipt format,
and forward-only economics compatible with earlier v0.16.x mainnet nodes.

This release focuses on the 0-peer / 1-active-peer / stuck verifier backlog
edge case observed after peers left the network:

- Removes libp2p demo-default relay reservation and circuit-source rate limiters
  from the public CYPHES relay configuration.
- Keeps the raised relay reservation and circuit budgets from v0.16.7.
- Makes desktop nodes drop stale relay circuit addresses when relay reservations
  close or expire.
- Re-reserves relay circuits when the relay connection is still alive but the
  p2p-circuit listener is gone.
- Re-registers with rendezvous immediately after a relay reservation is accepted.
- Adds relay reservation lost/requested telemetry for postmortem diagnosis.
- Preserves verifier integrity: no self-verification, no forced receipt
  clearing, no database reset, and no ATP rewrite.

## Assets

- `CYPHES_0.16.8_aarch64.dmg` - Apple Silicon macOS
- `CYPHES_0.16.8_x64.dmg` - Intel macOS
- `SHA256SUMS.txt` - release checksums

## Validation

- `npm run build`
- `cargo check --manifest-path src-tauri/Cargo.toml`
- `cargo test --manifest-path src-tauri/Cargo.toml`
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check`
- `cargo check --manifest-path relay/Cargo.toml`
- `cargo test --manifest-path relay/Cargo.toml`
- `cargo fmt --manifest-path relay/Cargo.toml --check`
- Relay deployed to Fly machine `32870902c72d28`, version `8`
- Relay smoke reservation accepted through `relay.cyphes.com`

## Checksums

```text
a14a13ad45382b0209c7b737636d1a1f653f9bf32a77393fb853256d3b6b8e7e  CYPHES_0.16.8_aarch64.dmg
751a8b6d9d6970809dbcbbbf5307caaef874b6a1b2d68acae9f35a8002843a5a  CYPHES_0.16.8_x64.dmg
```
