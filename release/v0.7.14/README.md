# CYPHES v0.7.14

v0.7.14 is a verifier-liveness and 24/7 Guardian Loop hotfix for the current
`cyphes-dev-v0.7.7` testnet.

## Assets

- `CYPHES_0.7.14_aarch64.dmg` - macOS Apple Silicon
- `CYPHES_0.7.14_x64.dmg` - macOS Intel
- `CYPHES_0.7.14_x64-setup.exe` - Windows x64
- `SHA256SUMS.txt` - asset checksums

## Network

- ATP wire protocol: `/cyphes/atp/0.7.14`
- Rendezvous namespace: `cyphes.repository-audit.v0.7.14`
- Testnet state: continues on `cyphes-dev-v0.7.7`

## Changes

- Raises self-pending contribution backpressure from 1 to 25 receipts.
- Keeps ATP integrity unchanged: ATP is still awarded only after independent
  verifier settlement.
- Adds verifier-pull behavior for `needs_verifier` receipts during labor
  inventory sync.
- Sends dependency-complete labor bundles for repair and resync instead of
  splitting campaign, claim, contribution, and verification objects.
- Prevents silent repair/sync failures from putting an otherwise useful peer on
  cooldown.
- Expands the bundled Guardian target index to 165 public audit targets.
- Changes Guardian re-audit epochs from wall-clock timing to completion-based
  target passes, so completed target sets loop into the next epoch.

## Notes

- Nodes join as verifiers by default. Work mode still requires Run.
- macOS and Windows builds are unsigned/ad hoc developer testnet builds.
- For best testnet behavior, run at least two independent verifier-capable nodes
  on stable networks.
