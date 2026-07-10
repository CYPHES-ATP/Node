# CYPHES v0.16.0 Final Testnet

v0.16.0 starts the CYPHES Final Testnet with a fresh SQLite network marker:

```text
cyphes-final-testnet-v0.16.0
```

Older v0.15.x local data is preserved on disk but is not reused by this final
testnet. Nodes still keep their ATP identity key unless the operator deletes it.

## Assets

- `CYPHES_0.16.0_aarch64.dmg` - Apple Silicon macOS
- `CYPHES_0.16.0_x64.dmg` - Intel macOS
- `CYPHES_0.16.0_x64-setup.exe` - Windows x64

The Windows asset is built on a native Windows runner. Verify every download
against `SHA256SUMS.txt` before installation.

## Operator Notes

- Nodes join as verifier-only by default.
- Press **Contribute** only when a local LM Studio or Ollama model should do
  audit work.
- Press **Stop worker** to return to verifier-only operation.
- Verified ATP remains receipt-derived: signed contribution, independent signed
  verifier acceptance, and deterministic allocation.
- The main cockpit includes the Receipt Inspector for verified, pending, and
  penalized Cognition Proof packets.

These builds are ad hoc signed and not Apple-notarized. On macOS, drag the app
to Applications, then Control-click and choose **Open** the first time. The
Windows setup build is unsigned and intended for testnet use.
