# CYPHES v0.16.2 Mainnet

v0.16.2 is the in-place CYPHES mainnet migration. It keeps the existing ledger
marker as the genesis ledger identifier:

```text
cyphes-final-testnet-v0.16.0
```

It does not force a fresh database. Final-testnet work, ATP allocations,
receipt trails, peer history, and proof roots continue forward. Old receipts
keep their original economics; v0.16.2 model scoring applies only to new
mainnet receipts.

## Assets

- `CYPHES_0.16.2_aarch64.dmg` - Apple Silicon macOS
- `CYPHES_0.16.2_x64.dmg` - Intel macOS
- `CYPHES_0.16.2_x64-setup.exe` - Windows x64

macOS builds are ad hoc signed and not Apple-notarized. The Windows setup build
is unsigned. Verify every download against `SHA256SUMS.txt` before
installation.

## Mainnet Genesis Archive

- Contributions: `6,068`
- Verifications: `6,015`
- ATP allocation rows: `12,030`
- ATP allocated: `636,044`
- Active submitted-pending receipts: `0`
- Signed Cognition Proof packets: `6,068`
- Worker identities observed: `3`
- Verifier identities observed: `5`
- Contribution receipt root: `5f3534827753611d7abf13785d655d84eed8fb75a42498c960622f12cb0e5f06`
- Verification receipt root: `0cefefcb7c5b45ef1855fec085db4f13f2e2614a4f801b9468f5a3e056058101`
- Cognition Proof root: `7f53352bef6312190ad6b0367e71838f224955ca860501b42c8433b96552296f`

## What Changed

- Moves release positioning from Final Testnet to Mainnet without resetting the
  database or changing the compatible `/cyphes/atp/0.15.1` labor wire.
- Adds a forward-only model scoring registry:
  `minimax-m3 = 10.0x`, `gpt-oss-20b = 3.0x`, `gpt-oss-120b = 10.0x`, and
  explicit high-tier handling for `kimi`, `qwen-max`, and frontier/cloud labels.
- Signs model/node capability cards into new runtime receipts:
  model name, declared parameter tier, provider class, context window when
  known, worker mode, and app version.
- Tightens the bounty gate so `reportable:true` requires concrete
  file/function/line, exploit path, impact, and reproduction evidence.
- Preserves parser-fallback and low-evidence penalties so generic or
  unstructured output cannot masquerade as bounty-grade evidence.

## Operator Notes

- Nodes join as verifier-only by default.
- Press **Contribute** only when a local LM Studio or Ollama model should do
  audit work.
- Press **Stop worker** to return to verifier-only operation.
- Verified ATP remains receipt-derived: signed contribution, independent signed
  verifier acceptance, and deterministic allocation.
- This release keeps the v0.16.0 ledger marker by design so all final-testnet
  receipts become mainnet genesis state.

## Verification

- `npm run build`
- `node scripts/assert-genesis-auto-mode.mjs`
- `cargo check --manifest-path src-tauri/Cargo.toml`
- `cargo test --manifest-path src-tauri/Cargo.toml`
- macOS Apple Silicon Tauri worker build
- macOS Intel Tauri worker build
- Windows x64 Tauri worker build through local `cargo-xwin`
- `codesign --verify --deep --strict --verbose=2` for both macOS apps
- `hdiutil verify` for both macOS DMGs
- `lipo -archs` verified `arm64` and `x86_64`
- `file` verified Windows PE x86-64 app and NSIS installer
- `shasum -a 256 -c SHA256SUMS.txt`

## SHA-256

```text
667cff5f406210f4767a5de47c6aba78cee330038c0136ae5de14dd640663c60  CYPHES_0.16.2_aarch64.dmg
c79b18fd69fb75e9928e69b6d92afa98636b46e76fcf97c7fe78cf5258c2fd6a  CYPHES_0.16.2_x64.dmg
f9b45fad64a169a6615a6d97901f88ccb510117bae3a064dc01b3b7e6f9348bf  CYPHES_0.16.2_x64-setup.exe
```
