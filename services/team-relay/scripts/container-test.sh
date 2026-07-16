#!/usr/bin/env bash
set -euo pipefail

fail() {
  printf 'relay container smoke failed: %s\n' "$*" >&2
  exit 1
}

command -v docker >/dev/null 2>&1 || fail "docker is required"
docker compose version >/dev/null 2>&1 || fail "docker compose v2 is required"
command -v curl >/dev/null 2>&1 || fail "curl is required"

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
compose_file="$root/docker-compose.example.yml"
host_port="${CNSHELL_RELAY_HOST_PORT:-18787}"
[[ "$host_port" =~ ^[0-9]+$ ]] || fail "host port must be numeric"
(( host_port >= 1024 && host_port <= 65535 )) || fail "host port must be between 1024 and 65535"

run_suffix="${GITHUB_RUN_ID:-local}-${GITHUB_RUN_ATTEMPT:-1}-$$"
run_suffix="$(printf '%s' "$run_suffix" | tr '[:upper:]' '[:lower:]' | tr -cd 'a-z0-9_-')"
project_name="cnshell-relay-smoke-$run_suffix"
export CNSHELL_RELAY_HOST_PORT="$host_port"
export CNSHELL_RELAY_ALLOW_UNVERIFIED_ACCOUNTS=1
compose=(docker compose --file "$compose_file" --project-name "$project_name")

cleanup() {
  status=$?
  trap - EXIT
  if (( status != 0 )); then
    "${compose[@]}" logs --no-color >&2 || true
  fi
  "${compose[@]}" down --volumes --remove-orphans --timeout 30 >/dev/null 2>&1 || true
  exit "$status"
}
trap cleanup EXIT

"${compose[@]}" config --quiet
"${compose[@]}" up --detach --build
container_id="$("${compose[@]}" ps --quiet relay)"
[[ -n "$container_id" ]] || fail "compose did not create the relay container"

healthy=false
for _ in {1..120}; do
  state="$(docker inspect --format '{{.State.Status}}' "$container_id")"
  health="$(docker inspect --format '{{if .State.Health}}{{.State.Health.Status}}{{else}}missing{{end}}' "$container_id")"
  if [[ "$state" == running && "$health" == healthy ]]; then
    healthy=true
    break
  fi
  [[ "$state" != exited && "$state" != dead ]] || fail "container exited before becoming healthy"
  sleep 1
done
[[ "$healthy" == true ]] || fail "container did not become healthy within 120 seconds"

base_url="http://127.0.0.1:$host_port"
[[ "$(curl --fail --silent --show-error "$base_url/health")" == '{"status":"ok"}' ]] || fail "liveness response was unexpected"
[[ "$(curl --fail --silent --show-error "$base_url/ready")" == '{"status":"ready"}' ]] || fail "readiness response was unexpected"
metrics="$(curl --fail --silent --show-error "$base_url/metrics")"
[[ "$metrics" == *'cnshell_relay_up 1'* ]] || fail "metrics did not report process liveness"
[[ "$metrics" == *'cnshell_relay_ready 1'* ]] || fail "metrics did not report database readiness"
[[ "$metrics" == *'cnshell_relay_websocket_active 0'* ]] || fail "metrics reported unexpected active WebSockets"
[[ "$metrics" != *'workspace'* && "$metrics" != *'device'* && "$metrics" != *'room'* ]] || fail "metrics exposed tenant labels"

[[ "$(docker inspect --format '{{.Config.User}}' "$container_id")" == '10001:10001' ]] || fail "container image user was not the relay service account"
[[ "$(docker inspect --format '{{.HostConfig.ReadonlyRootfs}}' "$container_id")" == true ]] || fail "root filesystem was not read-only"
docker inspect --format '{{json .HostConfig.SecurityOpt}}' "$container_id" | grep -Fq 'no-new-privileges:true' || fail "no-new-privileges was not enabled"
[[ "$(docker port "$container_id" 8787/tcp)" == "127.0.0.1:$host_port" ]] || fail "relay port was not bound only to host loopback"
[[ "$(docker inspect --format '{{range .Mounts}}{{if eq .Destination "/data"}}{{.Type}} {{.RW}}{{end}}{{end}}' "$container_id")" == 'volume true' ]] || fail "/data was not a writable named volume"
docker inspect --format '{{json .HostConfig.Tmpfs}}' "$container_id" | grep -Fq '"/tmp"' || fail "/tmp was not mounted as tmpfs"

"${compose[@]}" exec --no-TTY relay sh -c 'test "$(id -u)" = 10001 && test "$(id -g)" = 10001'
"${compose[@]}" exec --no-TTY relay sh -c 'test -s /data/relay.sqlite && touch /data/cnshell-volume-smoke && rm /data/cnshell-volume-smoke'
"${compose[@]}" exec --no-TTY relay sh -c 'touch /tmp/cnshell-tmpfs-smoke && rm /tmp/cnshell-tmpfs-smoke'

"${compose[@]}" stop --timeout 30 relay
[[ "$(docker inspect --format '{{.State.Status}}' "$container_id")" == exited ]] || fail "container did not stop"
[[ "$(docker inspect --format '{{.State.ExitCode}}' "$container_id")" == 0 ]] || fail "relay did not exit cleanly after SIGTERM"

printf 'relay container smoke passed (%s, port %s)\n' "$project_name" "$host_port"
