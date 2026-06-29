# Autonomous Guardian Loop

Status: v0.6.2 testnet seed

The v0.6.2 main CYPHES app is autonomous by default. Users open the app,
select a local LM Studio or Ollama model, and watch CYPHES coordinate public
audit work. There are no Auto Worker, Auto Verifier, Quest Seeder, or Work
Order controls in the main node UI.

## Runtime Loop

```text
Guardian Index v2
-> resolve GitHub target to pinned commit
-> create work only if target/path/commit is not already active
-> discovered worker auto-claims open work
-> local model runs bounded audit skill
-> worker signs contribution receipt
-> requester or verifier accepts independent worker contributions
-> signed verification/credit receipt returns to worker
-> report bundles can be exported from campaign.html
```

## What Runs By Default

- **Auto Worker** claims one open remote work unit when a selected local model
  is available, runs the bounded audit skill, signs the contribution, and sends
  the receipt back to the requester.
- **Auto Verifier** accepts pending signed contributions only for campaigns
  requested by this same local identity and only when the worker is a different
  ATP identity. Self-verification can test the local loop, but it cannot mint
  Verified ATP.
- **Quest Seeder** watches `protocol/targets/guardian-target-index.json`,
  resolves targets to pinned commits, and creates a signed campaign only when
  the same target/path/commit is not already covered locally.

The runtime limit remains enforced by Rust for autonomous worker runs. If a local
model exceeds the limit, CYPHES does not create a signed contribution.

v0.6.2 raises the default autonomous caps to support long-running testnet nodes:

- **Observation cap**: 2880 Guardian target observations per day.
- **Model audit cap**: 2880 local-model work-unit runs per day.

## GitHub Backoff

The loop depends on public GitHub reads for commit resolution, tree inventory,
and scoped file context. v0.5.7 keeps shared GitHub backoff across campaign
seeding and worker context reads. If GitHub returns a rate-limit response,
CYPHES pauses GitHub reads until the reset time and surfaces that status in the
cockpit instead of continuing to hammer GitHub or creating unpinned campaigns.

v0.5.7 also caches immutable pinned GitHub tree and raw-file reads locally under
`~/.cyphes/source-cache/github/`. Repeated work against the same commit/path can
reuse cached source context instead of spending API quota again.

Nodes can increase quota by configuring a local GitHub token through
`CYPHES_GITHUB_TOKEN`, `GITHUB_TOKEN`, `~/.cyphes/github.token`, or
`githubToken` in `~/.cyphes/settings.json`. CYPHES does not ship with an
embedded network-wide GitHub token.

For public-scale 24/7 operation, v0.6.1 includes the live Source Gateway at
`source.cyphes.com` with server-side CYPHES GitHub App credentials. The
remaining gateway work is cache limits, metrics, and per-node quotas.

## Guardian Index v2

The bundled index contains 100 structured public coverage targets. Each target
includes:

- source signals: `manual-curated`, `github`, and `defillama`;
- protocol, category, chains, static TVL/risk-rank seed;
- GitHub repository URLs and focused paths;
- protocol docs/security references when known;
- in-scope and out-of-scope text;
- contract criticality and work priority score;
- credit budget and cadence.

The index is a deterministic seed for developer-preview work generation. It is
not a live bounty feed, not an affiliation claim, and not a payout guarantee.

## Anti-Spam Rule

CYPHES creates at most one active local campaign per Guardian target/path/commit.
If the commit has not changed, the node records the observation and keeps
watching instead of creating duplicate work.

v0.5.6 also enforces duplicate suppression in SQLite for the same
requester/repository/commit/scope tuple, so UI races or reconnect replay do not
create parallel local campaigns for unchanged work.

If a Guardian Index row resolves to a stale, moved, or unavailable GitHub
repository, CYPHES records the target-level failure, advances the cursor, and
quarantines that row for 24 hours. GitHub rate-limit/backoff errors are treated
separately and pause repository reads instead of cycling through the index.

## Credit Semantics

CYPHES shows two credit states:

- **Pending ATP** is provisional. It estimates useful work while a node is
  running or after a contribution has been submitted but not yet verified.
- **Verified ATP** is receipt-derived. It increases only after a signed
  verifier result from a different ATP identity accepts a signed contribution
  and issues a deterministic credit allocation.

Credits are local, off-chain, receipt-backed accounting. They are not an
ERC-20, escrow balance, payout claim, or transferable token in this release.
v0.5.7 recomputes the displayed verified total from signed contribution and
verifier records. Local SQLite edits that do not match those receipts are
ignored by the credit summary.

v0.6.2 adds a quality deduction for parser fallback output. If a local model
returns unstructured prose that cannot be parsed into the CYPHES findings and
coverage schema, the contribution can still be signed and verified, but its ATP
allocation is multiplied by 0.10. The cockpit shows this as a red telemetry
event: `ATP quality deduction: parser fallback, 0 structured findings, -90%
projected reward`.

## What It Does Not Do

The Autonomous Guardian Loop does not:

- submit vulnerability reports to external programs;
- contact protocol teams;
- claim a payout;
- move funds;
- convert ATP Credits into a token;
- mark unverified model output as a valid vulnerability.

Human approval is required before disclosure, escalation, liquidity-pool
settlement, external submission, or protocol contact.

## Current Network Limits

The live loop depends on online peer delivery. The relay/rendezvous network can
discover nodes, but CYPHES does not yet have a durable replicated work index or
offline mailbox. If a requester/verifier is offline, a worker can create a
pending signed contribution, but Verified ATP arrives only after an independent
verifier comes online and accepts the contribution.
