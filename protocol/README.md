# ATP Repository Audit Profile

This directory contains the cross-implementation contract for the first CYPHES
workload: a read-only security audit of one public GitHub repository at one
exact commit.

The profile is intentionally narrower than the ATP specification:

- `cyphes.repository-security-audit/0.1` defines the negotiated contract.
- `cyphes.repository-security-audit-receipt/0.1` defines the terminal Proof of
  Cognition receipt.
- `schemas/` contains JSON Schema 2020-12 definitions for other languages.
- `fixtures/` contains canonical JSON examples consumed by the Rust tests.

The current Node creates and accepts the contract during `NEGOTIATE`. It does
not yet create a `ROUTE`, execute an audit, settle value, or claim that the
receipt fixture represents completed work.

See [the profile guide](../docs/REPOSITORY_AUDIT_PROFILE.md) for invariants and
the implementation sequence.
