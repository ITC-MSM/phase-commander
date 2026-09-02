#!/usr/bin/env bash
set -euo pipefail

default_render=${1:?usage: assert-compression-boundary.sh DEFAULT_RENDER SCALEOUT_RENDER}
scaleout_render=${2:?usage: assert-compression-boundary.sh DEFAULT_RENDER SCALEOUT_RENDER}
work_dir=$(mktemp -d)
trap 'rm -rf "$work_dir"' EXIT

extract_doc() {
  awk -v kind="$1" -v name="$2" '
    BEGIN { RS = "---"; ORS = "" }
    $0 ~ "kind: " kind "\\n" && $0 ~ "metadata:\\n  name: " name "\\n" { print }
  ' "$3"
}

extract_doc Ingress phase-server "$default_render" > "$work_dir/default-health.yaml"
extract_doc Ingress phase-server-ws "$default_render" > "$work_dir/default-ws.yaml"
extract_doc Ingress phase-server-backup "$default_render" > "$work_dir/default-backup.yaml"
test -s "$work_dir/default-health.yaml"
test -s "$work_dir/default-ws.yaml"
test -s "$work_dir/default-backup.yaml"
grep -q 'path: /health' "$work_dir/default-health.yaml"
grep -q -- '-compress@kubernetescrd' "$work_dir/default-health.yaml"
grep -q 'path: /p2p-draft-backup' "$work_dir/default-backup.yaml"
grep -q -- '-compress@kubernetescrd' "$work_dir/default-backup.yaml"
grep -q 'path: /ws' "$work_dir/default-ws.yaml"
if grep -q -- '-compress@kubernetescrd' "$work_dir/default-ws.yaml"; then
  echo 'WebSocket Ingress unexpectedly carries compression'
  exit 1
fi

assert_ingressroute_pair() {
  local name=$1
  local output="$work_dir/$2"
  extract_doc IngressRoute "$name" "$scaleout_render" > "$output"
  test -s "$output"
  awk '/^    - kind: Rule/{route++} route == 1' "$output" > "$output-ws"
  awk '/^    - kind: Rule/{route++} route == 2' "$output" > "$output-http"
  grep -q 'PathPrefix(`/ws`)' "$output-ws"
  if grep -q 'name: phase-server-compress' "$output-ws"; then
    echo "$name WebSocket route unexpectedly carries compression"
    exit 1
  fi
  grep -q 'name: phase-server-compress' "$output-http"
}

assert_ingressroute_pair phase-server scaleout-entry.yaml
assert_ingressroute_pair phase-server-0 scaleout-ordinal.yaml
