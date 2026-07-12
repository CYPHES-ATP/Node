# Self-Hosting Security

CYPHES is designed for verifier-first participation. A node can join the
network, sync signed receipts, and verify independent work without starting
local model execution. Worker mode begins only when the operator selects a local
LM Studio or Ollama model and presses **Contribute**.

## Current Boundary

The current repository-audit worker is intentionally bounded:

- it reads public, pinned GitHub source archives;
- it rejects oversized archives, unsafe paths, links, and excessive file
  counts;
- it executes local model calls through LM Studio or Ollama;
- it signs Cognition Proof artifacts and waits for independent verifier
  acceptance before ATP Credits are earned;
- it does not intentionally execute repository code during the standard audit
  path.

This is still untrusted-input processing. A node operator is ingesting arbitrary
source archives, prompts, model output, verifier packets, and future proof
artifacts. Until hardened worker sandboxing ships, serious operators should not
run worker mode on a primary wallet, developer, signing, or production machine.

## Recommended Self-Hosted Modes

### Safest: Verifier-Only Node

Use this mode for security teams, partners, and public testnet participants who
only want to help settlement:

1. Install CYPHES.
2. Join the network.
3. Do not select a model.
4. Do not press **Contribute**.

The node can still sync, verify, and settle eligible independent receipts.

### Recommended Worker Mode: Dedicated Host Or VM

For worker participation, run CYPHES on an isolated machine or VM:

- separate OS account or separate VM;
- no browser wallet extensions;
- no private keys, seed phrases, exchange sessions, production API keys, or
  customer secrets;
- no mounted developer home directory;
- no write access to source repositories outside the CYPHES data directory;
- local model endpoint limited to the worker environment;
- OS firewall enabled with only expected CYPHES/model traffic allowed;
- regular snapshots so the worker can be reset after testing.

Recommended data separation:

```bash
mkdir -p "$HOME/.cyphes-worker"
CYPHES_DATA_DIR="$HOME/.cyphes-worker" npm run tauri dev
```

For packaged builds, prefer running the app in the dedicated OS account or VM so
the default `~/.cyphes` directory is already isolated from personal data.

### Container-Level Control Objective

The security target for the next worker-hardening release is a headless worker
container or equivalent OS sandbox with:

- non-root runtime user;
- read-only application image;
- dedicated writable CYPHES data volume;
- no host home-directory mount;
- no Docker socket mount;
- CPU, memory, process, and disk quotas;
- egress policy limited to the CYPHES relay/source gateway and the configured
  local model endpoint;
- explicit artifact export directory;
- reproducible image digest for operators and auditors.

Until that containerized/headless worker path is implemented and tested, the
project should describe self-hosted worker mode as appropriate for testnet,
dedicated hosts, VMs, and controlled research environments.

## Do Not

- Do not run worker mode on a machine holding wallets or production signing
  keys.
- Do not mount private source trees or secrets into a worker environment.
- Do not put a shared GitHub token into a public binary or container image.
- Do not treat an accepted model-written claim as a confirmed exploit without
  concrete evidence and, for exploit-class findings, reproduction.

## Buyer-Facing Summary

CYPHES can be evaluated today as a verifier-first network and as an isolated
worker testnet. Enterprise self-hosted worker deployments should use dedicated
hosts or VMs now, and move to the hardened headless worker container once that
release lands.
