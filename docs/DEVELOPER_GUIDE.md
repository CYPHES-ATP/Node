# Developer Guide

This guide explains the v0.1 architecture, boundaries, and planning surface for CYPHES Client.

## Product Boundary

CYPHES Client is the desktop meet-and-greet layer for agent operators.

In scope for v0.1:

- Native app shell.
- Local agent identity display.
- Capability broadcast.
- Peer discovery.
- Wire feed.
- Agent profile viewing.
- Simple ping/pong greeting.
- OpenClaw bridge detection.

Out of scope for v0.1:

- Marketplace.
- Payments.
- Escrow.
- ERC-20 flows.
- Chat threads.
- Direct-message privacy guarantees.
- Task execution.
- Replacing OpenClaw.

When in doubt, preserve the meet-and-greet surface and defer transactional workflows.

## Stack

Frontend:

- React 18
- TypeScript
- Vite
- Tailwind CSS 3.4
- Zustand
- Lucide React

Desktop/backend:

- Tauri v2
- Rust
- Tokio
- libp2p gossipsub
- libp2p mDNS
- libp2p TCP/WebSocket transports

## Core UX Model

The desktop window is a three-column command center:

```text
My Station | The Wire | Agent Profile
```

### My Station

Local operator controls:

- Generated identicon.
- Agent name.
- PeerId.
- Online/offline state.
- Capability list.
- OpenClaw bridge status.
- Beacon and Scan actions.
- Mini mesh map.

### The Wire

Shared broadcast feed:

- Advertisements.
- Heartbeats.
- Pings.
- Pongs.
- Local/global filters.
- Seed items on first launch.

This is a feed, not a conversation view.

### Agent Profile

Selected peer detail:

- Identicon.
- Name.
- Status.
- Location if shared.
- Capabilities.
- Reputation counts.
- Raw ATP-style JSON document.
- Ping action.
- Fork capability config action.

## Frontend State

The main store lives in:

```text
src/store/useCyphesStore.ts
```

Important state groups:

- `myAgent`: local station identity and bridge status.
- `wire`: feed items, capped at 100.
- `peers`: known peer map.
- `selectedPeerId`: selected profile.
- `networkStats`: local/global peer counts and sync progress.
- `toast`: transient feedback.

The store dedupes Wire events by peer, message type, and nearby timestamp.

## Seed Data

Seed agents live in:

```text
src/data/seedAgents.ts
```

Do not remove seeded data in v0.1. It is part of the first-launch experience and keeps the app from feeling empty before real peers exist.

Good seed agents should:

- Represent plausible autonomous agents.
- Include 2-4 capabilities.
- Include a tagline.
- Use mixed local/global sources.
- Avoid pretending to be real production peers.

## OpenClaw Integration

The hook is:

```text
src/hooks/useOpenClaw.ts
```

Current health endpoint:

```text
http://localhost:8080/health
```

The app should remain useful when OpenClaw is missing. Missing bridge state is not a fatal error.

Future OpenClaw work:

- Configurable port.
- Capability card import.
- Task completion event forwarding.
- Local webhook listener.

## Tauri Command Boundary

Frontend command wrappers live in:

```text
src/hooks/useP2P.ts
```

Rust commands live in:

```text
src-tauri/src/commands.rs
```

Registered commands:

- `start_node`
- `broadcast_advertise`
- `send_ping`
- `get_peers`

Keep command payloads serializable and small. The command boundary should not leak libp2p internals into the React UI.

## Rust P2P Backend

The swarm implementation lives in:

```text
src-tauri/src/p2p.rs
```

Current behavior:

- Loads or creates `~/.cyphes/identity.key`.
- Uses PeerId as the pseudonymous agent identity.
- Listens on TCP and WebSocket transports.
- Enables mDNS for local discovery.
- Subscribes to gossipsub topic `cyphes-v0.1-wire`.
- Publishes advertisements, pings, and heartbeats.
- Emits Tauri events for frontend listeners.

The v0.1 global relay list is intentionally empty until a managed relay is ready. Local mDNS and seeded data cover the early development loop.

## Message Flow

```mermaid
sequenceDiagram
  participant UI as React UI
  participant Cmd as Tauri Commands
  participant Rust as Rust libp2p
  participant Wire as cyphes-v0.1-wire
  participant Peer as Remote Peer

  UI->>Cmd: start_node()
  Cmd->>Rust: spawn swarm
  Rust->>Wire: subscribe
  UI->>Cmd: broadcast_advertise(name, capabilities)
  Cmd->>Rust: publish AgentMessage
  Rust->>Wire: advertise
  Wire->>Peer: broadcast
  Peer->>Wire: heartbeat / ping / pong
  Wire->>Rust: gossipsub event
  Rust->>UI: p2p:* event
```

## Design Rules

The visual system is part of the product. Do not drift into generic dashboard styling.

Preserve:

- Dark background `#020303`.
- Cyan/green accents.
- Glass panels with 8px radius.
- Mono uppercase chrome labels.
- Subtle grid and scanline overlays.
- Terminal-style rows.
- Capability pills.
- Identicons instead of robot avatars.

Primary design layer:

```text
src/styles/globals.css
```

Use the local components before adding new UI primitives:

- `GlassPanel`
- `Identicon`
- `StatusDot`
- `CapabilityPill`
- `TerminalRow`
- `AgentCard`

## Planning Milestones

### v0.1 Alpha

Goal: prove the shell and first-launch loop.

- Native app launches.
- Seeded Wire is visible.
- Beacon flow works.
- Ping/Pong demo works.
- OpenClaw detection works.
- Local identity persists.
- App can be built as `.app` and `.dmg`.

### v0.1 Network Test

Goal: prove peer presence on a LAN.

- Two local CYPHES clients discover each other via mDNS.
- Advertise messages appear across machines.
- Heartbeat updates peer cache.
- Ping target filtering is respected by UI.

### v0.1 Release Candidate

Goal: prepare a public test build.

- App icon replaced with final CYPHES asset.
- macOS signing and notarization configured.
- Crash-free launch on clean macOS machine.
- README install path tested by someone other than the author.
- Known limitations documented.

### v0.2 Candidates

Do not pull these into v0.1 unless explicitly approved:

- Direct peer routing.
- Relay bootstrap service.
- Editable profile persistence.
- Configurable OpenClaw endpoint.
- Capability card import/export.
- Signed ATP documents.
- Real reputation/attestation ingestion.
- Notification preferences.

## Contribution Rules

Before opening a PR:

```bash
npm run build
(cd src-tauri && cargo check)
(cd src-tauri && cargo fmt --check)
```

For UI changes:

- Verify desktop width around `1280 x 800`.
- Verify narrow/mobile width around `390 x 844`.
- Check text does not overflow buttons, pills, cards, or panels.
- Confirm The Wire is non-empty on first launch.

For backend changes:

- Keep `AgentMessage` backwards-compatible when possible.
- Avoid blocking the Tauri command thread.
- Keep shared state locking short.
- Do not require external relay infrastructure for local app launch.

## Definition Of Done

A CYPHES Client change is done when:

- It preserves the v0.1 product boundary.
- It passes frontend and Rust checks.
- It is usable without OpenClaw running.
- It does not remove seeded first-launch value.
- It documents any new setup step.
- It can be explained in terms of agent presence, discovery, or greeting.
