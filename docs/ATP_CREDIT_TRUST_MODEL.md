# ATP Credit Trust Model

Status: v0.6.1 source preview

CYPHES has two different trust layers. They should not be confused.

## ATP Receipts

ATP envelopes, contributions, verifier decisions, and report artifacts are
signed objects. They use Ed25519 identities, canonical JSON, SHA-256 hashes,
nonces, and local event-chain links. A node cannot forge another node's signed
work without that node's private key, and any mutation of a signed object breaks
verification.

This is the strong part of the system today.

## Verified ATP

v0.6.1 treats ATP Credits as a derived view over signed receipts, not as a
trusted SQLite balance.

Credits count as **Verified ATP** only when all of these are true:

- the worker contribution is signed;
- the verifier result is signed;
- the verifier decision accepts the contribution;
- the verifier identity is different from the worker identity;
- the credit allocation references the contribution id, verification id,
  campaign id, worker identity, and contribution receipt hash;
- the allocation matches the deterministic CYPHES scoring formula.

The UI displays only independently verified, receipt-derived credits as earned
ATP. If someone edits `~/.cyphes/atp.sqlite3` and inserts a fake allocation,
the credit summary recomputes trust from the signed contribution and verifier
records and ignores the forged row.

## Provisional ATP

Pending or projected ATP can move while a local model is running, but it is not
earned. Self-verification and single-node preview loops are useful for testing
the pipeline, but they cannot mint Verified ATP in v0.6.1.

The honest status for single-node or offline work is:

```text
Submitted, awaiting independent verifier
```

## What This Is Not Yet

Verified ATP is still not a token, escrow balance, payout claim, or globally
canonical chain balance. Each node keeps local SQLite state. CYPHES can verify
receipt integrity and reject obvious local tampering, but there is not yet a
distributed consensus mechanism or on-chain settlement contract.

The next settlement milestone is a chain adapter that binds ATP identity,
receipt hashes, verifier policy, and credit issuance rules to a public state
transition. Until then, CYPHES is a verifiable audit-labor coordination network
with receipt-derived accounting.

## Source Reads And 24/7 Operation

GitHub is not the network database. v0.5.7 added a local pinned-source cache so
immutable GitHub tree and raw-file reads are reused by URL instead of repeatedly
burning API quota for the same commit/path.

v0.6.1 adds the `cyphes-source-gateway` service so many nodes can reuse shared
server-side cached source context without receiving the GitHub token.

For public-scale 24/7 operation, CYPHES nodes should read through the Source
Gateway:

```text
CYPHES nodes
-> source.cyphes.com
   -> GitHub App token
   -> read-through cache by repo, commit, and path
   -> ETag conditional requests
   -> signed source manifests
-> nodes audit pinned source context locally
```

The gateway fetches each repo/commit/path once, serves many nodes from cache,
and returns signed source manifest headers. Embedding source manifest hashes
directly in contribution receipts remains the next receipt-profile step.
