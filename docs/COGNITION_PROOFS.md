# CYPHES Cognition Proofs

Cognition Proofs are the signed work packets CYPHES uses to turn local model
labor into verifier-settled ATP Credits.

Every new paid contribution carries a Cognition Proof packet. On the current
`cyphes-dev-v0.7.7` testnet, v0.15.7 serializes that packet through the legacy
`defenseProof` wire alias/profile so mixed verifier nodes can validate the same
canonical contribution hash. The app, docs, schema, and UI still refer to the
primitive as a Cognition Proof. The packet binds six things into the worker
receipt:

- **Target**: campaign, work unit, protocol, repository, commit, scope hash, and
  authorization hash.
- **Claim**: the security hypothesis or coverage claim the local model worked
  on.
- **Method**: runtime adapter, local model, skill/input/output hashes,
  constraints, and command trail.
- **Evidence**: notes hash, artifact hashes, finding counts, coverage counts,
  and reproducible steps when available.
- **Quality**: parser fallback status, structured-output status, quality tier,
  and deterministic multiplier.
- **Settlement**: the ATP credit profile and the independent-verifier finality
  rule.

An independent verifier does not wait through a challenge window. It accepts,
rejects, reproduces, or requests revision for the signed contribution. Accepted
work receives an `autonomousFinality` packet bound to both the contribution
receipt hash and the Cognition Proof hash, then ATP Credits settle immediately.

This is not a claim that every local model output is a valid vulnerability.
Cognition Proofs make the work reproducible, accountable, penalizable, and
settleable. Final reports and ATP balances still require accepted independent
verification.

v0.15.7 keeps the v0.15.3 proof-quality gate before settlement. The local audit runtime now
prompts for a required JSON Cognition Proof shape, requires non-empty
evidence-backed coverage, allows empty findings for valid no-issue results, and
tries one automatic repair pass when a model returns prose. Outputs that still
fail the schema remain parser-fallback coverage and keep the deterministic ATP
quality deduction.

v0.15.7 also preserves the v0.15.4 duplicate/superseded preflight and adds a
stable reconnect path: stale local claims are released when signed independent
verifier receipts prove a work unit is already settled, superseded self-pending
receipts no longer count against worker backpressure, and catch-up sync can
move larger verified response batches without changing ATP finality.

The pre-rename proof field and profile are treated as the compatibility wire
form for the live testnet. Future fresh-network capability-gated testnets can
switch the canonical wire field/profile to `cognitionProof` /
`cyphes.cognition-proof/0.1` without breaking existing receipts.
