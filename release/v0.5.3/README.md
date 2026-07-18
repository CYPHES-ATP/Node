# CYPHES v0.5.3 Developer Preview

Historical archive. The current public testnet seed is CYPHES v0.6.2; use the
current download in the root [README](../../README.md#download) unless you are
verifying this older release.

Apple Silicon developer preview for the CYPHES ATP audit-labor network.

## Downloads

- `CYPHES-v0.5.3-aarch64.dmg` - worker/operator node.
- `CYPHES-Requester-v0.5.3-aarch64.dmg` - requester/admin node for creating campaigns, verifying contributions, and exporting report bundles.

## What changed

- Streams local model output from LM Studio and Ollama while the audit skill runs.
- Updates tokens/sec from streamed runtime chunks instead of waiting for the final response.
- Keeps pending ATP provisional during execution and converts to earned ATP only after requester verification.
- Shrinks the cockpit into a leaner header: small status/provider/model stack on the left, four live telemetry instruments on the right.
- Preserves remotely claimable work units, signed contributions, verification receipts, credits, and report export.

## Checksums

```text
0e8df48177a6723d0fb612d16ef0e4d72c81051db7fb9ad264fe5af87fd3f78e  CYPHES-Requester-v0.5.3-aarch64.dmg
7cdd79eaad4a76997301b942d317d8ca1c75c92db4663c11726e4e7b5bc11819  CYPHES-v0.5.3-aarch64.dmg
```

## Signing

These DMGs are ad hoc signed developer previews. They are not Apple-notarized production releases.
