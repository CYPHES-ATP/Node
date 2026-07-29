# CYPHES v0.17.4 Mainnet Model Scoring

v0.17.4 is a non-mandatory mainnet release. It preserves the existing
`cyphes-final-testnet-v0.16.0` genesis ledger marker, the `/cyphes/atp/0.15.1`
labor wire, receipt format, and forward-only economics. No database reset is
required and old nodes stay compatible.

This folder carries the cumulative work of v0.17.1 through v0.17.4. None of the
intermediate versions were built, so v0.17.4 is the first artifact containing
any of it.

## Model scoring (v0.17.4)

- `glm-5.2` earns a `20.0x` tier on measured output: 3.75 findings per pass at
  53% unique titles and 81 tok/s across 102 passes, with 102 of 102 passes
  carrying three or more evidence-backed coverage items.
- `glm-5.1` stays at `10.0x`; a future `glm-5.3` starts at `10.0x`. Tiers are
  earned per release, not inherited by family.
- `kimi-k3` holds a reserved `50.0x` tier. It has no network data yet.
- Rejects the output-contract example when a model returns it verbatim. A 3B
  model treats the contract as a template to fill; a realistic example is more
  dangerous than an obvious one because the echo looks like genuine output.

## Headless worker (v0.17.3)

A node can run with no display, no webview, and no Tauri runtime. This is the
supported path for WSL2, Linux servers, and any host without a desktop session,
where the GUI build would previously start, fail to composite a webview, and
idle forever without connecting to the relay.

```bash
CYPHES_HEADLESS=1 CYPHES_CONTRIBUTE=0 RUST_LOG=cyphes_desktop_lib=info \
  /Applications/CYPHES.app/Contents/MacOS/cyphes-desktop
```

- `--headless` works in place of `CYPHES_HEADLESS=1`.
- Logs `[HEADLESS] node started`, then `[CONTRIBUTE]` per tick, to **stderr**.
- `RUST_LOG` works for the first time; the app previously had no logging
  framework, so the variable was inert.
- Clean SIGTERM shutdown for systemd.
- Verifier-only is the default and costs no inference. Campaign seeding is not
  yet available headless and remains a cockpit duty.

## Economic integrity (v0.17.2)

- GLM releases matched no tier rule and were credited at the `0.9x`
  unknown-model floor, the same rate as a 3B local model. Replaced the if/else
  cascade with an ordered tier table.
- Multipliers above `3.0x` on a cloud-proxied runtime are gated on measured
  throughput of at least 25 tok/s. A missing measurement fails the gate rather
  than passing it. One-sided by design: large local models are legitimately
  slower, so low throughput is never held against a local claim.
- Adds `protocol/targets/benchmark-set.v1.json`, 19 commit-pinned diff-active
  repositories with a required full SHA, so model comparison audits identical
  code on every run.

## Audit quality (v0.17.1)

- Cloud-proxied models receive a context budget matched to their window: 48
  files / 700 KB, against 16 files / 180 KB for the local tier.
- Workers follow Solidity and Vyper imports and inherited base contracts into a
  second fetch pass, so the file that settles a finding is present.
- `.vy` is selectable for the first time. Every Vyper-scoped campaign
  previously received no scoped source at all.
- Audit skill pack v0.5 adds an Exploitability Gate: before a finding may exceed
  `informational`, the worker must confirm no mitigation is already present,
  name the caller who can reach it, show a numeric bound is reachable with
  realistic values, check adjacent code for documented intent, and cite evidence
  from the file under accusation.

## Validation

- `npm run build`
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check`
- `cargo test --manifest-path src-tauri/Cargo.toml` — 101 tests pass
- Headless mode verified end to end against the live network: a node with no
  display started, connected to the relay, verified two real peer receipts at
  145 ATP each, and exited cleanly on SIGTERM.

## Assets

Linux nodes run headless from source. The Windows x64 setup build is unsigned;
verify its checksum before installing.

- `CYPHES_0.17.4_aarch64.dmg` — Apple Silicon
- `CYPHES_0.17.4_x64.dmg` — Intel
- `CYPHES_0.17.4_x64-setup.exe` — Windows x64
- `SHA256SUMS.txt`

## Checksums

```text
fc25d8cb74886b929ffd1b8472102f89bf274d1f769137c0cfcc3f8fab7a5fcd  CYPHES_0.17.4_aarch64.dmg
31c9345ea16cfa052796ba5f054995547f94cd9df0bdc6c860e4fb9a3095b724  CYPHES_0.17.4_x64.dmg
44347f5a796ff03c9158b744247dbcaa80e31f63135e54ae64b1d7478dfb0e82  CYPHES_0.17.4_x64-setup.exe
```

Verify before installing:

```bash
shasum -a 256 -c SHA256SUMS.txt
```

The macOS builds are ad hoc signed and not Apple-notarized. After installing,
Control-click the app and choose **Open**, or strip the quarantine attribute
with `xattr -dr com.apple.quarantine`. The Windows x64 setup build is unsigned.
