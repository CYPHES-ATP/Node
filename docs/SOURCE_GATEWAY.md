# Source Gateway

Status: v0.6.1 source preview

`source-gateway/` contains the MVP service for `source.cyphes.com`.

```text
CYPHES nodes
-> source.cyphes.com
   -> server-side GitHub token or GitHub App installation token
   -> shared read-through cache
   -> ETag / Last-Modified conditional requests
   -> signed source manifest headers
-> nodes audit cached pinned source context locally
```

## Implemented

- standalone `cyphes-source-gateway` Rust binary;
- `/healthz`;
- `/v1/github/repository?repo=owner/repo`;
- `/v1/github/resolve?repo=owner/repo&ref=main`;
- `/v1/github/tree?repo=owner/repo&commit=<sha>`;
- `/v1/github/file?repo=owner/repo&commit=<sha>&path=<path>`;
- server-side `CYPHES_GITHUB_TOKEN` / `GITHUB_TOKEN` support;
- GitHub App installation-token minting using:
  - `CYPHES_GITHUB_APP_ID`;
  - `CYPHES_GITHUB_INSTALLATION_ID`;
  - `CYPHES_GITHUB_PRIVATE_KEY_PEM` or `CYPHES_GITHUB_PRIVATE_KEY_PATH`;
- disk cache by upstream URL, which maps to repo/ref/commit/path;
- ETag and Last-Modified revalidation;
- Ed25519 signed source manifest headers;
- Dockerfile and compose file;
- desktop-node gateway-first reads with direct GitHub fallback.

## Node Configuration

Default:

```bash
CYPHES_SOURCE_GATEWAY_URL=https://source.cyphes.com
```

Local QA:

```bash
export CYPHES_SOURCE_GATEWAY_URL=http://127.0.0.1:8080
```

Disable gateway:

```bash
export CYPHES_DISABLE_SOURCE_GATEWAY=1
```

If the gateway is down, CYPHES falls back to local GitHub token/direct reads.

## Gateway Deployment

Fast token-backed run:

```bash
export CYPHES_GITHUB_TOKEN=github_pat_or_installation_token
cargo run --manifest-path source-gateway/Cargo.toml
```

GitHub App run:

```bash
export CYPHES_GITHUB_APP_ID=123456
export CYPHES_GITHUB_INSTALLATION_ID=987654
export CYPHES_GITHUB_PRIVATE_KEY_PATH=/secure/cyphes-github-app.pem
cargo run --manifest-path source-gateway/Cargo.toml
```

Docker:

```bash
cd source-gateway
docker compose up --build
```

## Still Needed

- deploy the service at `source.cyphes.com`;
- create/install the CYPHES GitHub App;
- store secrets in the deployment environment;
- add operational logs, metrics, and cache size limits;
- include source manifest hashes directly in contribution receipts;
- add per-node quotas keyed by ATP identity.

