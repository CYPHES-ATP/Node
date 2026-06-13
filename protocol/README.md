# ATP Repository Audit Profile

This directory is the cross-implementation boundary for the first CYPHES
workload: a bounded audit of one public GitHub repository at one exact commit.

- `schemas/` contains JSON Schema 2020-12 contract and receipt definitions.
- `fixtures/repository-audit-*.json` contains canonical structural examples.
- `fixtures/atp-l1-repository-audit.valid/` is a real complete transaction
  bundle produced by CYPHES Node and accepted by Artifact Two.

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
