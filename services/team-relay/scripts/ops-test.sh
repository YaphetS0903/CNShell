#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REPOSITORY_ROOT="$(cd "$ROOT/../.." && pwd)"
BACKUP_SCRIPT="$ROOT/scripts/backup.sh"
RESTORE_SCRIPT="$ROOT/scripts/restore.sh"
SQLITE3_BIN="${CNSHELL_RELAY_SQLITE3_BIN:-/usr/bin/sqlite3}"

fail() {
  printf 'relay ops test: %s\n' "$*" >&2
  exit 1
}

[[ -x "$SQLITE3_BIN" ]] || fail "/usr/bin/sqlite3 or CNSHELL_RELAY_SQLITE3_BIN is required"
command -v curl >/dev/null 2>&1 || fail "curl is required"
command -v cargo >/dev/null 2>&1 || fail "cargo is required"

temporary_directory="$(mktemp -d /tmp/cnshell-relay-ops.XXXXXX)"
relay_pid=""
cleanup() {
  if [[ -n "$relay_pid" ]] && kill -0 "$relay_pid" 2>/dev/null; then
    kill -TERM "$relay_pid" 2>/dev/null || true
    wait "$relay_pid" 2>/dev/null || true
  fi
  rm -rf -- "$temporary_directory"
}
trap cleanup EXIT HUP INT TERM

expect_failure() {
  local label="$1"
  shift
  if "$@" > "$temporary_directory/expected-failure.log" 2>&1; then
    fail "$label unexpectedly succeeded"
  fi
}

database="$temporary_directory/relay.sqlite"
backup_directory="$temporary_directory/backups"
mkdir "$backup_directory"
"$SQLITE3_BIN" "$database" < "$ROOT/migrations/0001_relay.sql"
"$SQLITE3_BIN" "$database" \
  "INSERT INTO accounts(id,email,display_name,password_hash,status,created_at,updated_at) VALUES('account-1','ops@example.com','Ops','test-hash','active','2026-07-16T00:00:00Z','2026-07-16T00:00:00Z');"

expect_failure "unencrypted backup without the development flag" \
  env -u CNSHELL_RELAY_AGE_RECIPIENT -u CNSHELL_RELAY_ALLOW_PLAINTEXT_BACKUP \
  CNSHELL_RELAY_SQLITE3_BIN="$SQLITE3_BIN" "$BACKUP_SCRIPT" "$database" "$backup_directory"

ln -s "$database" "$temporary_directory/relay-link.sqlite"
expect_failure "symbolic-link database backup" \
  env CNSHELL_RELAY_ALLOW_PLAINTEXT_BACKUP=1 CNSHELL_RELAY_SQLITE3_BIN="$SQLITE3_BIN" \
  "$BACKUP_SCRIPT" "$temporary_directory/relay-link.sqlite" "$backup_directory"

quoted_backup_directory="$temporary_directory/quoted'backup"
mkdir "$quoted_backup_directory"
quoted_backup="$(env CNSHELL_RELAY_ALLOW_PLAINTEXT_BACKUP=1 \
  CNSHELL_RELAY_BACKUP_TIMESTAMP=20260716T005959Z \
  CNSHELL_RELAY_BACKUP_RETENTION_COUNT=1 CNSHELL_RELAY_SQLITE3_BIN="$SQLITE3_BIN" \
  "$BACKUP_SCRIPT" "$database" "$quoted_backup_directory")"
[[ -f "$quoted_backup" && -f "$quoted_backup.sha256" ]] || fail "quoted backup path was not handled safely"

: > "$backup_directory/cnshell-relay-not-a-timestamp.sqlite"
backup_one="$(env CNSHELL_RELAY_ALLOW_PLAINTEXT_BACKUP=1 \
  CNSHELL_RELAY_BACKUP_TIMESTAMP=20260716T010101Z \
  CNSHELL_RELAY_BACKUP_RETENTION_COUNT=2 CNSHELL_RELAY_SQLITE3_BIN="$SQLITE3_BIN" \
  "$BACKUP_SCRIPT" "$database" "$backup_directory")"
