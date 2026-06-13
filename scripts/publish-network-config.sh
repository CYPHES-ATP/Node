#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 2 || $# -gt 3 ]]; then
  echo "usage: $0 <public-multiaddr-without-peer-id> <relay-peer-id> [status]" >&2
  exit 1
fi

public_addr="${1%/}"
peer_id="$2"
status="${3:-online}"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
target="$root/network/bootstrap.json"

if [[ "$public_addr" == */p2p/* ]]; then
  echo "public address must not include /p2p/PEER_ID" >&2
  exit 1
fi

full_addr="$public_addr/p2p/$peer_id"
updated_at="$(date -u +'%Y-%m-%dT%H:%M:%SZ')"
tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT

jq \
  --arg address "$full_addr" \
  --arg status "$status" \
  --arg updated_at "$updated_at" \
  '.status = $status
   | .relayAddr = $address
   | .rendezvousAddr = $address
   | .updatedAt = $updated_at' \
  "$target" >"$tmp"

mv "$tmp" "$target"
trap - EXIT
echo "Published configuration prepared at $target"
echo "$full_addr"
