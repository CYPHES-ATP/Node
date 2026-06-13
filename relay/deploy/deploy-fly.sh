#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 || $# -gt 5 ]]; then
  echo "usage: $0 <fly-app-name> [region] [organization] [ip-family] [public-host]" >&2
  echo "example: $0 cyphes-atp-network sjc personal 4 relay.cyphes.com" >&2
  exit 1
fi

app="$1"
region="${2:-sjc}"
organization="${3:-personal}"
ip_family="${4:-4}"
public_host="${5:-${app}.fly.dev}"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
config="$root/relay/fly.toml"

case "$ip_family" in
  4)
    ip_type="v4"
    dns_protocol="dns4"
    manifest_status="online"
    ;;
  6)
    ip_type="v6"
    dns_protocol="dns6"
    manifest_status="online-ipv6-preview"
    ;;
  *)
    echo "ip-family must be 4 or 6" >&2
    exit 1
    ;;
esac

if command -v fly >/dev/null 2>&1; then
  fly_bin="$(command -v fly)"
elif command -v flyctl >/dev/null 2>&1; then
  fly_bin="$(command -v flyctl)"
elif [[ -x "$HOME/.fly/bin/flyctl" ]]; then
  fly_bin="$HOME/.fly/bin/flyctl"
else
  echo "flyctl is required: https://fly.io/docs/flyctl/install/" >&2
  exit 1
fi

for dependency in jq cargo; do
  if ! command -v "$dependency" >/dev/null 2>&1; then
    echo "$dependency is required" >&2
    exit 1
  fi
done

if ! "$fly_bin" auth whoami >/dev/null 2>&1; then
  echo "Fly.io authentication is required. Run: $fly_bin auth login" >&2
  exit 1
fi

if ! "$fly_bin" status --app "$app" >/dev/null 2>&1; then
  "$fly_bin" apps create "$app" --org "$organization" --yes
fi

volumes="$("$fly_bin" volumes list --app "$app" --json)"
if ! jq -e '.[] | select((.name // .Name) == "relay_data")' \
  <<<"$volumes" >/dev/null; then
  "$fly_bin" volumes create relay_data \
    --app "$app" \
    --region "$region" \
    --size 1 \
    --yes
fi

ips="$("$fly_bin" ips list --app "$app" --json)"
if ! jq -e \
  --arg ip_type "$ip_type" \
  '.[] | select(((.type // .Type // "") | ascii_downcase) == $ip_type)' \
  <<<"$ips" >/dev/null; then
  if [[ "$ip_family" == "4" ]]; then
    if ! "$fly_bin" ips allocate-v4 --app "$app" --yes; then
      echo "A dedicated IPv4 is required for raw libp2p TCP on Fly.io." >&2
      echo "Enable billing in the Fly dashboard, then rerun this command:" >&2
      echo "https://fly.io/dashboard" >&2
      exit 1
    fi
  else
    "$fly_bin" ips allocate-v6 --app "$app"
  fi
fi

public_addr="/${dns_protocol}/${public_host}/tcp/4001"

"$fly_bin" deploy \
  "$root/relay" \
  --app "$app" \
  --config "$config" \
  --primary-region "$region" \
  --env "CYPHES_RELAY_PUBLIC_ADDR=$public_addr" \
  --ha=false \
  --yes

machines="$("$fly_bin" machines list --app "$app" --json)"
machine_id="$(
  jq -r \
    '.[0].id // .[0].ID // empty' \
    <<<"$machines"
)"
if [[ -z "$machine_id" ]]; then
  echo "could not find the deployed relay machine" >&2
  exit 1
fi

machine_state="$(
  jq -r \
    '.[0].state // .[0].State // empty' \
    <<<"$machines"
)"
if [[ "$machine_state" != "started" ]]; then
  "$fly_bin" machine start "$machine_id" --app "$app"
fi

peer_id="$(
  "$fly_bin" machine exec "$machine_id" \
    '/usr/local/bin/cyphes-relay --print-peer-id' \
    --app "$app" |
    awk '/^12D3KooW/ { print $1; exit }'
)"

if [[ -z "$peer_id" ]]; then
  echo "could not read the deployed relay peer ID" >&2
  exit 1
fi

bootstrap_addr="${public_addr}/p2p/${peer_id}"

smoke_passed=0
for attempt in 1 2 3 4 5 6; do
  if cargo run --manifest-path "$root/relay/Cargo.toml" \
    --bin cyphes-network-smoke -- \
    "$bootstrap_addr"; then
    smoke_passed=1
    break
  fi

  if [[ "$attempt" -lt 6 ]]; then
    echo "Network smoke attempt $attempt failed; retrying in 10 seconds..." >&2
    sleep 10
  fi
done

if [[ "$smoke_passed" != "1" ]]; then
  echo "The public endpoint failed six automatic discovery attempts." >&2
  exit 1
fi

"$root/scripts/publish-network-config.sh" \
  "$public_addr" \
  "$peer_id" \
  "$manifest_status"

echo
echo "CYPHES network endpoint passed automatic discovery."
echo "Bootstrap address: $bootstrap_addr"
echo "Manifest prepared at: $root/network/bootstrap.json"
echo
echo "Commit and publish network/bootstrap.json only after reviewing this output."
