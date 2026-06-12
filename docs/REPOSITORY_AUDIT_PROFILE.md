# Repository Audit ATP Profile

Status: contract profile implemented; routing, execution, and receipt production
remain planned.

Profile version: `0.1`

## Purpose

This profile turns the generic ATP objects in the May 21, 2026 specification
into one interoperable work order: an independent worker audits one public
GitHub repository at one exact commit.

The implementation is split deliberately:

1. `DISCOVER` commits the repository, exact commit, requested scope, and a
   visibly non-payable proposed commercial term.
2. The worker's first `NEGOTIATE` event contains the complete audit contract.
3. The requester's second `NEGOTIATE` event accepts the canonical
   `contractHash`.
4. Future `ROUTE`, `EXECUTE`, `SETTLE`, and `ATTEST` work must extend that
   accepted contract without silently replacing it.

## Contract Invariants

The contract profile is `cyphes.repository-security-audit/0.1`.

- Repository URLs must be canonical public GitHub URLs.
- The repository must be pinned to a 40- or 64-character Git commit SHA.
- Repository access is read-only and limited to that commit.
- The checkout is deleted after receipt production.
- The five required artifacts are fixed by path.
- The current settlement is explicitly `zero-value`.
- The amount shown in the desktop form remains a `non-payable-term`.
- Worker signature, requester approval, artifact hashes, and event-chain
  verification are required by the proof policy.
- The offer envelope expiry and contract expiry must match.

The contract hash is:

```text
sha256(JCS(contract))
```

The requester accepts that hash, not an uncommitted UI representation.

Requests signed before this profile do not contain a commit SHA. They remain
valid historical events, but the client marks them as unpinned and requires a
new request instead of rewriting signed state.

## Required Artifacts

```text
artifacts/audit-report.md
artifacts/findings.json
artifacts/results.sarif
artifacts/checks.json
artifacts/manifest.json
```

The future manifest must bind each artifact path, media type, byte size, and
SHA-256 digest.

## Receipt Profile

The receipt profile is
`cyphes.repository-security-audit-receipt/0.1`.

It binds:

- the accepted contract hash;
- the exact repository commit and scope;
- exercised leases and accessed resources;
- artifact paths, hashes, media types, and sizes;
- requester approval;
- zero-value settlement;
- terminal ATP event root;
- receipt signatures.

`receiptHash` is computed over the JCS-canonical receipt after removing
`receiptHash` and `signatures`. Signatures bind that hash.

The Node currently validates the structural receipt profile and canonical hash
in tests, including the presence of distinct worker and requester signature
records. The fixture signature strings are non-cryptographic test values. The
Node does not yet produce or accept a receipt in the live transaction flow.
Artifact Two will remain the independent verifier for signatures, event
continuity, lease evidence, and artifact bytes.

## Deterministic Failures

Profile validation returns ATP wire codes with a stable profile reason:

```text
ATP_BAD_STATE: AUDIT_CONTRACT_REPOSITORY_UNPINNED: ...
ATP_BAD_STATE: AUDIT_CONTRACT_HASH_MISMATCH: ...
ATP_PROOF_UNSATISFIED: AUDIT_RECEIPT_HASH_MISMATCH: ...
ATP_PROOF_UNSATISFIED: AUDIT_RECEIPT_ARTIFACT_MISSING: ...
```

The wire code is suitable for ATP acknowledgements. The profile reason is
suitable for developer diagnostics and Artifact Two result mapping.

## Fixtures

Canonical examples live in:

```text
protocol/schemas/repository-audit-contract.v0.1.schema.json
protocol/schemas/repository-audit-receipt.v0.1.schema.json
protocol/fixtures/repository-audit-contract.v0.1.json
protocol/fixtures/repository-audit-receipt.v0.1.json
```

They are protocol fixtures, not evidence that an audit occurred. The Rust test
suite parses and validates both.

## Next Implementation Step

Add `ROUTE` using the accepted contract hash, an exact repository resource
descriptor, and an expiring read-only context lease. The worker must not clone
or inspect the repository until that lease is committed and verified.
