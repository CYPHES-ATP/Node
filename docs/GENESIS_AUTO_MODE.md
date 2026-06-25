# Genesis Auto Mode

Status: v0.5.4 developer preview

Genesis Auto Mode is the first 24/7 participation loop for CYPHES. It makes a
node feel alive without pretending that unverified model output is payment,
reputation, or a confirmed exploit.

## What It Does

The desktop cockpit exposes three independent toggles:

- **Auto Worker** claims one open remote work unit, runs the selected local
  model, signs the contribution, and submits the receipt back to the requester.
- **Auto Verifier** accepts pending signed contributions only for campaigns
  this node requested, then returns signed verification and ATP Credit receipts
  to the contributing worker.
- **Quest Seeder** creates one public DeFi guardian campaign per day from the
  local target index at `protocol/targets/guardian-target-index.json`.

Auto Worker has two guardrails:

- **Max daily work units** limits how many work units this node can complete in
  one UTC day.
- **Max runtime minutes** is enforced by the Rust command wrapper for auto
  worker runs. If the selected local model exceeds the limit, CYPHES does not
  create a signed contribution.

## What It Does Not Do

Genesis Auto Mode does not:

- submit vulnerability reports to external programs;
- contact protocol teams;
- claim a payout;
- move funds;
- convert ATP Credits into a token;
- mark unverified model output as a valid finding.

Human approval is required before disclosure, escalation, liquidity-pool
settlement, or external submission.

## Target Index

The local target index is intentionally small and transparent. It contains
public DeFi repositories, focused paths, scope text, audit briefs, tags, and ATP
Credit budgets.

Quest Seeder reads the target index, resolves the GitHub repository and pinned
commit, creates a signed protocol audit campaign, and broadcasts the work order
to discovered peers. It records a `Guardian target: <targetId>` marker in the
audit brief so duplicate daily seeding is easy to detect.

## Credit Semantics

CYPHES shows two credit states:

- **Pending ATP** is provisional. It estimates useful work while a node is
  running or after a contribution has been submitted but not yet verified.
- **Earned ATP** is receipt-backed. It increases only after a signed verifier
  result accepts a signed contribution and issues a credit allocation.

Auto Worker can create pending work. Auto Verifier can issue earned ATP only for
campaigns this node requested. Network-wide independent verification remains a
roadmap item.

## Network Pulse

The v0.5.4 cockpit shows:

- active visible nodes;
- open work units;
- pending ATP estimate;
- earned ATP;
- daily work progress;
- local cognition rate in tokens/sec.

Remote peers do not yet broadcast telemetry, so the cognition-rate gauge is the
local model stream. Peer telemetry, durable work indexes, and reliable offline
mailboxes are future protocol work.

## Why This Matters

Genesis Auto Mode makes CYPHES behave like an always-on guardian network while
keeping the accounting honest:

```text
campaign -> work units -> signed contributions -> verifier decisions -> credits -> report
```

The node can run continuously, participate in open DeFi coverage, and produce
portable receipts. The network can later add public liquidity-pool settlement
only after the receipt and verification layer is strong enough to deserve it.
