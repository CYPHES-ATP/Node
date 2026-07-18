# CYPHES v0.16.10 Settlement Recovery

v0.16.10 preserves the `cyphes-final-testnet-v0.16.0` ledger marker and fixes
the synchronization pressure that could strand independently verified receipts
after a relay churn window.

## Fixes

- Checks advertised labor IDs against the complete local database instead of
  comparing two bounded 512-item inventory windows.
- Removes the duplicate inventory pull path; each missing object is transferred
  once through the inventory response.
- Prioritizes stale-receipt rebroadcasts before bulk inventory and reserves half
  of each peer's outbound request window for settlement traffic.
- Waits for an infrastructure connection to close before redialing, preventing
  relay reconnects from racing a socket stuck in `CLOSE_WAIT`.
- Drops remote RFC1918, link-local, loopback, and ULA routes learned through
  rendezvous while retaining same-LAN discovery through mDNS.
- Aggregates high-volume route and bundle telemetry by minute and retains at
  most seven days / 50,000 rows.

Relay limits remain unchanged at 64 MiB per circuit and 10 minutes per circuit.
This release does not self-verify receipts, raise worker caps, clear pending
rows, reset the database, or rewrite ATP credit history.

## Validation

- `npm run build`
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check`
- `cargo check --manifest-path src-tauri/Cargo.toml`
- `cargo test --manifest-path src-tauri/Cargo.toml`
- `cargo test --manifest-path relay/Cargo.toml`

## Assets

- `CYPHES_0.16.10_aarch64.dmg`
- `CYPHES_0.16.10_x64.dmg`
- `CYPHES_0.16.10_x64-setup.exe`
- `SHA256SUMS.txt`

## Checksums

```text
39166659c174925ec7f1df1ac18777dc6a27296151d10a54ed586c8414051b9e  CYPHES_0.16.10_aarch64.dmg
d2d584c06c09c016449c8e7c9e0f9adbd7afbda02a08b3499c5c0236d8ebb0e3  CYPHES_0.16.10_x64.dmg
```
