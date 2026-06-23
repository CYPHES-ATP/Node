# ATP Implementation Status

Last reviewed: June 23, 2026

## Conformance Position

CYPHES Node now contains an ATP-L1 repository-audit vertical slice with
L2-style signed context leases. It completes and independently verifies one
zero-value work order, but it is not a full implementation of every ATP
profile, terminal path, settlement rail, or internet discovery mechanism.

The product layer now also includes a local v0.4 `Run Audit Pipeline`
audit-labor slice. Protocol audit campaigns can be created from a pinned
repository and scope, decomposed into professional work units, completed by
multi-pass signed node contributions, accepted or rejected by signed verifier
decisions, assigned receipt-backed ATP Credits, and exported as a professional
markdown report bundle. This is local persistence and receipt accounting, not
global campaign discovery, token settlement, or autonomous OpenClaw/Hermes
execution yet.

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
| Manual direct/relay multiaddress dialing | Implemented |
| DCUtR behavior | Implemented |
| CYPHES-hosted public endpoint | `relay.cyphes.com` is live on a dedicated IPv4 and externally smoke tested; redundancy pending |
| Durable public work index | Not implemented |
| AutoNAT and reachability scoring | Not implemented |
| Offline mailbox and durable retry | Not implemented |

## Audit Labor Network

| Capability | Status |
| --- | --- |
| Protocol audit campaign object | Implemented locally |
| Mandatory pinned commit for campaigns | Implemented |
| Work-unit decomposition | Implemented locally |
| Signed node contribution object | Implemented locally |
| Signed verifier result object | Implemented locally |
| ATP Credit allocation from accepted receipts | Implemented locally |
| Rejected/duplicate/non-reportable lead appendix | Implemented locally |
| v0.4 multi-pass audit pipeline | Implemented locally |
| Professional markdown report bundle export | Implemented locally |
| LM Studio local model runtime | Implemented locally |
| Ollama local model runtime | Implemented locally |
| Runtime progress and tokens/sec events | Implemented locally |
| Versioned audit skill hash in contribution runtime | Implemented locally |
| Network-wide campaign/work discovery | Not implemented |
| OpenClaw/Hermes runtime adapter | Not implemented |
| Live bounty intake or payout | Not implemented |
| ERC-20 or escrow settlement | Intentionally deferred |

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
