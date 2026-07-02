# CYPHES v0.7.10

Hotfix release for audit-labor settlement liveness.

## What changed

- Stores expired signed work-unit claims as historical evidence without locking the work unit.
- Lets repaired contributions validate against the signed claim window instead of claim receipt time.
- Runs an immediate verifier pass after labor object bundle ingest.
- Persists audit-labor bundle, verification, and outbound failure telemetry to SQLite.
- Gates labor inventory by app version and capabilities.
- Moves ATP wire protocol to `/cyphes/atp/0.7.10` and rendezvous namespace to `cyphes.repository-audit.v0.7.10`.
- Lowers self-pending worker backpressure to `1` receipt for faster liveness failure detection.

## Testnet

This release keeps the existing SQLite testnet id `cyphes-dev-v0.7.7` so current stuck receipts can be repaired instead of hidden by a fresh network reset.

## Assets

- `CYPHES_0.7.10_aarch64.dmg` for Apple Silicon Macs.
- `CYPHES_0.7.10_x64.dmg` for Intel Macs.

Verify with `SHA256SUMS.txt` before installing.

## Verification

- `cargo test`
- `npm run build`
