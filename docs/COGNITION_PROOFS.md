# CYPHES Cognition Proofs

Cognition Proofs are the signed work packets CYPHES uses to turn local model
labor into verifier-settled ATP Credits.

Every new paid contribution carries a `cognitionProof` object. The packet binds
six things into the worker receipt:

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

The pre-rename proof field and profile are treated as legacy wire aliases for
older testnet receipts. New v0.15.1 receipts and schemas use Cognition Proof
naming.
