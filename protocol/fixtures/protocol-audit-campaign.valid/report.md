# CYPHES Protocol Audit Report

## Document Control

| Field | Value |
| --- | --- |
| Protocol | AAVE |
| Repository | `aave-dao/aave-v3-origin` |
| Pinned commit | `fd1fbd9150426ca8ace9cee45b4acf912ae84f5b` |
| Campaign | `campaign_aave_fixture_001` |
| Skill pack | `cyphes-audit-skill` `0.4` |
| Skill pack hash | `sha256:2f14a452d06b1dfc1aca03b31c6639ded5a94aea5bf4bc531eabdb7fcabb7f18` |
| Custom SKILL overlay | `none` |
| Profile | `cyphes.final-audit-report/0.1` |
| Evidence rule | Accepted CYPHES receipts only |

## Executive Summary

This fixture demonstrates the CYPHES v0.5 audit labor lifecycle. It contains two signed work-unit claims, one accepted scope-mapping contribution, one rejected duplicate lead, one verifier acceptance, one verifier rejection, and one receipt-backed ATP Credit allocation.

No accepted reportable vulnerability is present in this fixture. The report should be read as verified coverage and duplicate-lead handling, not as an external reward claim or exploit submission.

## Scope And Limits

Repository-state audit of `aave-dao/aave-v3-origin` at the pinned commit. The fixture models the lifecycle objects and report shape only; it does not claim live protocol testing, deployed-bytecode certification, external payment, or real-world exploit validity.

## Methodology

CYPHES decomposes protocol audit work into signed passes. A worker node submits artifacts for a work unit, a verifier node accepts or rejects that contribution, ATP Credits are issued only from accepted verifier receipts, and the final report includes accepted findings plus appendix-only rejected or duplicate leads.

## Audit Pass Matrix

| Pass | Status | Contributions | Accepted | Receipt evidence |
| --- | --- | ---: | ---: | --- |
| Scope mapping | `accepted` | 1 | 1 | `sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd` |
| DeFi exploit-class pass | `rejected` | 1 | 0 | `sha256:1111111111111111111111111111111111111111111111111111111111111111` |

## Work Unit Claims

| Work Unit | Worker | Claim | Status |
| --- | --- | --- | --- |
| Scope mapping | `urn:libp2p:12D3KooWWorkerAcceptedFixture` | `claim_scope_map_001` | `claimed` |
| DeFi exploit-class pass | `urn:libp2p:12D3KooWWorkerRejectedFixture` | `claim_duplicate_lead_001` | `claimed` |

## Evidence Arbitration

| Verification | Decision | Reason | Target |
| --- | --- | --- | --- |
| `verification_scope_map_accept_001` | `accepted` | COVERAGE_ACCEPTED | `contribution_scope_map_001` |
| `verification_duplicate_reject_001` | `rejected` | DUPLICATE_KNOWN_ISSUE | `contribution_duplicate_lead_001` |

## Findings Register

| ID | Severity | Title | Impact | Source |
| --- | --- | --- | --- | --- |
| none | n/a | No accepted reportable findings yet | n/a | n/a |

## Accepted Findings

No accepted reportable findings are included in this fixture.

## Coverage And Negative Findings

### Scope mapping / `contribution_scope_map_001`

Mapped active scope, impact categories, known issues, and reportability gates before deeper audit work.

| Area | Status | Evidence |
| --- | --- | --- |
| scope and reportability gate | `completed` | AAVE.pdf pages 1-3 and 10-13 process mapped. |

## Non-reportable, Rejected, Or Duplicate Leads

- `AAVE-DUP-001` / `high`: Duplicate liquidation lead (`duplicate`) from `sha256:1111111111111111111111111111111111111111111111111111111111111111`.

## Runtime And Receipt Appendix

- `contribution_scope_map_001` by `urn:libp2p:12D3KooWWorkerAcceptedFixture` using `cyphes-deterministic-fixture` / `none`. Work unit: `Scope mapping`. Receipt: `sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd`.
- `contribution_duplicate_lead_001` by `urn:libp2p:12D3KooWWorkerRejectedFixture` using `cyphes-deterministic-fixture` / `none`. Work unit: `DeFi exploit-class pass`. Receipt: `sha256:1111111111111111111111111111111111111111111111111111111111111111`.

## Credit Allocation Summary

| Receiver | Total ATP Credits | Source |
| --- | ---: | --- |
| `urn:libp2p:12D3KooWWorkerAcceptedFixture` | 35 | `sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd` |

## Report Integrity

This fixture contains two contributions, two verifier decisions, and one credit allocation. It demonstrates that rejected and duplicate leads remain appendix-only while accepted coverage receives receipt-backed ATP Credits.
