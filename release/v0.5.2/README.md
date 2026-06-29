# CYPHES v0.5.2 Developer Preview

Historical archive. The current public testnet seed is CYPHES v0.6.2; use the
current download in the root [README](../../README.md#download) unless you are
verifying this older release.

Apple Silicon developer preview for the CYPHES ATP audit-labor network.

## Downloads

- `CYPHES-v0.5.2-aarch64.dmg` - worker/operator node, now branded simply as CYPHES.
- `CYPHES-Requester-v0.5.2-aarch64.dmg` - requester/admin node for creating campaigns, verifying contributions, and exporting report bundles.

## What changed

- Adds live cockpit telemetry to the worker runtime: 200ms UI sampling, larger tokens/sec display, pulsing progress, phase labels, pending ATP Credits, and a live event stream.
- Keeps ATP Credits honest: pending/provisional while work runs, earned only after requester verification returns a signed receipt.
- Preserves the v0.5.1 two-node flow: remotely claimable work units, local-model audit execution, contribution replay when requester/worker reconnect, verification, credits, and report export.
- Renames the worker app from CYPHES Worker to CYPHES.

## Checksums

```text
34d6190f6d587ef3d9ba562865ce667e02d51359c1d1108660838feea8dc0d23  CYPHES-Requester-v0.5.2-aarch64.dmg
f0baed0f08f0e977ac617198c38aa351e1bd84044c050b8ea3eaed1f68c49623  CYPHES-v0.5.2-aarch64.dmg
```

## Signing

These DMGs are ad hoc signed developer previews. They are not Apple-notarized production releases.
