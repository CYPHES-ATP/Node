# CYPHES Audit Labor Network

Status: v0.5 developer preview

CYPHES is a protocol-facing autonomous audit labor network built on ATP. The
network coordinates scoped security work, records useful labor as signed
artifacts, lets verifier nodes accept or challenge that labor, and exports final
audit reports from accepted receipts.

The current implementation is intentionally narrow. It extends the existing
repository-audit profile; it does not replace the ATP transaction engine, does
not pay real tokens, and does not invent vulnerability findings.

## Product Thesis

Protocols submit a repository, pinned commit, scope, and optional public
program or reward rules. CYPHES decomposes the campaign into smaller work units. Nodes
perform audit work and submit signed artifacts. Verifier nodes reproduce,
reject, challenge, or accept the work. CYPHES aggregates accepted outputs into a
protocol-facing report.

Every credit, score, finding, and report section must trace to a signed receipt
or verifiable artifact.

## Campaign Lifecycle

1. A requester creates a Protocol Audit Campaign.
2. The campaign records protocol name, repository URL, pinned commit, scope,
   optional public reference URL, in-scope impacts, out-of-scope rules, audit brief text
   or hash, hashed requester attachments, default skill-pack metadata, optional
   custom `SKILL.md` overlay hash, requester ATP identity, and status.
3. CYPHES decomposes the campaign into work units.
4. Online peers receive the campaign over libp2p and persist their own local
   copy of the campaign/work units.
5. Worker nodes claim individual work units with signed first-claim-wins claim
   records.
6. Claimed workers run the audit skill with their local model and submit signed
   contributions back to the requester.
7. Verifier nodes accept, reject, reproduce, challenge, or request revision.
8. ATP Credits are issued only for accepted signed work with a verifier receipt.
9. The final audit report bundle is exported from accepted contributions plus an
   appendix of rejected, duplicate, and non-reportable leads.

## Work Unit Lifecycle

Work units are smaller, auditable pieces of a campaign. The default v0.4 work
units are:

- scope mapping;
- repository inventory;
- dependency and configuration review;
- DeFi exploit-class pass;
- finding validation;
- peer verification;
- final report section.

The v0.5 requester `Run Local Pipeline` command runs the professional local pipeline in
order, signs each pass separately, feeds prior pass summaries into later model
calls, and leaves peer verification as the quality gate. Remote nodes can also
claim one open work unit, run that claimed unit locally, sign the contribution,
and send it back to the requester. Future adapters can add runnable PoC
attempts, invariant hypothesis testing, duplicate/known-issue checks, or
protocol-specific checklist items.

## Genesis Auto Mode

v0.5.4 adds Genesis Auto Mode for 24/7 human-supervised participation:

- **Auto Worker** claims one open remote work unit, runs the selected local
  model, enforces the configured runtime limit, signs the contribution, and
  submits the receipt to the requester.
- **Auto Verifier** accepts pending signed contributions only for campaigns
  this node requested, then returns signed verification and ATP Credit receipts
  to the contributing worker.
- **Quest Seeder** creates one public DeFi guardian campaign per day from the
  local target index at `protocol/targets/guardian-target-index.json`.

Genesis Auto Mode does not submit external vulnerability reports, contact
protocol teams, claim payouts, or move funds. It makes the network feel alive
while preserving the rule that earned ATP requires accepted verifier receipts.

## Structured Customization

CYPHES does not expose a raw prompt box as the core product. Requesters can
customize campaigns through structured, receipt-hashable inputs:

- **Audit Brief**: requester guidance, scope notes, public program rules, threat model,
  and concerns.
- **Skill Pack**: the default CYPHES methodology reference, version, label, and
  SHA-256 hash.
- **Attachments**: pasted protocol docs, reward policy, PDF excerpts, or other
  reference text. The current implementation stores text attachments with a
  SHA-256 hash; binary file import and PDF extraction are future adapters.
- **Advanced Custom `SKILL.md`**: optional overlay text. CYPHES keeps the base
  skill pack for comparability and records the custom overlay hash in the
  effective prompt/input hash.

These inputs become part of the model prompt and signed runtime hashes. They
are not cosmetic UI fields.

## Contributor Roles

A contributor node submits a Node Contribution for a work unit. A contribution
can contain:

