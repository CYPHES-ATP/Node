# CYPHES v0.16.3 Mainnet Performance Upgrade

v0.16.3 is a non-mandatory mainnet performance upgrade. It keeps the existing
`cyphes-final-testnet-v0.16.0` ledger marker, ATP labor wire, receipt format,
and forward-only economics compatible with v0.16.1/v0.16.2 nodes.

This release targets the growing-ledger refresh path:

- Adds one aggregate backend dashboard endpoint for campaigns, network progress,
  and verified ATP summary.
- Replaces ordinary cockpit full-ledger snapshot refreshes with compact summary
  refreshes.
- Caches verified credit summaries by allocation ledger head.
- Coalesces duplicate network-triggered dashboard reloads.
- Lazy-loads full campaign snapshots for detailed receipt inspection and worker
  actions instead of every dashboard refresh.
- Adds the missing `credit_allocations(receiver_agent_id, issued_at,
  allocation_id)` index.

## Assets

- `CYPHES_0.16.3_aarch64.dmg` - Apple Silicon macOS
- `CYPHES_0.16.3_x64.dmg` - Intel macOS
- `CYPHES_0.16.3_x64-setup.exe` - Windows x64
- `SHA256SUMS.txt` - release checksums

## Compatibility

Older v0.16.1/v0.16.2 nodes can continue participating. v0.16.3 nodes should
feel smoother at larger ledger sizes, but this release does not fork the
network and does not require a database reset.

## Validation

- `npm run build`
- `cargo test --manifest-path src-tauri/Cargo.toml`
- `cargo test --manifest-path source-gateway/Cargo.toml`
- `shasum -a 256 -c SHA256SUMS.txt`

## Checksums

```text
f03a5ed02bef8c48dcc4206a369f5400390a2abe0b9fd1fda72f820c7f8d7b56  CYPHES_0.16.3_aarch64.dmg
1a7edb7d56161a729a13476be083b272f5e241fb386dbea54cbea105ac27470c  CYPHES_0.16.3_x64.dmg
38655e018a67f43475b450a4e76fc01b1ef01b212c2e8b827ae5a45469f034d7  CYPHES_0.16.3_x64-setup.exe
```
