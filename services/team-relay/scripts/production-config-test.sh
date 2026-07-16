#!/usr/bin/env bash
set -euo pipefail

fail() {
  printf 'relay production config smoke failed: %s\n' "$*" >&2
  exit 1
}

for command in docker openssl curl jq; do
  command -v "$command" >/dev/null 2>&1 || fail "$command is required"
done
docker compose version >/dev/null 2>&1 || fail "docker compose v2 is required"

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
compose_file="$root/docker-compose.production.yml"
temporary="$(mktemp -d "${TMPDIR:-/tmp}/cnshell-relay-production.XXXXXX")"
run_suffix="${GITHUB_RUN_ID:-local}-${GITHUB_RUN_ATTEMPT:-1}-$$"
run_suffix="$(printf '%s' "$run_suffix" | tr '[:upper:]' '[:lower:]' | tr -cd 'a-z0-9_-')"
project_name="cnshell-relay-production-$run_suffix"
port_base=$((22000 + ($$ % 10000)))

export CNSHELL_RELAY_PUBLIC_HOST=localhost
export CNSHELL_RELAY_HTTP_BIND=127.0.0.1
export CNSHELL_RELAY_HTTPS_BIND=127.0.0.1
export CNSHELL_RELAY_HTTP_PORT="$port_base"
export CNSHELL_RELAY_HTTPS_PORT="$((port_base + 1))"
export CNSHELL_RELAY_SMTP_HOST=mail.invalid
export CNSHELL_RELAY_SMTP_PORT=465
export CNSHELL_RELAY_SMTP_SECURITY=tls
export CNSHELL_RELAY_SMTP_FROM='CNshell Test <relay@example.com>'
export CNSHELL_RELAY_SMTP_USERNAME=relay@example.com
export CNSHELL_RELAY_SMTP_PASSWORD_HOST_FILE="$temporary/smtp-password"
export CNSHELL_RELAY_TLS_CERT_FILE="$temporary/tls.crt"
export CNSHELL_RELAY_TLS_KEY_FILE="$temporary/tls.key"
export CNSHELL_RELAY_ALERTMANAGER_CONFIG_FILE="$temporary/alertmanager.yml"

printf '%s' 'production-config-smoke-only' > "$CNSHELL_RELAY_SMTP_PASSWORD_HOST_FILE"
printf '%s\n' \
  'route:' \
  '  receiver: blackhole' \
  '  group_by: [alertname]' \
  'receivers:' \
  '  - name: blackhole' > "$CNSHELL_RELAY_ALERTMANAGER_CONFIG_FILE"
openssl req -x509 -newkey rsa:2048 -sha256 -nodes -days 1 \
  -subj '/CN=localhost' -addext 'subjectAltName=DNS:localhost' \
  -keyout "$CNSHELL_RELAY_TLS_KEY_FILE" -out "$CNSHELL_RELAY_TLS_CERT_FILE" \
  >/dev/null 2>&1
chmod 0444 \
  "$CNSHELL_RELAY_SMTP_PASSWORD_HOST_FILE" \
  "$CNSHELL_RELAY_TLS_CERT_FILE" \
  "$CNSHELL_RELAY_TLS_KEY_FILE" \
  "$CNSHELL_RELAY_ALERTMANAGER_CONFIG_FILE"

compose=(docker compose --file "$compose_file" --project-name "$project_name")

cleanup() {
  status=$?
  trap - EXIT
  if (( status != 0 )); then
    "${compose[@]}" logs --no-color >&2 || true
  fi
  "${compose[@]}" down --volumes --remove-orphans --timeout 30 >/dev/null 2>&1 || true
  rm -rf "$temporary"
  exit "$status"
}
trap cleanup EXIT

"${compose[@]}" config --quiet
"${compose[@]}" up --detach --build

relay_id="$("${compose[@]}" ps --quiet relay)"
proxy_id="$("${compose[@]}" ps --quiet proxy)"
proxy_metrics_id="$("${compose[@]}" ps --quiet proxy-metrics)"
prometheus_id="$("${compose[@]}" ps --quiet prometheus)"
alertmanager_id="$("${compose[@]}" ps --quiet alertmanager)"
for id in "$relay_id" "$proxy_id" "$proxy_metrics_id" "$prometheus_id" "$alertmanager_id"; do
  [[ -n "$id" ]] || fail "Compose did not create every production service"
done

healthy=false
for _ in {1..180}; do
  relay_health="$(docker inspect --format '{{if .State.Health}}{{.State.Health.Status}}{{else}}missing{{end}}' "$relay_id")"
  proxy_health="$(docker inspect --format '{{if .State.Health}}{{.State.Health.Status}}{{else}}missing{{end}}' "$proxy_id")"
  prometheus_health="$(docker inspect --format '{{if .State.Health}}{{.State.Health.Status}}{{else}}missing{{end}}' "$prometheus_id")"
  services_running=true
  for id in "$relay_id" "$proxy_id" "$proxy_metrics_id" "$prometheus_id" "$alertmanager_id"; do
    [[ "$(docker inspect --format '{{.State.Status}}' "$id")" == running ]] || services_running=false
  done
  if [[ "$relay_health" == healthy && "$proxy_health" == healthy \
    && "$prometheus_health" == healthy && "$services_running" == true ]]; then
    healthy=true
    break
  fi
  sleep 1
done
[[ "$healthy" == true ]] || fail "production services did not become healthy within 180 seconds"

