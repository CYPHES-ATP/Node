# CYPHES v0.17.6 Scope-Aware Guardian Expansion

v0.17.6 is a full, wire-compatible desktop release. It preserves the
`cyphes-final-testnet-v0.16.0` genesis ledger marker, the `/cyphes/atp/0.15.1`
labor wire, existing databases, identities, receipts, balances, and settlement
rules. No reset or migration is required, and older nodes remain compatible.

## Guardian target expansion

- Expands the bundled Guardian index from 205 to 280 targets.
- Adds 75 unique, reachable repositories linked from live public Immunefi
  program resources and deduplicated against the existing repository set.
- Keeps the 40 explicit Cantina/Immunefi additions from v0.17.5, for 115
  bounty-guided targets in total.
- Program-linked repositories are intentionally marked
  `program-linked-needs-human-confirmation`; a public repository link alone is
  not treated as proof that every contract or impact is bounty-eligible.
- Existing speculative and historical coverage remains available for human
  triage instead of being deleted from the discovery surface.

## Dated scope snapshots

- Every newly seeded campaign records a dated scope snapshot in its existing
  signed campaign attachment: bounty URL, repository and pinned commit,
  repository-scope status, contract scope, implementation status, archive
  status, review class, impact categories, and excluded assumptions.
- GitHub archive status is detected when the repository is inspected. Archived
  repositories default to `historical-archived`.
- Known legacy Compound, Euler, Liquity, Ribbon, and Yearn repositories are
  labeled historical or superseded in the target index.
- Unsupported, fee-on-transfer, rebasing, malicious-token, and privileged-role
  assumptions cannot become automatically reportable unless the evidence shows
  that the program permits the condition or the exploit creates it.
- CI/configuration hardening stays separate from smart-contract bounty leads.

## Honest reportability and abstention

- A model can create and retain a candidate lead, but cannot select its own
  `reportable` outcome.
- CYPHES promotes a lead only during the dedicated finding-validation work unit,
  after explicit current scope, in-scope impact, evidence, reproduction,
  assumptions, and review-class checks pass.
- Leads that do not pass remain visible as `needs_review` or
  `needs_reproduction`; this does not narrow the human-triage surface.
- If every explicitly requested target path is absent from the pinned source
  tree, the run records `abstained_source_absent`, emits no invented finding,
  and applies no hallucination penalty.

## Model economics

- Retains the exact-family `17.5x` tier for `deepseek-v4-flash`, including the
  live `deepseek-v4-flash:0731-cloud` identifier.
- Existing cloud-throughput and output-quality gates remain unchanged.

## Compatibility boundary

- No database schema or ledger change.
- No identity, receipt, contribution, verification, or settlement change.
- No P2P message, capability, wire-profile, or relay change.
- Auto mode remains read-only and never submits findings or contacts projects.

## Validation

- 280 unique target IDs, 115 bounty-guided targets, and 75 program-linked
  additions.
- All 75 new repository URLs passed `git ls-remote` reachability checks.
- `npm run build`
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check`
- `cargo test --manifest-path src-tauri/Cargo.toml` (105 passed, 1 ignored)

## Assets

The release includes Apple Silicon and Intel macOS DMGs, a Windows x64 NSIS
installer, and SHA-256 checksums.
