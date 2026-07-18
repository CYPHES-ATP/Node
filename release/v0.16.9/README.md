# CYPHES v0.16.9 Mainnet Relay Reservation Hotfix

v0.16.9 is a non-mandatory mainnet rejoin hotfix. It preserves the existing
`cyphes-final-testnet-v0.16.0` ledger marker, ATP labor wire, Cognition Proof
receipts, and forward-only economics.

This release targets the stuck 25-pending-receipt state observed when nodes left
and rejoined the public relay network:

- Separates relay reservation requested state from relay reservation accepted
  state.
- Advertises relayed listen addresses only after the relay confirms the
  reservation.
- Retries relay reservations that remain pending for 45 seconds without
  acceptance.
- Removes stale relay listeners before requesting a fresh relay circuit.
- Keeps receipt integrity unchanged: no self-verification, no forced clearing,
  no database reset, and no ATP rewrite.

## Assets

- `CYPHES_0.16.9_aarch64.dmg` - Apple Silicon macOS
- `CYPHES_0.16.9_x64.dmg` - Intel macOS
- `CYPHES_0.16.9_x64-setup.exe` - Windows x64
- `SHA256SUMS.txt` - release checksums

## Validation

- `npm run build`
- `cargo check --manifest-path src-tauri/Cargo.toml`
- `cargo test --manifest-path src-tauri/Cargo.toml`
- `cargo test --manifest-path relay/Cargo.toml`

## Checksums

```text
fd3b9c2356a62b1a3dae20754a61ddad9cb7de1f0bf8e3f771798872846d0f17  CYPHES_0.16.9_aarch64.dmg
bc1804d9198fd407a4a9fe1177b10afa4ea42a84bd8b5a3dfde3df73af5420bf  CYPHES_0.16.9_x64.dmg
210a59a385d245d95bf6f7d8d9e27c3ef8847a234c076ff037d9e73ff23d9762  CYPHES_0.16.9_x64-setup.exe
```
