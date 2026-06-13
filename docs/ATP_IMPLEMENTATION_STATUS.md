# ATP Implementation Status

Last reviewed: June 12, 2026

## Conformance Position

CYPHES Node now contains an ATP-L1 repository-audit vertical slice with
L2-style signed context leases. It completes and independently verifies one
zero-value work order, but it is not a full implementation of every ATP
profile, terminal path, settlement rail, or internet discovery mechanism.

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
| Signed rendezvous registration and automatic peer discovery | Implemented and locally smoke tested |
| Default network manifest and runtime overrides | Implemented |
| Manual direct/relay multiaddress dialing | Implemented |
| DCUtR behavior | Implemented |
| CYPHES-hosted public endpoint | Awaiting public Linux host and DNS |
| Durable public work index | Not implemented |
| AutoNAT and reachability scoring | Not implemented |
| Offline mailbox and durable retry | Not implemented |

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

A public different-network two-laptop run remains the final operational
acceptance test after the CYPHES endpoint is deployed.

## Production Exit Criteria

- Operate redundant public relay and rendezvous infrastructure.
- Run the complete transaction across independently controlled machines and
  networks in CI or a repeatable staging environment.
- Add a hardened worker sandbox and resource limits.
- Add offline delivery, retries, peer abuse controls, and key recovery.
- Complete reject, revoke, cancel, expire, and dispute paths.
- Add a funded settlement adapter before representing compensation as payable.
- Ship signed installers and an update policy.
