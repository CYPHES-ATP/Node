# CYPHES v0.17.10 Autonomous Worker Reliability and Observability

v0.17.10 is a non-mandatory mainnet release. It preserves the existing
`cyphes-final-testnet-v0.16.0` genesis ledger marker, the `/cyphes/atp/0.15.1`
labor wire, receipt format, and forward-only economics. No database reset is
required and older nodes stay compatible.

This release changes no consensus state, mints no credit, and rewrites no
history. It fixes two operator-reported defects, both of which came down to
state that was either held only in memory or logged below the level operators
actually run.

## The problems

**A failing work unit was retried without bound.** An operator reported the
autonomous worker returning to the same campaign indefinitely: the unit would
fail, they would restart CYPHES Desktop, a different protocol would run, and
the scheduler would come straight back to the failing unit.

Three separate leaks combined:

1. The backoff map lived in process. Closing the app dropped every backoff and
   give-up decision the node had made.
2. Taking a fresh claim cleared the attempt history. Because a claim expires
   after `WORK_UNIT_CLAIM_TTL_MS` and
   `reset_work_unit_after_claim_expiry` then returns the unit to `open`, the
   attempt counter reset on every cycle.
3. There was no lifetime bound. `MAX_RUN_ATTEMPTS_PER_CLAIM` is, as its name
   says, per claim.

Campaigns are iterated newest-first and the tick runs one unit from the first
campaign with available work, so a poisoned unit near the front of that
ordering also starved older campaigns.

**An idle verifier looked exactly like a broken one.** A second operator ran a
verifier-only node, watched it clear three receipts in 36 seconds, then saw
nothing for ten hours, and asked whether that was normal. It was — but the logs
could not have said so, and could not have said otherwise either. The
verifier-only idle branch logged at `debug!`, and `p2p.rs` contained no
`info!` calls at all, so a healthy idle node and one that had silently lost its
relay produced identical output: nothing.

## Fixes

- Work unit backoff is persisted to a local `worker_work_unit_failures` table.
  The counter is split in two: `attempts` stays per-claim and still drives the
  short retry delay, while `total_attempts` survives both re-claims and
  restarts and bounds the loop.
- After six lifetime failures a work unit is abandoned **on that node only**
  and skipped by selection. It stays open for other workers, who may have a
  model or context window that succeeds where this node failed.
- Selection scans past abandoned units rather than giving up on the campaign,
  so one bad unit no longer hides the rest of a campaign's work.
- The verifier-only idle tick now reports at `info!`, throttled to once every
  five minutes, and carries the state that distinguishes idle from
  disconnected: connected peer count, relay status, rendezvous registration.
- Relay reservation transitions log at `info!`, guarded on the previous value
  so they report state changes rather than every keepalive.

The failure table is deliberately not protocol state: it is never signed, never
gossiped, and never leaves the node.

## Model Scoring Registry

`deepseek-v4.1-flash` is added at `25.0x`, matched on the exact family so
neither `deepseek-v4-flash` (17.5x, earned) nor the older DeepSeek patterns can
reach or widen it. Contributed by a node operator running the model in
production; the tier remains subject to both existing gates.

Both scoring gates are unchanged. The throughput gate assigns the `3.0x`
large-local ceiling to any cloud claim under 25 tokens/sec or missing its
measurement, and the output-quality gate caps any contribution without a
reportable finding or three evidence-backed coverage items at `1.0x`.

## Dependencies

tauri 2.11.3 to 2.11.5, tokio 1.52.3 to 1.53.1, serde 1.0.228 to 1.0.229, and
vite 8.0.16 to 8.1.3. None address a security advisory against this codebase.

## Verification

`npm run check` passes end to end: frontend build, genesis auto-mode assertion,
`cargo fmt --check`, `cargo check`, `cargo test`, and the source-gateway suite.
119 tests pass, 0 fail.
