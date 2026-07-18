# CYPHES v0.16.11 Settlement Queue Recovery

v0.16.11 preserves the `cyphes-final-testnet-v0.16.0` ledger marker and repairs
two deterministic queue-starvation conditions that could strand receipts after
a relay outage even after connectivity returned.

## Fixes

- Limits stale-receipt repair to live `submitted` contributions whose work
  units have not already reached a reviewed-terminal state. Superseded history
  can no longer consume the entire 32-receipt repair window.
- Retries pending labor dependencies in causal order: campaigns, claims,
  contributions, then verifications. Each object class receives its own bounded
  retry window, so old missing verification targets cannot hide repairable
  contributions.
- Runs dependency repair on every network synchronization tick instead of only
  after new objects arrive.
- Rotates failed dependency objects by attempt count and update time, with
  exponential retry backoff capped at 256 seconds.
- Preserves retry state when duplicate sync bundles requeue the same dependency,
  preventing duplicate traffic from postponing its next repair attempt.
- Keeps contribution repair bundles paired with the signed historical claim
  that was valid when the contribution was created.

Relay limits remain unchanged at 64 MiB per circuit and 10 minutes per circuit.
This release does not self-verify receipts, raise worker caps, clear pending
rows, reset the database, or rewrite ATP credit history.

## Validation

- `npm run build`
- `node scripts/assert-genesis-auto-mode.mjs`
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check`
- `cargo check --manifest-path src-tauri/Cargo.toml`
- `cargo test --manifest-path src-tauri/Cargo.toml`
- `cargo fmt --manifest-path source-gateway/Cargo.toml --check`
- `cargo test --manifest-path source-gateway/Cargo.toml`
- `cargo test --manifest-path relay/Cargo.toml`

Regression coverage includes superseded-window starvation, settled-work-unit
exclusion, per-kind pending dependency fairness, and historical claim replay.

## Assets

- `CYPHES_0.16.11_aarch64.dmg`
- `CYPHES_0.16.11_x64.dmg`
- `CYPHES_0.16.11_x64-setup.exe`
- `SHA256SUMS.txt`

## Checksums

```text
d9e104fc55773196f3af26188d06519688ee3742207b06bed5af39890b8acc8c  CYPHES_0.16.11_aarch64.dmg
76c4b936451a6d07cd08611083892f4ab9ea394035f3902d81630444b748a57a  CYPHES_0.16.11_x64.dmg
06262aacf9aca5a639e4aebc9be0fd7d2a23bd910994257d995108b687090ed8  CYPHES_0.16.11_x64-setup.exe
```
