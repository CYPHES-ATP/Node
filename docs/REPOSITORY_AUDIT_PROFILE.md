# Repository Audit ATP Profile

Status: implemented developer-preview vertical slice

Profile version: `0.1`

## Purpose

This profile defines one interoperable ATP work order: an independent worker
audits a public GitHub repository at an exact commit and returns a verifiable
Proof of Cognition.

## Transaction

1. `DISCOVER` commits repository, commit, scope, and non-payable proposed term.
2. Worker `NEGOTIATE` commits the full audit contract.
3. Requester `NEGOTIATE` selects its canonical `contractHash`.
4. Requester `ROUTE` grants signed repository-read and artifact-write leases.
5. Worker performs bounded deterministic activity under those leases.
6. Worker sends a signed result containing access evidence and artifacts.
7. Requester verifies the result and emits zero-value `SETTLE`.
8. Worker emits `ATTEST` with a signed Proof of Cognition.

There is no `EXECUTE` wire envelope in this profile. Execution is the bounded
activity authorized by `ROUTE` and evidenced before `SETTLE`.

## Contract Invariants

- Canonical public GitHub URL.
- Exact 40- or 64-character commit SHA.
- Read-only repository access.
- No repository code execution.
- No network use after the pinned archive fetch.
- Checkout deletion after result production.
- Maximum 3,600-second contract duration.
- Five required artifact paths.
- Proposed ATP Credits amount marked `receipt-backed-credit-estimate`.
- Actual settlement fixed to `zero-value`.
- Worker signature, requester approval, artifact hashes, and event chain
  required.

Legacy fixtures may still contain non-payable USDC terms so older ATP-L1
receipt bundles remain verifiable. New desktop-created requests use ATP
Credits only.

`contractHash = sha256(JCS(contract))`.

## Context Leases

`ROUTE` contains two requester-signed leases:

```text
github:<owner>/<repo>@<commit>
  operations: [read]

artifacts:<transaction-id>
  operations: [write]
```

Both leases bind issuer, resource, operation, purpose, boundary, retention,
audit policy, nonce, and TTL. The worker rejects inactive, widened, unsigned,
or contract-mismatched leases.

The worker additionally rejects archive path escape, symlink, hardlink, and
artifact namespace escape.

## Worker Output

```text
artifacts/audit-report.md
artifacts/findings.json
artifacts/results.sarif
artifacts/checks.json
artifacts/manifest.json
```

The current deterministic checks inventory files and report security-policy,
GitHub Actions, and tracked environment-file posture. This is proof of ATP
coordination and bounded work, not a claim of comprehensive source-code
vulnerability analysis.

The desktop label for the worker action is now **Run Audit Skill**. In this
developer preview, that command records deterministic local artifacts and
receipt-backed audit labor objects. OpenClaw/Hermes execution, model/tool logs,
and web/API-only repository reads are the next runtime adapter, not a hidden
capability in this profile.

## Receipt

The receipt profile is
`cyphes.repository-security-audit-receipt/0.1`.

It binds:

- transaction and accepted contract hash;
- exact repository and scope;
- exercised leases and resources;
- artifact path, media type, hash, and size;
- requester approval;
- zero-value settlement;
- `SETTLE` event root;
- worker Ed25519 signature.

`receiptHash` removes `receiptHash` and `signatures` before JCS hashing. Receipt
signatures remove only `signatures` before signing.

## Portable Bundle

```text
public-keys.json
envelopes.jsonl
transcript.jsonl
contract.json
leases.json
lease-access-log.jsonl
receipt.json
artifacts/
```

The committed real fixture is
`protocol/fixtures/atp-l1-repository-audit.valid/`. Artifact Two verifies
signature, hash, contract, lease, artifact, and event-chain integrity.

## Known Limits

- No hardened process/container isolation.
- No private repository capabilities.
- No live cancellation or lease revocation.
- No real payment proof.
- No broad language-specific static analysis.
- One fixed success sequence; rejection and dispute profiles remain.
