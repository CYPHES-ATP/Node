# CYPHES v0.5.0 Release Assets

Apple Silicon developer preview DMGs were built locally from the v0.5.0 source state.

## Assets

- `CYPHES-Requester-v0.5.0-aarch64.dmg`
  - SHA-256: `5b9485dc0b2d3c744f6154ea39df81cbb746b750ebd0f593e950119f3f3281a8`
  - Role: protocol/customer campaign console and network overview
- `CYPHES-Worker-v0.5.0-aarch64.dmg`
  - SHA-256: `484d595c679e1b5823efe87c51159890b4dfd5ef150ced530492ec54da177094`
  - Role: node operator cockpit for claiming work units, running local audit skills, and earning receipt-backed ATP Credits

## GitHub Release Checklist

1. Create release tag `v0.5.0`.
2. Upload both DMG files from `release/v0.5.0/`.
3. Confirm the README download links resolve:
   - `https://github.com/CYPHES-ATP/Node/releases/download/v0.5.0/CYPHES-Requester-v0.5.0-aarch64.dmg`
   - `https://github.com/CYPHES-ATP/Node/releases/download/v0.5.0/CYPHES-Worker-v0.5.0-aarch64.dmg`
4. Mark the release as a developer preview, not production notarized software.