backup_two="$(env CNSHELL_RELAY_ALLOW_PLAINTEXT_BACKUP=1 \
  CNSHELL_RELAY_BACKUP_TIMESTAMP=20260716T010102Z \
  CNSHELL_RELAY_BACKUP_RETENTION_COUNT=2 CNSHELL_RELAY_SQLITE3_BIN="$SQLITE3_BIN" \
  "$BACKUP_SCRIPT" "$database" "$backup_directory")"
backup_three="$(env CNSHELL_RELAY_ALLOW_PLAINTEXT_BACKUP=1 \
  CNSHELL_RELAY_BACKUP_TIMESTAMP=20260716T010103Z \
  CNSHELL_RELAY_BACKUP_RETENTION_COUNT=2 CNSHELL_RELAY_SQLITE3_BIN="$SQLITE3_BIN" \
  "$BACKUP_SCRIPT" "$database" "$backup_directory")"

[[ ! -e "$backup_one" && ! -e "$backup_one.sha256" ]] || fail "retention did not remove the oldest matching backup"
[[ -f "$backup_two" && -f "$backup_three" ]] || fail "retention removed a current backup"
[[ -f "$backup_directory/cnshell-relay-not-a-timestamp.sqlite" ]] || fail "retention removed a non-matching file"

clock_rollback_backup="$(env CNSHELL_RELAY_ALLOW_PLAINTEXT_BACKUP=1 \
  CNSHELL_RELAY_BACKUP_TIMESTAMP=20260716T000000Z \
  CNSHELL_RELAY_BACKUP_RETENTION_COUNT=2 CNSHELL_RELAY_SQLITE3_BIN="$SQLITE3_BIN" \
  "$BACKUP_SCRIPT" "$database" "$backup_directory")"
[[ -f "$clock_rollback_backup" && -f "$backup_three" ]] || fail "retention deleted the current clock-rollback backup"
[[ ! -e "$backup_two" ]] || fail "clock-rollback retention kept too many matching backups"

restored_database="$temporary_directory/restored.sqlite"
expect_failure "restore without stopped-service confirmation" \
  env CNSHELL_RELAY_ALLOW_PLAINTEXT_BACKUP=1 CNSHELL_RELAY_SQLITE3_BIN="$SQLITE3_BIN" \
  "$RESTORE_SCRIPT" "$backup_three" "$restored_database"

env CNSHELL_RELAY_ALLOW_PLAINTEXT_BACKUP=1 CNSHELL_RELAY_CONFIRM_SERVICE_STOPPED=1 \
  CNSHELL_RELAY_SQLITE3_BIN="$SQLITE3_BIN" \
  "$RESTORE_SCRIPT" "$backup_three" "$restored_database" >/dev/null
[[ "$("$SQLITE3_BIN" "$restored_database" "SELECT email FROM accounts WHERE id='account-1';")" == "ops@example.com" ]] || \
  fail "restored database content did not match"

expect_failure "restore over an existing database" \
  env CNSHELL_RELAY_ALLOW_PLAINTEXT_BACKUP=1 CNSHELL_RELAY_CONFIRM_SERVICE_STOPPED=1 \
  CNSHELL_RELAY_SQLITE3_BIN="$SQLITE3_BIN" \
  "$RESTORE_SCRIPT" "$backup_three" "$restored_database"

tampered_backup="$backup_directory/cnshell-relay-20260716T010104Z.sqlite"
cp "$backup_three" "$tampered_backup"
cp "$backup_three.sha256" "$tampered_backup.sha256"
printf 'tampered' >> "$tampered_backup"
expect_failure "tampered backup restore" \
  env CNSHELL_RELAY_ALLOW_PLAINTEXT_BACKUP=1 CNSHELL_RELAY_CONFIRM_SERVICE_STOPPED=1 \
  CNSHELL_RELAY_SQLITE3_BIN="$SQLITE3_BIN" \
  "$RESTORE_SCRIPT" "$tampered_backup" "$temporary_directory/tampered.sqlite"

