# Autonomous Guardian Loop

Status: v0.5.5 developer preview

The v0.5.5 main CYPHES app is autonomous by default. Users open the app,
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
-> requester auto-verifies requester-owned pending contributions
-> signed verification/credit receipt returns to worker
-> report bundles can be exported from campaign.html
```

## What Runs By Default

- **Auto Worker** claims one open remote work unit when a selected local model
  is available, runs the bounded audit skill, signs the contribution, and sends
  the receipt back to the requester.
- **Auto Verifier** accepts pending signed contributions only for campaigns
  requested by this same local identity, then returns signed verification and
  ATP Credit receipts to the contributing worker.
- **Quest Seeder** watches `protocol/targets/guardian-target-index.json`,
  resolves targets to pinned commits, and creates a signed campaign only when
  the same target/path/commit is not already covered locally.

The runtime limit remains enforced by Rust for autonomous worker runs. If a local
model exceeds the limit, CYPHES does not create a signed contribution.

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

## Credit Semantics

CYPHES shows two credit states:

- **Pending ATP** is provisional. It estimates useful work while a node is
  running or after a contribution has been submitted but not yet verified.
- **Earned ATP** is receipt-backed. It increases only after a signed verifier
  result accepts a signed contribution and issues a credit allocation.

Credits are local, off-chain, receipt-backed accounting. They are not an
ERC-20, escrow balance, payout claim, or transferable token in this release.

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
pending signed contribution, but earned ATP arrives only after the requester
comes back online and verifies the contribution.