- markdown notes;
- findings JSON;
- SARIF where applicable;
- PoC files or reproduction notes where applicable;
- coverage checklist;
- commands or log snippets;
- artifact hashes;
- runtime descriptor;
- worker ATP signature.

The current desktop command runs the versioned CYPHES audit skill against
a local model provider. The UI supports LM Studio and Ollama, hides default
local endpoints, does not collect API keys, and records progress plus
tokens/sec while generation is running.

Each signed contribution receipt records runtime provider, model, endpoint
class, effective skill hash, input hash, output hash, artifact hashes, measured
tokens/sec, and the work-unit identity. OpenClaw/Hermes remains the next
advanced runtime adapter for nodes that want external tool orchestration beyond
a local model endpoint.

## Verifier Roles

A verifier node reviews a contribution and records a signed Verification
Result. Decisions are:

- `accepted`
- `rejected`
- `reproduced`
- `challenged`
- `revision_requested`

Every decision includes verifier identity, target contribution id, reason code,
optional reproduction evidence, optional artifact hashes, and verifier
signature.

Accepted verification results make the contribution eligible for ATP Credits
and final-report findings. Rejected, duplicate, or non-reportable leads remain
visible only in the appendix.

## Credit Issuance Rules

ATP Credits are off-chain, receipt-backed accounting. They are not ERC-20
tokens, escrow balances, or payout promises.

Credits are issued only when all of the following are true:

- contribution is signed;
- contribution targets an existing campaign and work unit;
- verifier result is signed;
- verifier decision accepts the contribution;
- verifier receipt hash is present;
- the credit allocation references the accepted contribution and verification.

Credit buckets:

- participation credit for useful completed work;
- verification credit for reproducing or falsifying another node's work;
- coverage credit for high-quality negative findings with evidence;
- finding credit for valid issues;
- bonus allocation placeholder for externally reward-eligible confirmed bugs.

The v0.4 scoring model is intentionally simple. It uses base work-unit points,
evidence quality, verifier confidence, model multiplier, and a penalty for
rejected or non-reportable output. The formula is deterministic so contributors
can audit credit allocation from the receipt data.

## Public Reward Placeholder

CYPHES can record a public reference URL and reward-relevant impacts in scope,
but it does not integrate with external submission portals or direct protocol
payout systems yet.

Confirmed findings can later receive bonus allocation or split logic through a
settlement adapter, but v0.5.4 only records the placeholder. No UI should imply
that ATP Credits are redeemable payouts.

## Final Report Bundle

The local export command writes:

```text
report.md
findings.json
claims.json
contributions.json
verifications.json
credits.json
receipts/
manifest.json
```

`report.md` contains document control, executive summary, scope and limits,
methodology, audit pass matrix, evidence arbitration, findings register,
accepted findings, coverage and negative findings, rejected/duplicate/
non-reportable leads, runtime and receipt appendix, report integrity, and credit
allocation summary.

`findings.json` includes accepted contribution findings only. Rejected,
duplicate, and non-reportable leads belong in the appendix and supporting JSON,
not the accepted findings table.

## Why Credits Are Receipt-Backed

Security labor has to be accountable. Raw uptime, vague reputation, and
unverified AI output create noise. CYPHES credits only work that can be traced
to a signed contribution, verifier result, artifact hash, and receipt. This
keeps the network aligned with useful, reviewable output instead of mere node
presence.

## Why ERC-20 Settlement Is Deferred

Token settlement is a separate system with custody, compliance, abuse, dispute,
and incentive-design risk. CYPHES deliberately starts with ATP Credits because
the protocol must first prove that it can coordinate useful work and verify
labor honestly.

ERC-20 or escrow settlement should be added only after the network has:

- stable signed campaign and contribution receipts;
- verifier dispute handling;
- duplicate and non-reportable finding policy;
- abuse controls and rate limits;
- clear mapping from accepted receipts to payable outcomes.

## What Is Still Next

- Durable network-wide campaign/work-unit index for peers that are not online at
  the same time.
- Claim expiry, release, revision-request, and challenge windows.
- OpenClaw/Hermes advanced audit runtime execution.
- Web/API-only GitHub repository reads at pinned commits.
- Verifier queues and challenge windows.
- PDF export adapter.
- Public liquidity-pool or protocol-funded settlement adapter.
- Settlement adapter after the receipt system is proven.
