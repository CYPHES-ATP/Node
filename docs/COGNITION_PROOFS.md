# CYPHES Cognition Proofs

Cognition Proofs are the signed work packets CYPHES uses to turn local model
labor into verifier-settled ATP Credits.

Every new paid contribution carries a Cognition Proof packet. On the current
`cyphes-dev-v0.7.7` testnet, v0.15.2 serializes that packet through the legacy
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

The pre-rename proof field and profile are treated as the compatibility wire
form for the live testnet. Future fresh-network capability-gated testnets can
switch the canonical wire field/profile to `cognitionProof` /
`cyphes.cognition-proof/0.1` without breaking existing receipts.
