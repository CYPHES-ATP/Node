# CYPHES v0.5.6 Developer Preview

Release date: 2026-06-26

## Downloads

- `CYPHES-v0.5.6-aarch64.dmg`
- `CYPHES-Partner-v0.5.6-aarch64.dmg`

These Apple Silicon macOS builds are ad hoc signed and verified locally, but
not Apple-notarized yet.

## What Changed

- Renames the admin console from **CYPHES Requester** to **CYPHES Partner**.
- Consolidates the main node cockpit: Provider/Model, Tokens/sec, ATP earned,
  Pending, and Active Nodes are the primary top-level instruments.
- Removes the separate Network Alive panel from the main node app.
- Adds GitHub authenticated-read support through `CYPHES_GITHUB_TOKEN`,
  `GITHUB_TOKEN`, `~/.cyphes/github.token`, or `githubToken` in
  `~/.cyphes/settings.json`.
- Adds visible GitHub rate-limit/backoff state so the autonomous loop pauses
  instead of looking frozen.
- Uses canonical GitHub repository URLs after redirects/renames, preventing
  false campaign validation failures.
- Quarantines stale/unavailable Guardian Index rows for 24 hours and advances
  the work cursor.
- Strengthens duplicate campaign persistence suppression for the same
  requester/repository/commit/scope tuple.
- Makes verification/credit receipt retries idempotent so reconnects resend the
  existing receipt instead of minting duplicate credit events.
- Pins GitHub pause state inside the cockpit telemetry stream until GitHub
  reads resume, and tightens the autonomous campaign brief spacing.

## Verification

```text
npm run build
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
codesign --verify --deep --strict --verbose=2 CYPHES.app
codesign --verify --deep --strict --verbose=2 "CYPHES Partner.app"
hdiutil verify CYPHES-v0.5.6-aarch64.dmg
hdiutil verify CYPHES-Partner-v0.5.6-aarch64.dmg
```

Rust tests: 35 passed, 1 intentionally ignored live-GitHub fixture test.

## SHA-256

```text
b1b197ad73b7af1c73fc61feb58701fcd9a0fc7c8e95f06970654e2fd52a6e1d  CYPHES-v0.5.6-aarch64.dmg
77310d99947549c1eeadd039bc41f2745ce70a53554dceaa74c6baff41f7d804  CYPHES-Partner-v0.5.6-aarch64.dmg
```