wrong_schema_backup="$backup_directory/cnshell-relay-20260716T010105Z.sqlite"
"$SQLITE3_BIN" "$wrong_schema_backup" 'CREATE TABLE unrelated(value TEXT);'
wrong_schema_hash="$(shasum -a 256 "$wrong_schema_backup")"
printf '%s\n' "${wrong_schema_hash%% *}" > "$wrong_schema_backup.sha256"
expect_failure "wrong-schema backup restore" \
  env CNSHELL_RELAY_ALLOW_PLAINTEXT_BACKUP=1 CNSHELL_RELAY_CONFIRM_SERVICE_STOPPED=1 \
  CNSHELL_RELAY_SQLITE3_BIN="$SQLITE3_BIN" \
  "$RESTORE_SCRIPT" "$wrong_schema_backup" "$temporary_directory/wrong-schema.sqlite"

age_bin="${CNSHELL_RELAY_AGE_BIN:-$(command -v age 2>/dev/null || true)}"
if [[ -n "$age_bin" ]]; then
  age_keygen_bin="${CNSHELL_RELAY_AGE_KEYGEN_BIN:-${age_bin%/*}/age-keygen}"
  [[ -x "$age_bin" && -x "$age_keygen_bin" ]] || fail "age and age-keygen must both be executable"
  encrypted_directory="$temporary_directory/encrypted-backups"
  mkdir "$encrypted_directory"
  identity="$temporary_directory/backup-identity.agekey"
  "$age_keygen_bin" -o "$identity" > "$temporary_directory/age-keygen.log" 2>&1
  chmod 600 "$identity"
  recipient="$("$age_keygen_bin" -y "$identity")"
  encrypted_backup="$(env CNSHELL_RELAY_AGE_RECIPIENT="$recipient" \
    CNSHELL_RELAY_AGE_BIN="$age_bin" CNSHELL_RELAY_BACKUP_TIMESTAMP=20260716T020101Z \
    CNSHELL_RELAY_SQLITE3_BIN="$SQLITE3_BIN" \
    "$BACKUP_SCRIPT" "$database" "$encrypted_directory")"
  [[ "$encrypted_backup" == *.sqlite.age && -f "$encrypted_backup.sha256" ]] || \
    fail "encrypted backup files were not created"
  if LC_ALL=C grep -a -Fq 'SQLite format 3' "$encrypted_backup" || \
    LC_ALL=C grep -a -Fq 'ops@example.com' "$encrypted_backup"; then
    fail "encrypted backup exposed SQLite or sample account plaintext"
  fi
  encrypted_restore="$temporary_directory/encrypted-restored.sqlite"
  env CNSHELL_RELAY_CONFIRM_SERVICE_STOPPED=1 CNSHELL_RELAY_AGE_IDENTITY="$identity" \
    CNSHELL_RELAY_AGE_BIN="$age_bin" CNSHELL_RELAY_SQLITE3_BIN="$SQLITE3_BIN" \
    "$RESTORE_SCRIPT" "$encrypted_backup" "$encrypted_restore" >/dev/null
  [[ "$("$SQLITE3_BIN" "$encrypted_restore" "SELECT email FROM accounts WHERE id='account-1';")" == "ops@example.com" ]] || \
    fail "encrypted restore content did not match"

  wrong_identity="$temporary_directory/wrong-identity.agekey"
  "$age_keygen_bin" -o "$wrong_identity" > "$temporary_directory/wrong-age-keygen.log" 2>&1
  chmod 600 "$wrong_identity"
  expect_failure "encrypted restore with the wrong identity" \
    env CNSHELL_RELAY_CONFIRM_SERVICE_STOPPED=1 CNSHELL_RELAY_AGE_IDENTITY="$wrong_identity" \
    CNSHELL_RELAY_AGE_BIN="$age_bin" CNSHELL_RELAY_SQLITE3_BIN="$SQLITE3_BIN" \
    "$RESTORE_SCRIPT" "$encrypted_backup" "$temporary_directory/wrong-identity.sqlite"

  unsafe_identity="$temporary_directory/unsafe-identity.agekey"
  cp "$identity" "$unsafe_identity"
  chmod 644 "$unsafe_identity"
  expect_failure "encrypted restore with a group-readable identity" \
    env CNSHELL_RELAY_CONFIRM_SERVICE_STOPPED=1 CNSHELL_RELAY_AGE_IDENTITY="$unsafe_identity" \
    CNSHELL_RELAY_AGE_BIN="$age_bin" CNSHELL_RELAY_SQLITE3_BIN="$SQLITE3_BIN" \
    "$RESTORE_SCRIPT" "$encrypted_backup" "$temporary_directory/unsafe-identity.sqlite"
