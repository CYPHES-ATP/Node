# CYPHES v0.5.4 Developer Preview

Historical archive. The current public testnet seed is CYPHES v0.6.2; use the
current download in the root [README](../../README.md#download) unless you are
verifying this older release.

Apple Silicon DMGs:

- `CYPHES-v0.5.4-aarch64.dmg`
- `CYPHES-Requester-v0.5.4-aarch64.dmg`

SHA-256:

```text
cab3a5633c0e61caa5a106c007fb50a5b870bd37ea6f4ac4e2cfba237f5d73d7  CYPHES-v0.5.4-aarch64.dmg
c6b018f5ab1e80c428d4421f49e8a484157a9fb5e2da595de04ad320db3d5e72  CYPHES-Requester-v0.5.4-aarch64.dmg
```

Highlights:

- Genesis Auto Mode with Auto Worker, Auto Verifier, and Quest Seeder toggles.
- Local public DeFi guardian target index at `protocol/targets/guardian-target-index.json`.
- Auto Worker claims remote open work units, runs the selected local model, enforces the configured runtime limit, signs the contribution, and submits the receipt.
- Auto Verifier accepts pending signed contributions only for campaigns this node requested.
- Quest Seeder creates one public DeFi guardian campaign per day from the local target index.
- Live cockpit pulse for active nodes, open work, pending ATP, earned ATP, daily work progress, and local cognition rate.
- Pending ATP remains provisional. Earned ATP only changes after accepted verifier receipts.

Notarization:

- These builds are ad hoc signed but not Apple-notarized.
- Control-click the app, select **Open**, then confirm **Open** the first time.

Validation:

```text
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
hdiutil verify release/v0.5.4/CYPHES-v0.5.4-aarch64.dmg
hdiutil verify release/v0.5.4/CYPHES-Requester-v0.5.4-aarch64.dmg
```
