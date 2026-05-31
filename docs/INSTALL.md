# Install And Build Guide

This guide is for developers and early testers who want to run CYPHES Client locally or produce a macOS build.

## Prerequisites

Recommended baseline:

- macOS 14+
- Apple Silicon or Intel Mac
- Node.js 20+
- npm 10+
- Rust stable
- Xcode Command Line Tools

Install Xcode Command Line Tools:

```bash
xcode-select --install
```

Install Rust:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup default stable
rustup component add rustfmt
```

Check versions:

```bash
node --version
npm --version
rustc --version
cargo --version
```

## Clone And Install

```bash
git clone https://github.com/CYPHES-ATP/Client.git
cd Client
npm install
```

## Run Modes

### Web Preview

Use this to verify the React UI quickly:

```bash
npm run dev
```

Open the printed local URL, usually:

```text
http://localhost:1420
```

The web preview exercises the UI, seeded data, store, OpenClaw health check, Beacon demo flow, and Ping/Pong demo flow. It does not run the native Rust libp2p process in the same way as the Tauri desktop app.

### Native Desktop Dev

Use this for real Tauri behavior:

```bash
npm run tauri dev
```

This launches a native frameless macOS window and starts the Rust command backend.

### Debug Bundle

Use this for a local `.app` and `.dmg` without release optimization:

```bash
npm run tauri build -- --debug
```

Expected outputs:

```text
src-tauri/target/debug/bundle/macos/CYPHES.app
src-tauri/target/debug/bundle/dmg/CYPHES_0.1.0_aarch64.dmg
```

Open the app bundle:

```bash
open src-tauri/target/debug/bundle/macos/CYPHES.app
```

### Release Bundle

```bash
npm run tauri build
```

Expected outputs land under:

```text
src-tauri/target/release/bundle/
```

Release builds intended for public distribution should be signed and notarized.

## OpenClaw Bridge

CYPHES checks:

```text
http://localhost:8080/health
```

Expected shape:

```json
{
  "agent_id": "local-agent-or-peer-id",
  "name": "OPENCLAW_LOCAL",
  "capabilities": ["web-scrape", "code-gen", "summarize"],
  "status": "online"
}
```

If OpenClaw is unavailable, CYPHES still launches with local manual station data and seeded Wire agents.

## P2P Identity

The Rust backend generates or loads:

```text
~/.cyphes/identity.key
```

That keypair determines the local libp2p PeerId. Delete this file only when you intentionally want a new pseudonymous identity.

## Validation Checklist

Run these before opening a PR:

```bash
npm run build
(cd src-tauri && cargo check)
(cd src-tauri && cargo fmt --check)
```

Manual smoke test:

1. Launch with `npm run tauri dev`.
2. Confirm seeded agents appear in The Wire.
3. Click `BEACON`.
4. Select a seeded agent.
5. Click `PING`.
6. Confirm `PONG` appears.
7. Resize the app from desktop width down toward mobile width.
8. If OpenClaw is running, confirm bridge state becomes connected.

## Troubleshooting

### Port 1420 Is Busy

Tauri expects Vite on port `1420`. Stop the existing process:

```bash
lsof -nP -iTCP:1420 -sTCP:LISTEN
kill <pid>
```

### Missing Rust Formatter

```bash
rustup component add rustfmt
```

### macOS Blocks The App

Unsigned local builds can trigger Gatekeeper warnings. For local development, prefer `npm run tauri dev`. For distribution, configure signing and notarization.

### OpenClaw Missing

This is expected if the local runtime is not running. CYPHES should still show seeded agents and allow UI testing.

### libp2p Build Takes A While

The first native build compiles Tauri, WebKit bindings, and libp2p. Subsequent builds are much faster.