for id in "$relay_id" "$proxy_id" "$proxy_metrics_id" "$prometheus_id" "$alertmanager_id"; do
  container_user="$(docker inspect --format '{{.Config.User}}' "$id")"
  [[ -n "$container_user" && "$container_user" != root && "$container_user" != 0 \
    && "$container_user" != 0:* ]] \
    || fail "a production service was configured to run as root"
  [[ "$(docker inspect --format '{{.HostConfig.ReadonlyRootfs}}' "$id")" == true ]] \
    || fail "a production service did not use a read-only root filesystem"
  docker inspect --format '{{json .HostConfig.SecurityOpt}}' "$id" \
    | grep -Fq 'no-new-privileges:true' \
    || fail "a production service did not enable no-new-privileges"
  [[ "$(docker inspect --format '{{json .HostConfig.CapDrop}}' "$id")" == *'ALL'* ]] \
    || fail "a production service did not drop Linux capabilities"
done

[[ -z "$(docker port "$relay_id")" ]] || fail "relay port was published directly"
[[ -z "$(docker port "$prometheus_id")" ]] || fail "Prometheus port was published"
[[ -z "$(docker port "$alertmanager_id")" ]] || fail "Alertmanager port was published"

redirect_headers="$temporary/redirect-headers"
redirect_code="$(curl --silent --show-error --output /dev/null --dump-header "$redirect_headers" \
  --header 'Host: localhost' --write-out '%{http_code}' \
  "http://127.0.0.1:$CNSHELL_RELAY_HTTP_PORT/v1/accounts/login")"
[[ "$redirect_code" == 308 ]] || fail "HTTP did not redirect to HTTPS"
grep -Fiq 'location: https://localhost/v1/accounts/login' "$redirect_headers" \
  || fail "HTTP redirect did not use the configured public host"

wrong_host_code="$(curl --silent --output /dev/null --header 'Host: attacker.invalid' \
  --write-out '%{http_code}' "http://127.0.0.1:$CNSHELL_RELAY_HTTP_PORT/" || true)"
[[ "$wrong_host_code" == 000 ]] || fail "unknown HTTP Host was not rejected"

https_url="https://localhost:$CNSHELL_RELAY_HTTPS_PORT"
https_resolve="localhost:$CNSHELL_RELAY_HTTPS_PORT:127.0.0.1"
[[ "$(curl --insecure --silent --resolve "$https_resolve" --output /dev/null --write-out '%{http_code}' "$https_url/health")" == 404 ]] \
  || fail "public liveness endpoint was not hidden"
[[ "$(curl --insecure --silent --resolve "$https_resolve" --output /dev/null --write-out '%{http_code}' "$https_url/metrics")" == 404 ]] \
  || fail "public metrics endpoint was not hidden"

login_payload='{"email":"missing@example.com","password":"correct horse battery staple"}'
login_code="$(curl --insecure --silent --resolve "$https_resolve" --output /dev/null --write-out '%{http_code}' \
  --header 'Content-Type: application/json' --data "$login_payload" \
  "$https_url/v1/accounts/login")"
[[ "$login_code" == 401 ]] || fail "HTTPS request did not reach the relay"

limited=0
for _ in {1..10}; do
  code="$(curl --insecure --silent --resolve "$https_resolve" --output /dev/null --write-out '%{http_code}' \
    --header 'Content-Type: application/json' --data "$login_payload" \
    "$https_url/v1/accounts/login")"
  [[ "$code" == 429 ]] && limited=$((limited + 1))
done
(( limited > 0 )) || fail "authentication rate limit did not return 429"

secret_marker="cnshell-secret-$run_suffix"
curl --insecure --silent --resolve "$https_resolve" --output /dev/null \
  --header "Authorization: Bearer $secret_marker" \
  --header 'Content-Type: application/json' \
  --data "{\"marker\":\"$secret_marker\"}" \
  "$https_url/v1/workspaces/bootstrap" || true
proxy_logs="$("${compose[@]}" logs --no-color proxy)"
[[ "$proxy_logs" != *"$secret_marker"* ]] || fail "proxy logs exposed a header or request body"
[[ "$proxy_logs" != *'ERROR:'* ]] || fail "proxy entrypoint reported a configuration error"

"${compose[@]}" exec --no-TTY prometheus /bin/promtool check config /etc/prometheus/prometheus.yml
"${compose[@]}" exec --no-TTY prometheus /bin/promtool check rules /etc/prometheus/relay-alerts.yml
"${compose[@]}" exec --no-TTY alertmanager /bin/amtool check-config /etc/alertmanager/alertmanager.yml

metrics_ready=false
for _ in {1..60}; do
  relay_query="$("${compose[@]}" exec --no-TTY prometheus /bin/wget -qO- \
    'http://127.0.0.1:9090/api/v1/query?query=cnshell_relay_up' || true)"
  proxy_query="$("${compose[@]}" exec --no-TTY prometheus /bin/wget -qO- \
    'http://127.0.0.1:9090/api/v1/query?query=nginx_up' || true)"
  if jq -e 'any(.data.result[]?; .value[1] == "1")' >/dev/null <<<"$relay_query" \
    && jq -e 'any(.data.result[]?; .value[1] == "1")' >/dev/null <<<"$proxy_query"; then
    metrics_ready=true
    break
  fi
  sleep 1
done
[[ "$metrics_ready" == true ]] || fail "Prometheus did not scrape relay and proxy metrics"

printf 'relay production config smoke passed (%s, HTTPS port %s)\n' \
  "$project_name" "$CNSHELL_RELAY_HTTPS_PORT"
