# CYPHES v0.17.8 Mainnet Dependency Queue Recovery

v0.17.8 is a non-mandatory mainnet release. It preserves the existing
`cyphes-final-testnet-v0.16.0` genesis ledger marker, the `/cyphes/atp/0.15.1`
labor wire, receipt format, and forward-only economics. No database reset is
required and older nodes stay compatible.

This release fixes a local queue pathology. It changes no consensus state, mints
no credit, and rewrites no history.

## The problem

A node that receives a verification for a contribution it never received holds
an object whose dependency cannot be resolved. Relay circuits are capped at 10
minutes and 64 MiB by design, so a contribution bundle can be lost mid-transfer
while its verification arrives intact — the condition is expected, not
exceptional.

`needs_dependency` had no exit other than success. There was no terminal state
for "this dependency is never arriving", and `attempts` was never used as a
give-up bound. Retry backoff also saturated at 256 seconds after only eight
failures and then held that cadence permanently.

Measured on a live node before the fix:

- 3,637 queued objects, 8,156,429 total retry attempts
- 1,995 verification objects in `needs_dependency`, **every one** referencing a
  contribution the node had never received
- Oldest 26 days, newest minutes old, so the queue was still growing
- Worst single object at 72,200 attempts
- Roughly 8 dependency requests per second in aggregate, indefinitely, none of
  which could ever succeed

The same objects explain two downstream symptoms: settlement rescue reporting no
capable verifier, because the node was asking peers for contributions no peer
still held, and elevated relay reservation churn from carrying that traffic.

## Fixes

- Adds a terminal `abandoned` status. Objects past 500 attempts or 7 days in
  `needs_dependency` are retired and no longer selected for retry.
- Rows are kept rather than deleted. An abandoned object is evidence that a
  receipt was seen and its dependency never arrived, which is worth being able
  to inspect.
- Abandonment is reversible. If the dependency later arrives, re-recording the
  object returns it to `needs_dependency` with a fresh attempt count.
- Raises the retry backoff ceiling from 256 seconds to 4 hours, and the exponent
  from 8 to 14. Four minutes was far too aggressive for a dependency that may
  need a peer to come back online.
- Retires exhausted objects at the start of each retry pass, before any retry
  budget is spent, so they no longer crowd the per-kind limit.
- Adds a `get_pending_labor_queue` command reporting queue depth by object kind
  and status, so a node spending its time on unresolvable dependencies is
  visible rather than silent.

Existing nodes converge on first run: the sweep retires the accumulated backlog
in one pass. No manual cleanup is required.

## Validation

- `npm run build`
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check`
- `cargo test --manifest-path src-tauri/Cargo.toml` — 112 tests pass

New coverage asserts that both the attempt bound and the age bound retire an
object, that retired objects are never selected for retry again, that rows are
preserved for inspection, that a later dependency revives the object, and that
the backoff ceiling is measured in hours rather than minutes.

## Assets

macOS images were cut on an Apple Silicon host, so the Windows x64 installer is
not included here — it builds on CI. v0.17.7 remains fully compatible until it
lands: this release changes only local queue hygiene, so a v0.17.7 node settles,
verifies and earns identically, it simply keeps accumulating the stuck
dependency backlog.

Linux has no prebuilt binary and is built from source: the normal cockpit on a
desktop session, headless on a server or WSL2.

- `CYPHES_0.17.8_aarch64.dmg` — Apple Silicon
- `CYPHES_0.17.8_x64.dmg` — Intel
- `SHA256SUMS.txt`

## Checksums

```text
2143091cfcc0ba53973e8963fbb2f4e1cb7eafcdba12d82a4bf6dcd5098ef940  CYPHES_0.17.8_aarch64.dmg
d6991363e8ca5ccca1cfba2f45674482bddd2921f6a2af932805f2bc0befe48e  CYPHES_0.17.8_x64.dmg
```

Verify before installing:

```bash
shasum -a 256 -c SHA256SUMS.txt
```

Both builds are ad hoc signed and not Apple-notarized. After installing,
Control-click the app and choose **Open**, or strip the quarantine attribute
with `xattr -dr com.apple.quarantine`.
