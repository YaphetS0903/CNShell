#!/usr/bin/env bash
set -euo pipefail

umask 077

fail() {
  printf 'restore: %s\n' "$*" >&2
  exit 1
}

usage() {
  printf 'Usage: %s <relay-backup> <new-database-path>\n' "${0##*/}" >&2
  exit 2
}

reject_multiline_path() {
  local value="$1"
  [[ "$value" != *$'\n'* && "$value" != *$'\r'* ]] || fail "paths cannot contain line breaks"
}

resolve_sqlite3() {
  local candidate="${CNSHELL_RELAY_SQLITE3_BIN:-/usr/bin/sqlite3}"
  if [[ -x "$candidate" ]]; then
    printf '%s\n' "$candidate"
    return
  fi
  command -v sqlite3 2>/dev/null || fail "sqlite3 is required"
}

resolve_age() {
  local candidate="${CNSHELL_RELAY_AGE_BIN:-}"
  if [[ -n "$candidate" ]]; then
    [[ -x "$candidate" ]] || fail "CNSHELL_RELAY_AGE_BIN is not executable"
    printf '%s\n' "$candidate"
    return
  fi
  command -v age 2>/dev/null || fail "age is required to restore this backup"
}

sha256_file() {
  local path="$1"
  local output
  if command -v shasum >/dev/null 2>&1; then
    output="$(shasum -a 256 "$path")"
  elif command -v sha256sum >/dev/null 2>&1; then
    output="$(sha256sum "$path")"
  else
    fail "shasum or sha256sum is required"
  fi
  printf '%s\n' "${output%% *}"
}

verify_snapshot() {
  local sqlite3_bin="$1"
  local snapshot="$2"
  local quick_check foreign_key_check table_count column_count
  quick_check="$("$sqlite3_bin" -batch -noheader "$snapshot" 'PRAGMA quick_check;')"
  [[ "$quick_check" == "ok" ]] || fail "SQLite quick_check rejected the restored data"
  foreign_key_check="$("$sqlite3_bin" -batch -noheader "$snapshot" 'PRAGMA foreign_key_check;')"
  [[ -z "$foreign_key_check" ]] || fail "SQLite foreign_key_check rejected the restored data"
  table_count="$("$sqlite3_bin" -batch -noheader "$snapshot" \
    "SELECT count(*) FROM sqlite_schema WHERE type='table' AND name IN ('accounts','workspaces','devices','terminal_rooms','relay_audit_events');")"
  [[ "$table_count" == "5" ]] || fail "the backup is not a CNshell relay database"
  column_count="$("$sqlite3_bin" -batch -noheader "$snapshot" \
    "SELECT (SELECT count(*) FROM pragma_table_info('accounts') WHERE name IN ('id','email','password_hash','status')) + (SELECT count(*) FROM pragma_table_info('workspaces') WHERE name IN ('id','key_epoch','status')) + (SELECT count(*) FROM pragma_table_info('devices') WHERE name IN ('id','workspace_id','member_id','encryption_public_key','signing_public_key','status')) + (SELECT count(*) FROM pragma_table_info('terminal_rooms') WHERE name IN ('id','workspace_id','host_device_id','key_epoch','status','next_output_sequence','lease_generation')) + (SELECT count(*) FROM pragma_table_info('relay_audit_events') WHERE name IN ('id','workspace_id','actor_member_id','action','target_type','target_id','created_at'));")"
  [[ "$column_count" == "27" ]] || fail "the relay database schema is incomplete"
}

