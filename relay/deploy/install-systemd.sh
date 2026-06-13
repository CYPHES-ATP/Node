#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "usage: $0 <cyphes-relay-binary> <public-multiaddr-without-peer-id>" >&2
  echo "example: $0 target/release/cyphes-relay /dns4/relay.cyphes.com/tcp/4001" >&2
  exit 1
fi

binary="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
public_addr="$2"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

if [[ ! -x "$binary" ]]; then
  echo "relay binary is not executable: $binary" >&2
  exit 1
fi

if [[ "$public_addr" == */p2p/* ]]; then
  echo "public address must not include /p2p/PEER_ID" >&2
  exit 1
fi

if ! id -u cyphes-network >/dev/null 2>&1; then
  sudo useradd --system --home /var/lib/cyphes-network --shell /usr/sbin/nologin \
    cyphes-network
fi
sudo install -m 0755 "$binary" /usr/local/bin/cyphes-relay
sudo install -m 0644 "$root/relay/deploy/cyphes-network.service" \
  /etc/systemd/system/cyphes-network.service
printf 'CYPHES_RELAY_PUBLIC_ADDR=%s\n' "$public_addr" |
  sudo tee /etc/cyphes-network.env >/dev/null
sudo chmod 0644 /etc/cyphes-network.env
sudo systemctl daemon-reload
sudo systemctl enable --now cyphes-network.service

sleep 2
peer_id="$(
  sudo -u cyphes-network \
    CYPHES_RELAY_DATA_DIR=/var/lib/cyphes-network \
    /usr/local/bin/cyphes-relay --print-peer-id
)"

echo "CYPHES network service is running."
echo "Peer ID: $peer_id"
echo "Bootstrap address: $public_addr/p2p/$peer_id"
echo
echo "Open TCP and UDP port 4001, then run cyphes-network-smoke externally."
