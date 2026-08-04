# DeepSeek Ollama Cloud empty-response diagnostic snapshot

Captured for the v0.17.7 reliability release. This document contains no model
prompts, repository source, private keys, node identities, claim IDs, or ledger
rows.

## Operator-supplied observation

- Runtime: CYPHES headless node v0.17.6 on WSL2
- Provider: Ollama
- Model: `deepseek-v4-flash:0731-cloud`
- Session length: approximately four hours
- Successful contributions reported: 346
- Failed runs reported: 53
- Failures with `Ollama returned an empty streamed response`: 52
- Comparison: the operator did not observe the same failure concentration with
  `glm-5.2`

These counts came from the remote operator report. They were not present in the
local macOS node logs and are intentionally recorded as supplied evidence, not
as independently reproduced measurements.

## Locally verified v0.17.6 behavior

- Ollama `/api/chat` used streaming and requested `format: "json"`.
- A response with no assembled `message.content` returned the reported error.
- No bounded request retry existed.
- The autonomous failure path logged the error but retained the active claim.
- The following tick found the active local claim and then attempted the
  open-only claim operation again, preventing immediate resumption.
- The claim TTL is 15 minutes, after which network synchronization reopens an
  otherwise unsettled work unit. The unit was delayed, not permanently deleted.

## Data requested from operators after v0.17.7

For each model, report aggregate request attempts, successful contributions,
terminal failures, empty-content retry events, retry recoveries, latency, and
provider-reported token usage. Do not share prompts, source context, private
keys, full database files, or unredacted node logs publicly.
