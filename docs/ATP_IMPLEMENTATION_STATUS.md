# ATP Implementation Status

Last reviewed: July 10, 2026

## Conformance Position

CYPHES Node now contains an ATP-L1 repository-audit vertical slice with
L2-style signed context leases. It completes and independently verifies one
zero-value work order, but it is not a full implementation of every ATP
profile, terminal path, settlement rail, or internet discovery mechanism.

The product layer now also includes a v0.5 audit-labor slice. Protocol audit
campaigns can be created from a pinned repository, scope, audit brief, hashed
attachments, and optional custom `SKILL.md` overlay. CYPHES decomposes campaigns
into professional work units, broadcasts campaigns to discovered peers, lets
remote nodes claim individual work units, accepts signed node contributions,
records signed verifier decisions, assigns receipt-backed ATP Credits, and
exports a professional markdown report bundle. This is online peer coordination
and local receipt accounting, not durable global indexing, token settlement, or
autonomous OpenClaw/Hermes execution yet.

v0.16.1 runs on the Final Testnet
`cyphes-final-testnet-v0.16.0` SQLite store marker and carries forward the
stable Autonomous Guardian Loop:
verifier duty runs by default, while Auto Worker and Quest Seeder stay off until
the operator presses Contribute, then persist until Stop worker is pressed. Guardian Index v2
provides 165 structured public coverage targets; CYPHES watches target commits,
avoids duplicate unchanged target/path/commit campaigns, auto-claims remote
work when work mode is enabled and a local model is selected, returns signed
verification/ATP Credit receipts to workers after independent verification,
uses dependency-complete verifier-pull bundles to repair stuck receipts, starts
new Guardian epochs after completing target passes instead of on a fixed timer,
answers labor inventory with missing-object IDs before sending full bundles,
prioritizes globally reachable peer routes over stale private routes, requires
the sparse-inventory capability before expensive labor-bundle ingest from peers,
pauses visibly when GitHub rate limits the node, and supports a local GitHub
token for higher API quota. New contributions also carry standardized Cognition
Proof packets, and verifier acceptance signs an autonomous-finality packet that
binds settlement to the contribution receipt and proof hash. v0.16.1 also
requires evidence-backed structured output or one successful repair pass before
a model run counts as a full-quality proof. v0.16.1 keeps the cheap duplicate and
superseded-object preflight before labor-bundle signature verification, so
known or already-settled contribution/verification objects are telemetered and
skipped without mutating credits, work status, or verification state. It also
releases stale local claims when signed independent verifier receipts prove a
work unit already settled, excludes superseded self-authored receipts from
worker backpressure, and raises the libp2p response read cap with byte-capped
labor bundles so reconnecting nodes can catch up without flooding peers.
Stale unverified receipts whose work unit finalized through a different
accepted contribution are reconciled into a superseded lifecycle, and
reportable bounty candidates now require concrete location, exploit path,
impact, and reproduction evidence.
The main cockpit also includes a Receipt Inspector for reviewing verified,
pending, and penalized Cognition Proof packets without trusting raw SQLite rows.
External disclosure, protocol contact, payout claims, and settlement remain
human-gated and not implemented.

v0.5.7 source preview tightens the credit trust boundary. Verified ATP now
requires an accepted verifier receipt from an ATP identity different from the
worker identity, and the displayed credit summary is derived from signed
contribution/verifier records instead of trusting raw SQLite allocation rows.
Self-verification remains useful for local QA, but it cannot mint earned ATP.
v0.5.7 also adds a local pinned-source cache for immutable GitHub tree and
raw-file reads.

v0.6.1 adds the Source Gateway MVP and live testnet seed infrastructure.
`source-gateway/` builds a standalone `cyphes-source-gateway` service with
server-side GitHub token or GitHub App installation-token support, shared
read-through disk cache, ETag/Last-Modified revalidation, signed source
manifest headers, and Docker deployment files. The CYPHES-operated
`source.cyphes.com` gateway is deployed on Fly.io with GitHub App credentials
stored server-side. Desktop nodes try the Source Gateway first and fall back to
direct GitHub reads if unavailable.

v0.6.2 raises the autonomous observation/model-audit caps to 2880/day each and
reduces parser-fallback contributions by 90% in the deterministic ATP scoring
formula. The cockpit shows the fallback deduction as a red telemetry event while
the signed artifact is still preserved for review.

The verified path is:

```text
DISCOVER -> NEGOTIATE offer -> NEGOTIATE selection -> ROUTE
         -> bounded worker activity -> SETTLE -> ATTEST
```

Worker activity is runtime behavior after `ROUTE`; `EXECUTE` is not emitted as
an ATP v0.3 wire verb.

## Verb Matrix

| ATP verb | Status | Repository-audit behavior |
| --- | --- | --- |
| `ADVERTISE` | Model only | Enum support exists; signed capability cards and indexing remain |
| `DISCOVER` | Implemented | Signed public repository request pinned to a commit |
| `NEGOTIATE` | Implemented slice | Worker offers typed contract; requester selects its canonical hash |
| `ROUTE` | Implemented slice | Requester sends signed repository-read and artifact-write leases |
| `SETTLE` | Implemented zero-value | Requester approves a verified signed worker result |
| `ATTEST` | Implemented slice | Worker signs Proof of Cognition and both nodes export a bundle |
| `REJECT` | Kernel only | Generic terminal transition; product commands and reason registry remain |
| `REVOKE` | Kernel only | Generic terminal transition; lease revocation propagation remains |

## Proof And Enforcement

