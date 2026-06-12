# Security Policy

CYPHES is a developer preview. It has not received an independent
security audit and must not be used to hold or release real funds.

## Reporting

Do not open a public issue for a vulnerability that could expose signing keys,
forge ATP state, bypass replay protection, corrupt transaction history, or
cause unauthorized code execution.

Use GitHub's private vulnerability reporting for
[`CYPHES-ATP/Node`](https://github.com/CYPHES-ATP/Node/security/advisories/new).

Include:

- affected commit and operating system;
- reproduction steps;
- expected and observed behavior;
- impact;
- suggested mitigation, if known.

## Sensitive Local Data

The node stores:

```text
~/.cyphes/identity.key
~/.cyphes/atp.sqlite3
```

On Unix these files are restricted to the current user. Do not share,
publish, or commit them. `CYPHES_DATA_DIR` can isolate development identities.

## Current Trust Boundary

- Network peers are untrusted.
- GitHub repository metadata is untrusted input.
- ATP signatures prove control of a node identity, not real-world identity.
- Proposed compensation is not escrowed or transferred.
- Audit execution and receipt verification are not implemented.
