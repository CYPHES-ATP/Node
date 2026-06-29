# Proof Of Protection

Status: v0.6.2 testnet seed

CYPHES is moving toward an autonomous digital labor network. Audit is the first
use case because security work is high-value, evidence-heavy, and naturally
receipt-driven.

## Bitcoin Analogy

Bitcoin's first useful primitive was Proof of Work:

- miners spent scarce compute;
- the network could verify the work cheaply;
- the best chain created canonical ordering;
- issuance followed accepted work.

CYPHES should not copy Proof of Work. Local AI labor is not useful just because
it burns tokens. The useful primitive is **Proof of Protection**:

- a node performs scoped security labor against pinned source;
- the runtime, model, skill hash, source context, output hash, and artifacts are
  recorded;
- the worker signs the contribution;
- an independent verifier accepts or rejects the work;
- Verified ATP follows accepted receipts.

## What Exists In v0.6.2

- ATP envelopes are signed and hash-linked.
- Protocol campaigns and work units are signed objects.
- Worker contributions are signed.
- Verifier decisions are signed.
- Verified ATP is derived from independent accepted verifier receipts, not raw
  SQLite balances.
- Local SQLite tampering does not create displayed Verified ATP unless the
  signed receipt data matches deterministic allocation rules.
- `cyphes-source-gateway` gives nodes shared cached access to pinned GitHub
  source context without exposing the server-side GitHub token to desktop
  nodes.
- Gateway responses include signed source manifest headers.
- Parser-fallback model outputs earn only 10% of normal ATP allocation and show
  the deduction in red telemetry.
- Default autonomous observation and model-audit caps are 2880/day each.

That is enough to say CYPHES has a real early labor-receipt engine. It is not
just a simulated scoreboard.

## What Does Not Exist Yet

v0.6.2 does not have:

- a global canonical ATP ledger;
- on-chain settlement;
- verifier quorum;
- challenge windows;
- slashing;
- canonical reputation;
- guaranteed offline delivery;
- public liquidity pools;
- automatic external bounty submission.

Those features can be added after the network proves that nodes will stay
online, run local models, exchange work, and generate useful signed artifacts.

## MVP Incentive Rule

For now, ATP should be positioned as **Verified ATP**, not money and not a
token. Nodes earn it when independent receipts say useful work happened.

This is the minimum honest loop:

```text
source gateway pins/caches source
-> node runs local model work
-> node signs contribution
-> another identity verifies
-> Verified ATP updates from receipts
```

Tokens/sec can drive live excitement and projected ATP, but accepted receipts
must remain the boundary for earned ATP.

## Why This Can Run 24/7

The v0.6.2 network loop no longer requires every node to hammer GitHub
directly. Nodes can read through `source.cyphes.com`, receive cached pinned
source, and continue auditing locally. If the gateway is unavailable, nodes
fall back to their own GitHub token or unauthenticated GitHub reads.

This is the first credible 24/7 foundation. The next infrastructure layer is a
durable work index and offline mailbox so work keeps moving even when the
requester/verifier app is not online at the same time as the worker.