| Requirement | Status |
| --- | --- |
| RFC 8785 canonical envelope signing | Implemented |
| Ed25519 identity and transport/issuer binding | Implemented |
| Explicit genesis and hash-linked `prev` chain | Implemented |
| Nonce and idempotency replay defense | Implemented |
| Expiry checks | Implemented |
| Persistent contracts, leases, results, receipts | Implemented |
| Requester lease signatures | Implemented |
| Lease TTL, operation, and namespace checks | Implemented |
| Pinned GitHub archive path safety | Implemented |
| Worker artifact hash and signature verification | Implemented |
| Receipt hash, signature, artifact, lease, contract, and event verification | Implemented in Artifact Two |
| Hardened process/container isolation | Not implemented |
| Lease attenuation, sublease, and live revocation | Not implemented |
| Complete deterministic ATP error registry | Partial |

## Network

| Capability | Status |
| --- | --- |
| TCP, WebSocket, QUIC, Noise, Yamux | Implemented |
| mDNS LAN discovery | Implemented |
| Identify and Ping | Implemented |
| Circuit Relay v2 client and reservation | Implemented and smoke tested |
| Combined deployable relay/rendezvous service | Implemented |
| Signed rendezvous registration and automatic peer discovery | Implemented and externally smoke tested |
| Default network manifest and runtime overrides | Implemented |
| Manual direct/relay multiaddress dialing | Command implemented; hidden from main v0.6.1 UI |
| DCUtR behavior | Implemented |
| CYPHES-hosted public endpoint | `relay.cyphes.com` is live on a dedicated IPv4 and externally smoke tested; redundancy pending |
| Durable public work index | Not implemented |
| AutoNAT and reachability scoring | Not implemented |
| Offline mailbox and durable retry | Not implemented |

## Audit Labor Network

| Capability | Status |
| --- | --- |
| Protocol audit campaign object | Implemented |
| Mandatory pinned commit for campaigns | Implemented |
| Audit brief, attachment hashes, and custom SKILL overlay hash | Implemented |
| Work-unit decomposition | Implemented |
| Remote campaign broadcast to online peers | Implemented |
| Signed first-claim-wins work-unit claims | Implemented |
| Claimed-worker contribution enforcement | Implemented |
| Remote claimed work-unit execution and contribution return | Implemented |
| Signed node contribution object | Implemented |
| Signed verifier result object | Implemented locally |
| ATP Credit allocation from accepted independent receipts | Implemented locally |
| Verified ATP recomputed from signed receipts instead of trusted SQLite rows | Implemented locally |
| Self-verification blocked from earned ATP issuance | Implemented locally |
| Rejected/duplicate/non-reportable lead appendix | Implemented locally |
| v0.5 local/remote audit skill execution | Implemented |
| Professional markdown report bundle export | Implemented locally |
| Autonomous Guardian Loop | Implemented locally; verifier-on by default, worker/seeder require Run |
| Guardian Index v2 with 165 structured public targets | Implemented |
| Commit-diff watch and duplicate target/path/commit suppression | Implemented locally |
| GitHub authenticated reads and rate-limit backoff | Implemented locally |
| Local pinned-source cache for GitHub tree/raw-file reads | Implemented locally |
| Source Gateway binary | Implemented |
| Server-side GitHub App installation-token minting | Implemented in Source Gateway |
| Shared read-through source cache | Implemented in Source Gateway |
| ETag and Last-Modified revalidation | Implemented in Source Gateway |
| Signed source manifest headers | Implemented in Source Gateway |
| Duplicate campaign persistence suppression | Implemented locally |
| Verification-result idempotent resend | Implemented locally |
| Stale Guardian target quarantine | Implemented locally |
| Auto Worker runtime limit | Implemented |
| LM Studio local model runtime | Implemented locally |
| Ollama local model runtime | Implemented locally |
| Runtime progress and tokens/sec events | Implemented locally |
| Effective skill hash in contribution runtime | Implemented locally |
| Durable network-wide campaign/work index | Not implemented |
| Deployed `source.cyphes.com` testnet seed service | Implemented |
| Source manifest hash embedded directly in contribution receipts | Not implemented |
| Per-node Source Gateway quota keyed by ATP identity | Not implemented |
| OpenClaw/Hermes runtime adapter | Not implemented |
| External report submission or payout claim | Not implemented |
| ERC-20, ERC-8004, or escrow settlement | Intentionally deferred |

## Verified Evidence

The committed bundle under
`protocol/fixtures/atp-l1-repository-audit.valid/` verifies with Artifact Two.
It binds the pinned `octocat/Hello-World` source, two ATP identities, leases,
five artifacts, requester approval, event root, and worker receipt signature.

The live network and protocol assertions are intentionally separate:

1. the transaction test completes the real pinned-repository work order and
   exports a valid bundle;
2. the network smoke client proves two fresh nodes reserve circuits, register
   signed peer records, discover each other, and connect automatically;
3. the relay smoke client independently proves a reservation with a 64 MiB,
   ten-minute circuit limit;
4. the desktop command path exposes every transaction operation to two
   independently running clients.

A complete work-order run between two independently controlled consumer
networks remains the next operational acceptance test.

## Production Exit Criteria

- Operate redundant public relay and rendezvous infrastructure.
- Run the complete transaction across independently controlled machines and
  networks in CI or a repeatable staging environment.
- Add a hardened worker sandbox and resource limits.
- Add offline delivery, retries, peer abuse controls, and key recovery.
- Complete reject, revoke, cancel, expire, and dispute paths.
- Add a funded settlement adapter before representing compensation as payable.
- Add Apple notarization, Windows/Linux packages, and an update policy.