require_private_file() {
  local path="$1"
  local mode
  if mode="$(stat -f '%Lp' "$path" 2>/dev/null)"; then
    :
  elif mode="$(stat -c '%a' "$path" 2>/dev/null)"; then
    :
  else
    fail "cannot inspect age identity permissions"
  fi
  [[ "$mode" =~ ^[0-7]{3,4}$ ]] || fail "age identity permissions are invalid"
  (( (8#$mode & 077) == 0 )) || fail "age identity must not be readable by group or other users"
}

[[ $# -eq 2 ]] || usage

backup_path="$1"
target_path="$2"
reject_multiline_path "$backup_path"
reject_multiline_path "$target_path"
[[ "${CNSHELL_RELAY_CONFIRM_SERVICE_STOPPED:-0}" == "1" ]] || \
  fail "stop the relay, then set CNSHELL_RELAY_CONFIRM_SERVICE_STOPPED=1"
[[ ! -L "$backup_path" ]] || fail "backup path cannot be a symbolic link"
[[ -f "$backup_path" ]] || fail "backup must be an existing regular file"
[[ ! -e "$target_path" && ! -L "$target_path" ]] || fail "target database already exists"
backup_path="$(cd "$(dirname "$backup_path")" && pwd -P)/$(basename "$backup_path")"

backup_name="${backup_path##*/}"
[[ "$backup_name" =~ ^cnshell-relay-[0-9]{8}T[0-9]{6}Z\.sqlite(\.age)?$ ]] || \
  fail "backup filename is not recognized"
checksum_path="$backup_path.sha256"
[[ ! -L "$checksum_path" ]] || fail "checksum path cannot be a symbolic link"
[[ -f "$checksum_path" ]] || fail "checksum sidecar is missing"
checksum_size="$(wc -c < "$checksum_path" | tr -d '[:space:]')"
[[ "$checksum_size" =~ ^[0-9]+$ && "$checksum_size" -le 128 ]] || fail "checksum sidecar is oversized"
checksum_contents="$(<"$checksum_path")"
[[ "$checksum_contents" =~ ^[0-9a-f]{64}$ ]] || fail "checksum sidecar is malformed"
actual_checksum="$(sha256_file "$backup_path")"
[[ "$actual_checksum" == "$checksum_contents" ]] || fail "backup checksum mismatch"

if [[ -n "${CNSHELL_RELAY_PID_FILE:-}" ]]; then
  pid_file="$CNSHELL_RELAY_PID_FILE"
  reject_multiline_path "$pid_file"
  [[ ! -L "$pid_file" ]] || fail "PID file cannot be a symbolic link"
  if [[ -f "$pid_file" ]]; then
    pid="$(<"$pid_file")"
    [[ "$pid" =~ ^[0-9]+$ ]] || fail "PID file is malformed"
    if kill -0 "$pid" 2>/dev/null; then
      fail "relay process $pid is still running"
    fi
  fi
fi

target_directory="${target_path%/*}"
if [[ "$target_directory" == "$target_path" ]]; then
  target_directory="."
elif [[ -z "$target_directory" ]]; then
  target_directory="/"
fi
[[ ! -L "$target_directory" ]] || fail "target directory cannot be a symbolic link"
[[ -d "$target_directory" ]] || fail "target directory must already exist"
[[ -w "$target_directory" ]] || fail "target directory is not writable"
target_directory="$(cd "$target_directory" && pwd -P)"
target_path="$target_directory/${target_path##*/}"

sqlite3_bin="$(resolve_sqlite3)"
staging_directory="$(mktemp -d "$target_directory/.cnshell-relay-restore.XXXXXX")"
cleanup() {
  rm -rf -- "$staging_directory"
}
trap cleanup EXIT HUP INT TERM
snapshot="$staging_directory/restored.sqlite"

case "$backup_name" in
  *.sqlite.age)
    identity="${CNSHELL_RELAY_AGE_IDENTITY:-}"
    [[ -n "$identity" ]] || fail "CNSHELL_RELAY_AGE_IDENTITY is required"
    reject_multiline_path "$identity"
    [[ ! -L "$identity" ]] || fail "age identity path cannot be a symbolic link"
    [[ -f "$identity" ]] || fail "age identity must be an existing regular file"
    require_private_file "$identity"
    age_bin="$(resolve_age)"
    "$age_bin" --decrypt --identity "$identity" "$backup_path" > "$snapshot"
    ;;
  *.sqlite)
    [[ "${CNSHELL_RELAY_ALLOW_PLAINTEXT_BACKUP:-0}" == "1" ]] || \
      fail "plaintext restore requires CNSHELL_RELAY_ALLOW_PLAINTEXT_BACKUP=1"
    cp -- "$backup_path" "$snapshot"
    ;;
esac

chmod 600 "$snapshot"
verify_snapshot "$sqlite3_bin" "$snapshot"
[[ ! -e "$target_path" && ! -L "$target_path" ]] || fail "target database appeared during restore"
mv -- "$snapshot" "$target_path"
chmod 600 "$target_path"

printf '%s\n' "$target_path"
