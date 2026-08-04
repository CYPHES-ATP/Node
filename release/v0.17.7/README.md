# CYPHES v0.17.7 Ollama Cloud Reliability

v0.17.7 is a narrow, wire-compatible reliability release. It preserves the
`cyphes-final-testnet-v0.16.0` genesis ledger marker, the `/cyphes/atp/0.15.1`
labor wire, existing databases, identities, receipts, balances, targets, and
settlement rules. No reset or migration is required.

## Empty-response recovery

- Retries transient Ollama request, stream, HTTP 408/429/5xx, and empty-content
  failures up to two times with bounded backoff.
- Omits Ollama's structured-output `format` constraint for cloud-tagged models,
  where that option is not supported, while retaining the strict Cognition
  Proof prompt and local schema validation/repair.
- Records sanitized response metadata for diagnosis: status, content type, raw
  byte and chunk counts, parsed lines, content and thinking byte counts,
  completion state, token count, and duration. Prompts and repository source
  are never written to these telemetry events.

## Headless claim recovery

- Resumes a work-unit claim already owned by the local worker instead of trying
  to claim the already-claimed unit again.
- Applies a one-minute cooldown after a failed run and permits at most two
  work-unit runs per active claim. A second terminal failure waits for the
  existing 15-minute claim-expiry recovery path instead of hammering the model
  provider on every 12-second autonomous tick.
- Clears local failure state when the unit succeeds or reopens for a new claim.

## Compatibility boundary

- No database schema or ledger change.
- No identity, receipt, contribution, verification, or settlement change.
- No P2P message, capability, wire-profile, relay, target, or model-economics
  change.
- Existing v0.17.x nodes remain network-compatible.

## Validation

- Ollama cloud request-shape regression test.
- Bounded retry-policy regression test.
- Ollama stream metadata parsing regression test.
- Headless cooldown and retry-ceiling regression tests.
- Full frontend and Rust validation results are recorded with the published
  release assets.

## Assets

The release includes Apple Silicon and Intel macOS DMGs, a Windows x64 NSIS
installer, and SHA-256 checksums.
