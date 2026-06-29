# CYPHES Source Gateway

Status: v0.6.1 MVP service

`cyphes-source-gateway` is the first network-infrastructure step for running
CYPHES nodes 24/7 without every node independently exhausting GitHub API quota.

```text
CYPHES node
-> source.cyphes.com
   -> server-side GitHub token or GitHub App installation token
   -> read-through disk cache by repo, commit, and path
   -> ETag / Last-Modified conditional requests
   -> signed source manifest headers
-> node audits pinned source context locally
```

## Endpoints

```text
GET /healthz
GET /v1/github/repository?repo=owner/repo
GET /v1/github/resolve?repo=owner/repo&ref=main
GET /v1/github/tree?repo=owner/repo&commit=<40-char-sha>
GET /v1/github/file?repo=owner/repo&commit=<40-char-sha>&path=contracts/Pool.sol
```

JSON endpoints return the GitHub-compatible JSON object with an additional
`cyphesSourceManifest` field. File responses return the raw bytes and include:

```text
x-cyphes-source-body-sha256
x-cyphes-source-public-key
x-cyphes-source-manifest
```

`x-cyphes-source-manifest` is base64url-encoded JSON signed by the gateway's
Ed25519 key.

## Auth

Fast deployment:

```bash
export CYPHES_GITHUB_TOKEN=github_pat_or_installation_token
cargo run --manifest-path source-gateway/Cargo.toml
```

GitHub App deployment:

```bash
export CYPHES_GITHUB_APP_ID=123456
export CYPHES_GITHUB_INSTALLATION_ID=987654
export CYPHES_GITHUB_PRIVATE_KEY_PATH=/secure/cyphes-github-app.pem
cargo run --manifest-path source-gateway/Cargo.toml
```

The GitHub token stays server-side. CYPHES desktop nodes do not receive it.

## Cache

Default cache directory:

```text
.cyphes-source-cache/
```

Override:

```bash
export CYPHES_SOURCE_GATEWAY_CACHE_DIR=/var/lib/cyphes/source-cache
```

Cache policy:

- repository metadata: 1 hour
- moving ref resolution: 5 minutes
- pinned commit trees and raw files: 30 days

Stale cache entries are revalidated with `If-None-Match` and
`If-Modified-Since` when GitHub supplies ETag or Last-Modified headers.

## Node Wiring

CYPHES nodes use `https://source.cyphes.com` by default. Override for local QA:

```bash
export CYPHES_SOURCE_GATEWAY_URL=http://127.0.0.1:8080
```

Disable gateway reads:

```bash
export CYPHES_DISABLE_SOURCE_GATEWAY=1
```

If the gateway is unavailable, nodes fall back to their local GitHub token or
unauthenticated GitHub reads.

