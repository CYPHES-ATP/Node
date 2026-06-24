# CYPHES v0.5.1 Release Assets

Apple Silicon developer preview DMGs were built locally from the v0.5.1 source state.

## Fix

v0.5.1 replays locally stored work-unit claims and signed contributions when peers reconnect. A Worker can submit a work unit while the Requester is offline, then deliver that signed contribution later without rerunning the audit.

## Assets

- `CYPHES-Requester-v0.5.1-aarch64.dmg`
  - SHA-256: `01cd3fbd545d480809d5db6c0e723dd964a0ce0bc9ab72a7372e1a49bdfd5d2c`
  - Role: protocol/customer campaign console and network overview
- `CYPHES-Worker-v0.5.1-aarch64.dmg`
  - SHA-256: `d8b4614647766bad701ff0f393c26034e46321a6a01154ee46e311a1007582d9`
  - Role: node operator cockpit for claiming work units, running local audit skills, and earning receipt-backed ATP Credits

## GitHub Release Checklist

1. Create release tag `v0.5.1`.
2. Upload both DMG files from `release/v0.5.1/`.
3. Confirm the README download links resolve:
   - `https://github.com/CYPHES-ATP/Node/releases/download/v0.5.1/CYPHES-Requester-v0.5.1-aarch64.dmg`
   - `https://github.com/CYPHES-ATP/Node/releases/download/v0.5.1/CYPHES-Worker-v0.5.1-aarch64.dmg`
4. Mark the release as a developer preview, not production notarized software.
