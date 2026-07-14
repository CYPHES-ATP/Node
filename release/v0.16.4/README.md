# CYPHES v0.16.4 Mainnet Hotfix

v0.16.4 is a non-mandatory mainnet hotfix over v0.16.3. It keeps the existing
`cyphes-final-testnet-v0.16.0` ledger marker, ATP labor wire, receipt format,
and forward-only economics compatible with v0.16.1/v0.16.2/v0.16.3 nodes.

This release fixes a long-running worker cockpit issue:

- Refreshes local claim state before running a cached claimed work unit.
- Treats missing local claim state as stale UI state instead of repeatedly
  flashing "Claim the work unit before running it."
- Keeps backend claim enforcement strict; workers still cannot submit without a
  valid signed claim.
- Preserves the v0.16.3 aggregate dashboard summary, cached credit refreshes,
  network event coalescing, and lazy campaign snapshot loading.

## Assets

- `CYPHES_0.16.4_aarch64.dmg` - Apple Silicon macOS
- `CYPHES_0.16.4_x64.dmg` - Intel macOS
- `CYPHES_0.16.4_x64-setup.exe` - Windows x64
- `SHA256SUMS.txt` - release checksums

## Validation

- `npm run build`
- `cargo check --manifest-path src-tauri/Cargo.toml`
- `cargo test --manifest-path src-tauri/Cargo.toml network_progress_summary_tracks_pending_and_verified_work`

## Checksums

```text
334f71004c17fab469e464cc201a7ed801b520b523f01f4050369cd3299270a9  CYPHES_0.16.4_aarch64.dmg
9db2081125f06f938614caf1dbe26a8611cc7e28be6d6e41c37b99f1f90321ad  CYPHES_0.16.4_x64.dmg
aece48e1347437caf3267288d8e5c29483e10b771ade1282d9b0e4d4c9d78811  CYPHES_0.16.4_x64-setup.exe
```
