# CYPHES Cognition Proofs

Cognition Proofs are the signed work packets CYPHES uses to turn local model
labor into verifier-settled ATP Credits.

Every new paid contribution carries a Cognition Proof packet. v0.16.2 keeps the
`cyphes-final-testnet-v0.16.0` ledger marker as the mainnet genesis identifier
and continues serializing that packet through the legacy `defenseProof` wire
alias/profile so rolling verifier nodes validate the same canonical
contribution hash. The app, docs, schema, and UI still refer to the primitive as
a Cognition Proof. The packet binds six things into the worker receipt:

- **Target**: campaign, work unit, protocol, repository, commit, scope hash, and
  authorization hash.
- **Claim**: the security hypothesis or coverage claim the local model worked
  on.
- **Method**: runtime adapter, model, provider class, declared parameter tier,
  app version, worker mode, skill/input/output hashes, constraints, and command
  trail.
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

v0.16.2 keeps the proof-quality gate before settlement. The local audit runtime
prompts for a required JSON Cognition Proof shape, requires non-empty
evidence-backed coverage, allows empty findings for valid no-issue results, and
tries one automatic repair pass when a model returns prose. Outputs that still
fail the schema remain parser-fallback coverage and keep the deterministic ATP
quality deduction.

v0.16.2 also preserves the duplicate/superseded preflight, stays on the
mainnet genesis database marker, and keeps the stable reconnect path:
stale local claims are released when signed independent verifier receipts prove
a work unit is already settled, superseded self-pending receipts no longer count
against worker backpressure, and catch-up sync can move larger verified
response batches without changing ATP finality.

Reportable bounty candidates now need concrete file/function/line evidence,
exploit path, impact, and reproduction steps. Low-evidence structured coverage
is still useful, but it earns the lower proof-quality tier instead of pretending
to be bounty-grade output. Strong model economics are forward-only: v0.16.2
raises `minimax-m3` to `10.0x`, adds explicit frontier/cloud tiers, and signs
the multiplier into new receipts without rewriting old ATP allocations.

The pre-rename proof field and profile are treated as the compatibility wire
form for the live testnet. Future fresh-network capability-gated testnets can
switch the canonical wire field/profile to `cognitionProof` /
`cyphes.cognition-proof/0.1` without breaking existing receipts.
