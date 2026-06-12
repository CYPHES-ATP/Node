# Install And Test

The current distribution path is source-only. There are no signed release
binaries yet.

## Native Development

```bash
npm install
npm run tauri dev
```

The native app starts the libp2p node and discovers other CYPHES nodes on
the same LAN.

## Browser Preview

```bash
npm run dev
```

The browser preview is visual-only and read-only. It cannot sign, persist,
broadcast, or accept ATP requests.

## Two-Node Test

1. Start the native app on two computers connected to the same LAN.
2. Confirm each app reports one LAN peer.
3. On the requester, enter a public GitHub repository URL and compensation.
4. Sign and post the audit request.
5. Confirm the requester reports a peer receipt only after the second node
   receives, verifies, and commits the request.
6. Select **Offer to audit** on the worker node.
7. Confirm the requester shows the signed worker offer.
8. Select **Select worker** on the requester.
9. Confirm both nodes report the transaction as `NEGOTIATED`.

No payment is transferred during this test.

For two isolated profiles on one development machine, set
`CYPHES_DATA_DIR` for the second process so it has a distinct identity and
database.

See [JOIN_NETWORK.md](JOIN_NETWORK.md) for expected UI states and
troubleshooting.

## Build Checks

```bash
npm run build
(cd src-tauri && cargo fmt --check)
(cd src-tauri && cargo check)
(cd src-tauri && cargo test)
```
