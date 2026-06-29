# Guardian Index

Status: v0.6.1 testnet seed

`protocol/targets/guardian-target-index.json` is the bundled work seed for the
Autonomous Guardian Loop. It gives CYPHES enough structured public targets to
run continuously without pretending to be a live bounty platform.

## Target Fields

Each target includes:

- `targetId`: stable local identifier used for duplicate detection.
- `protocolName`, `category`, `chains`: user-facing context.
- `source`: source signals such as `manual-curated`, `github`, `defillama`, or
  `protocol-owned-docs`.
- `tvlRiskRank`: static risk-rank seed used for priority. It is not a live TVL
  claim.
- `repoUrl` and `repoUrls`: public GitHub repositories CYPHES can resolve to a
  pinned commit.
- `contractPaths`: focused paths when known.
- `docsUrl` and `securityUrl`: public reference links when known.
- `inScopeText` and `outOfScopeText`: scope boundary text copied into campaign
  attachments.
- `contractCriticality`, `priorityScore`, `creditBudget`: local work-priority
  and ATP Credit estimation inputs.
- `cadence`: currently `commit-diff-watch`.

## Work Creation Rule

CYPHES resolves the GitHub repository to the current default-branch commit. It
creates work only when the same target/path/commit is not already active in the
local campaign set. If unchanged, it records the observation and moves on.
v0.5.6 also rejects duplicate local campaign persistence for the same
requester/repository/commit/scope tuple.

If GitHub rate-limits the node, CYPHES records a local backoff window and pauses
GitHub reads until the reset time. The node can keep peer networking alive while
GitHub-backed work discovery waits.

If a target points to a moved, stale, or unavailable public repository, CYPHES
records a local target failure, advances to the next target, and skips the
failed row for 24 hours. Invalid index rows should not pin the autonomous loop.

## Source Policy

The index can use public protocol docs, GitHub repos, release notes, security
pages, and public DeFi indexes as source signals. It must not claim affiliation
with a protocol or external bounty marketplace unless that relationship exists
and is documented.

Auto mode does not submit reports, contact protocols, claim rewards, or move
funds. Human approval is required before external disclosure.

## Regeneration

The current generated seed is produced by:

```bash
node scripts/generate-guardian-index.mjs
```

Regeneration is deterministic. Review changes before committing because this
file directly affects autonomous work creation.