else
  printf 'relay operations drill: age unavailable, encrypted round trip skipped\n' >&2
fi

cargo build --quiet --manifest-path "$ROOT/Cargo.toml"
relay_binary="$ROOT/target/debug/cnshell-team-relay"
[[ -x "$relay_binary" ]] || fail "relay binary was not built"

port="${CNSHELL_RELAY_OPS_TEST_PORT:-$((40000 + $$ % 20000))}"
service_database="$temporary_directory/service.sqlite"
service_log="$temporary_directory/service.log"
env CNSHELL_RELAY_DATABASE_URL="sqlite://$service_database?mode=rwc" \
  CNSHELL_RELAY_BIND="127.0.0.1:$port" RUST_LOG=cnshell_team_relay=info \
  "$relay_binary" > "$service_log" 2>&1 &
relay_pid=$!

ready=0
for _ in {1..100}; do
  if curl --fail --silent "http://127.0.0.1:$port/ready" >/dev/null 2>&1; then
    ready=1
    break
  fi
  if ! kill -0 "$relay_pid" 2>/dev/null; then
    break
  fi
  sleep 0.1
done
if [[ "$ready" != "1" ]]; then
  sed -n '1,120p' "$service_log" >&2
  fail "relay did not become ready"
fi

health_response="$(curl --fail --silent "http://127.0.0.1:$port/health")"
ready_response="$(curl --fail --silent "http://127.0.0.1:$port/ready")"
metrics_response="$(curl --fail --silent "http://127.0.0.1:$port/metrics")"
[[ "$health_response" == '{"status":"ok"}' ]] || fail "liveness response was unexpected"
[[ "$ready_response" == '{"status":"ready"}' ]] || fail "readiness response was unexpected"
[[ "$metrics_response" == *'cnshell_relay_up 1'* ]] || fail "metrics did not report process liveness"
[[ "$metrics_response" == *'cnshell_relay_ready 1'* ]] || fail "metrics did not report database readiness"
[[ "$metrics_response" == *'cnshell_relay_websocket_active 0'* ]] || fail "metrics reported an unexpected active WebSocket"

pid_file="$temporary_directory/relay.pid"
printf '%s\n' "$relay_pid" > "$pid_file"
expect_failure "restore while the relay PID is active" \
  env CNSHELL_RELAY_ALLOW_PLAINTEXT_BACKUP=1 CNSHELL_RELAY_CONFIRM_SERVICE_STOPPED=1 \
  CNSHELL_RELAY_PID_FILE="$pid_file" CNSHELL_RELAY_SQLITE3_BIN="$SQLITE3_BIN" \
  "$RESTORE_SCRIPT" "$backup_three" "$temporary_directory/live-restore.sqlite"

kill -TERM "$relay_pid"
if ! wait "$relay_pid"; then
  relay_pid=""
  fail "relay did not exit cleanly after SIGTERM"
fi
relay_pid=""
grep -Fq 'CNshell team relay shutting down' "$service_log" || fail "graceful shutdown was not logged"

printf 'relay operations drill passed (%s)\n' "$REPOSITORY_ROOT"
