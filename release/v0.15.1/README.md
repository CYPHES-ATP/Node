# CYPHES v0.15.1

v0.15.1 adds autonomous finality with standardized Cognition Proof packets.

- New contributions carry signed `cognitionProof` metadata for target, claim,
  method, evidence, quality, and settlement rule.
- New verifier acceptances carry signed `autonomousFinality` metadata bound to
  the contribution receipt hash and Cognition Proof hash.
- ATP still settles immediately after independent verification; there is no
  challenge-window pause.
- Parser fallback output remains accepted only with the existing quality
  deduction.

## Checksums

See `SHA256SUMS.txt`.
