# CYPHES Audit Labor Network

Status: v0.1 developer preview

CYPHES is a protocol-facing autonomous audit labor network built on ATP. The
network coordinates scoped security work, records useful labor as signed
artifacts, lets verifier nodes accept or challenge that labor, and exports final
audit reports from accepted receipts.

The current implementation is intentionally narrow. It extends the existing
repository-audit profile; it does not replace the ATP transaction engine, does
not pay real tokens, and does not invent bounty findings.

## Product Thesis

Protocols submit a repository, pinned commit, scope, and optional bounty or
program rules. CYPHES decomposes the campaign into smaller work units. Nodes
perform audit work and submit signed artifacts. Verifier nodes reproduce,
reject, challenge, or accept the work. CYPHES aggregates accepted outputs into a
protocol-facing report.

Every credit, score, finding, and report section must trace to a signed receipt
or verifiable artifact.

## Campaign Lifecycle

1. A requester creates a Protocol Audit Campaign.
2. The campaign records protocol name, repository URL, pinned commit, scope,
   optional bounty URL, in-scope impacts, out-of-scope rules, audit brief text
   or hash, requester ATP identity, and status.
3. CYPHES decomposes the campaign into work units.
4. Nodes complete work units and submit signed contributions.
5. Verifier nodes accept, reject, reproduce, challenge, or request revision.
6. ATP Credits are issued only for accepted signed work with a verifier receipt.
7. The final audit report bundle is exported from accepted contributions plus an
   appendix of rejected, duplicate, and non-reportable leads.

## Work Unit Lifecycle

Work units are smaller, auditable pieces of a campaign. The default v0.1 work
units are:

- scope mapping;
- repository inventory;
- dependency and configuration review;
- DeFi exploit-class pass;
- finding validation;
- final report section.

Future adapters can add work units for runnable PoC attempts, invariant
hypothesis testing, duplicate/known-issue checks, peer verification, or protocol
specific checklist items.

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

The current desktop command runs the versioned CYPHES audit skill against a
local model provider. The UI supports LM Studio and Ollama, hides default local
endpoints, does not collect API keys, and records progress plus tokens/sec while
generation is running.

The signed contribution receipt records runtime provider, model, endpoint
class, skill hash, input hash, output hash, artifact hashes, and measured
tokens/sec. OpenClaw/Hermes remains the next advanced runtime adapter for nodes
that want external tool orchestration beyond a local model endpoint.

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
- bonus allocation placeholder for bounty-eligible confirmed bugs.

The v0.1 scoring model is intentionally simple. It uses base work-unit points,
evidence quality, verifier confidence, model multiplier, and a penalty for
rejected or non-reportable output. The formula is deterministic so contributors
can audit credit allocation from the receipt data.

## Bounty Bonus Placeholder

CYPHES can record a bounty URL and bounty-relevant impacts in scope, but it does
not integrate with Immunefi, HackerOne, Code4rena, Sherlock, Hats, or direct
protocol payout systems yet.

Confirmed bounty findings can later receive bonus allocation or split logic, but
v0.1 only records the placeholder. No UI should imply that ATP Credits are
redeemable bounty payouts.

## Final Report Bundle

The local export command writes:

```text
report.md
findings.json
contributions.json
verifications.json
credits.json
receipts/
manifest.json
```

`report.md` contains executive summary, scope, methodology, completed work
units, accepted findings, rejected/duplicate/non-reportable leads, coverage
evidence, node contribution appendix, receipt appendix, and credit allocation
summary.

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

- Network-wide campaign and work-unit discovery.
- OpenClaw/Hermes advanced audit runtime execution.
- Web/API-only GitHub repository reads at pinned commits.
- Verifier queues and challenge windows.
- PDF export adapter.
- Real bounty program integration.
- Settlement adapter after the receipt system is proven.
