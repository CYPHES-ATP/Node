#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
artifact_two="${ARTIFACT_TWO_DIR:-"$root/../Artifact-Two"}"

output="$(
  cargo test \
    --manifest-path "$root/src-tauri/Cargo.toml" \
    completes_a_real_atp_l1_repository_transaction \
    -- --ignored --nocapture 2>&1
)"
printf '%s\n' "$output"

bundle="$(
  printf '%s\n' "$output" |
    sed -n 's/^ATP_E2E_BUNDLE=//p' |
    tail -n 1
)"

if [[ -z "$bundle" ]]; then
  echo "ATP-L1 test did not report a receipt bundle" >&2
  exit 1
fi

python3 "$artifact_two/tools/verify_atp_bundle.py" "$bundle"
