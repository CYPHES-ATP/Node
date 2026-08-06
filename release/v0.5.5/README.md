# CYPHES v0.5.5 Developer Preview

Historical archive. The current public testnet seed is CYPHES v0.6.2; use the
current download in the root [README](../../README.md#download) unless you are
verifying this older release.

Apple Silicon DMGs:

- `CYPHES-v0.5.5-aarch64.dmg`
- `CYPHES-Requester-v0.5.5-aarch64.dmg`

SHA-256:

```text
d50dbb91ff943aa5a849f70119734cd6ba54aeb9e289871de0c33124ee03d17d  CYPHES-v0.5.5-aarch64.dmg
e7f7affc0e14c567b91cf4b294b94b0d75fa70437f70bb79966552d7515b67e9  CYPHES-Requester-v0.5.5-aarch64.dmg
```

## What Changed

- Main CYPHES app is now autonomous by default. Auto Worker, Auto Verifier, and
  Quest Seeder are on without user toggles.
- Manual Work Orders controls were removed from the main node UI.
- New autonomous cockpit shows local model tokens/sec, pending ATP, earned ATP,
  progress, peers, target metadata, and live receipt events.
- Guardian Index v2 includes 100 structured public coverage targets with
  source signals, category, chains, static TVL/risk rank seed, GitHub repos,
  focused paths, docs/security references, criticality, and priority score.
- CYPHES watches target commits and suppresses duplicate unchanged
  target/path/commit campaigns.
- Browser preview now reads the same bundled Guardian Index for honest README
  screenshots.
- `campaign.html` remains the admin/protocol console for manual campaign
  creation, verification inspection, ATP proof logs, and report export.

## What It Does Not Do

- Does not submit external reports.
- Does not contact protocols.
- Does not claim bounty payouts.
- Does not move funds.
- Does not make ATP Credits transferable tokens.
- Does not make unverified model output a confirmed vulnerability.

## Verification

```bash
npm run check
cargo fmt --manifest-path relay/Cargo.toml --check
cargo test --manifest-path relay/Cargo.toml
codesign --verify --deep --strict --verbose=2 src-tauri/target/release/bundle/macos/CYPHES.app
codesign --verify --deep --strict --verbose=2 "src-tauri/target/release/bundle/macos/CYPHES Requester.app"
hdiutil verify release/v0.5.5/CYPHES-v0.5.5-aarch64.dmg
hdiutil verify release/v0.5.5/CYPHES-Requester-v0.5.5-aarch64.dmg
```

Current verification results:

- `npm run check`: passed; 34 Rust tests passed, 1 ignored archive/network integration test.
- Relay fmt/tests: passed.
- Codesign verification: passed for both app bundles.
- DMG verification: passed for both DMGs.

These builds are ad hoc signed and not Apple-notarized.
