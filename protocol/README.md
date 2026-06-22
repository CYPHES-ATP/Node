# ATP Protocol Artifacts

This directory is the cross-implementation boundary for CYPHES ATP work. It
currently contains the original repository-audit transaction profile and the
first protocol audit labor network objects.

- `schemas/` contains JSON Schema 2020-12 profile definitions.
- `fixtures/repository-audit-*.json` contains canonical structural examples.
- `fixtures/atp-l1-repository-audit.valid/` is a real complete transaction
  bundle produced by CYPHES Node and accepted by Artifact Two.
- `fixtures/protocol-audit-campaign.valid/` demonstrates one campaign with
  work units, two signed contributions, accepted/rejected verification,
  receipt-backed ATP Credits, and an aggregated report.

The implemented sequence is:

```text
DISCOVER -> NEGOTIATE -> NEGOTIATE -> ROUTE -> SETTLE -> ATTEST
```

Runtime worker activity occurs after `ROUTE` under signed leases and is
evidenced by the signed result and terminal receipt.

Verify the portable fixture:

```bash
python3 ../../Artifact-Two/tools/verify_atp_bundle.py \
  fixtures/atp-l1-repository-audit.valid
```

See [the profile guide](../docs/REPOSITORY_AUDIT_PROFILE.md).

See [the audit labor network guide](../docs/AUDIT_LABOR_NETWORK.md) for the
campaign, contribution, verification, credit, and report lifecycle.
