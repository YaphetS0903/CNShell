#!/usr/bin/env bash
set -euo pipefail

umask 077

fail() {
  printf 'backup: %s\n' "$*" >&2
  exit 1
}

usage() {
  printf 'Usage: %s <relay-database> <backup-directory>\n' "${0##*/}" >&2
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
  command -v age 2>/dev/null || fail "age is required for encrypted backups"
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
  [[ "$quick_check" == "ok" ]] || fail "SQLite quick_check rejected the snapshot"
  foreign_key_check="$("$sqlite3_bin" -batch -noheader "$snapshot" 'PRAGMA foreign_key_check;')"
  [[ -z "$foreign_key_check" ]] || fail "SQLite foreign_key_check rejected the snapshot"
  table_count="$("$sqlite3_bin" -batch -noheader "$snapshot" \
    "SELECT count(*) FROM sqlite_schema WHERE type='table' AND name IN ('accounts','workspaces','devices','terminal_rooms','relay_audit_events');")"
  [[ "$table_count" == "5" ]] || fail "the snapshot is not a CNshell relay database"
  column_count="$("$sqlite3_bin" -batch -noheader "$snapshot" \
    "SELECT (SELECT count(*) FROM pragma_table_info('accounts') WHERE name IN ('id','email','password_hash','status')) + (SELECT count(*) FROM pragma_table_info('workspaces') WHERE name IN ('id','key_epoch','status')) + (SELECT count(*) FROM pragma_table_info('devices') WHERE name IN ('id','workspace_id','member_id','encryption_public_key','signing_public_key','status')) + (SELECT count(*) FROM pragma_table_info('terminal_rooms') WHERE name IN ('id','workspace_id','host_device_id','key_epoch','status','next_output_sequence','lease_generation')) + (SELECT count(*) FROM pragma_table_info('relay_audit_events') WHERE name IN ('id','workspace_id','actor_member_id','action','target_type','target_id','created_at'));")"
  [[ "$column_count" == "27" ]] || fail "the relay database schema is incomplete"
}

apply_retention() {
  local directory="$1"
  local keep="$2"
  local protected_path="$3"
  local path base index old_ifs retained_others
  local -a candidates=()
  local -a sorted=()

  [[ "$keep" != "0" ]] || return 0
  shopt -s nullglob
  for path in "$directory"/cnshell-relay-*.sqlite "$directory"/cnshell-relay-*.sqlite.age; do
    base="${path##*/}"
    if [[ -f "$path" && ! -L "$path" && "$base" =~ ^cnshell-relay-[0-9]{8}T[0-9]{6}Z\.sqlite(\.age)?$ ]]; then
      candidates+=("$path")
    fi
  done
  shopt -u nullglob
  (( ${#candidates[@]} > keep )) || return 0

  old_ifs="$IFS"
  IFS=$'\n'
  sorted=($(printf '%s\n' "${candidates[@]}" | LC_ALL=C sort -r))
  IFS="$old_ifs"
  retained_others=0
  for ((index = 0; index < ${#sorted[@]}; index += 1)); do
    path="${sorted[$index]}"
    if [[ "$path" == "$protected_path" ]]; then
      continue
    fi
    if (( retained_others < keep - 1 )); then
      retained_others=$((retained_others + 1))
    else
      rm -f -- "$path" "$path.sha256"
    fi
  done
}

[[ $# -eq 2 ]] || usage

database="$1"
backup_directory="$2"
reject_multiline_path "$database"
reject_multiline_path "$backup_directory"
[[ ! -L "$database" ]] || fail "database path cannot be a symbolic link"
[[ -f "$database" ]] || fail "database must be an existing regular file"
[[ ! -L "$backup_directory" ]] || fail "backup directory cannot be a symbolic link"
[[ -d "$backup_directory" ]] || fail "backup directory must already exist"
[[ -w "$backup_directory" ]] || fail "backup directory is not writable"
database_absolute="$(cd "$(dirname "$database")" && pwd -P)/$(basename "$database")"
backup_directory="$(cd "$backup_directory" && pwd -P)"

sqlite3_bin="$(resolve_sqlite3)"
timestamp="${CNSHELL_RELAY_BACKUP_TIMESTAMP:-$(date -u +%Y%m%dT%H%M%SZ)}"
[[ "$timestamp" =~ ^[0-9]{8}T[0-9]{6}Z$ ]] || fail "backup timestamp must use YYYYMMDDTHHMMSSZ"
retention_count="${CNSHELL_RELAY_BACKUP_RETENTION_COUNT:-14}"
[[ "$retention_count" =~ ^(0|[1-9][0-9]{0,3}|10000)$ ]] || \
  fail "retention count must be an integer from 0 through 10000"

recipient="${CNSHELL_RELAY_AGE_RECIPIENT:-}"
if [[ -n "$recipient" ]]; then
  [[ ${#recipient} -le 4096 && "$recipient" != *$'\n'* && "$recipient" != *$'\r'* ]] || \
    fail "age recipient is malformed"
  age_bin="$(resolve_age)"
  backup_name="cnshell-relay-$timestamp.sqlite.age"
  encrypted=1
elif [[ "${CNSHELL_RELAY_ALLOW_PLAINTEXT_BACKUP:-0}" == "1" ]]; then
  backup_name="cnshell-relay-$timestamp.sqlite"
  encrypted=0
else
  fail "set CNSHELL_RELAY_AGE_RECIPIENT; plaintext is allowed only with CNSHELL_RELAY_ALLOW_PLAINTEXT_BACKUP=1"
fi

final_path="$backup_directory/$backup_name"
checksum_path="$final_path.sha256"
[[ ! -e "$final_path" && ! -L "$final_path" ]] || fail "backup already exists: $backup_name"
[[ ! -e "$checksum_path" && ! -L "$checksum_path" ]] || fail "checksum already exists: $backup_name.sha256"

staging_directory="$(mktemp -d "$backup_directory/.cnshell-relay-backup.XXXXXX")"
cleanup() {
  rm -rf -- "$staging_directory"
}
trap cleanup EXIT HUP INT TERM

snapshot="$staging_directory/snapshot.sqlite"
staging_name="${staging_directory##*/}"
(
  cd "$backup_directory"
  "$sqlite3_bin" -batch -cmd '.timeout 5000' "$database_absolute" \
    "VACUUM INTO '$staging_name/snapshot.sqlite';"
)
chmod 600 "$snapshot"
verify_snapshot "$sqlite3_bin" "$snapshot"

staged_backup="$staging_directory/$backup_name"
if [[ "$encrypted" == "1" ]]; then
  "$age_bin" --recipient "$recipient" --output "$staged_backup" "$snapshot"
  chmod 600 "$staged_backup"
  rm -f -- "$snapshot"
else
  mv -- "$snapshot" "$staged_backup"
fi

digest="$(sha256_file "$staged_backup")"
printf '%s\n' "$digest" > "$staging_directory/$backup_name.sha256"
chmod 600 "$staging_directory/$backup_name.sha256"

[[ ! -e "$final_path" && ! -L "$final_path" ]] || fail "backup destination changed while the snapshot was being created"
mv -- "$staged_backup" "$final_path"
mv -- "$staging_directory/$backup_name.sha256" "$checksum_path"
apply_retention "$backup_directory" "$retention_count" "$final_path"

printf '%s\n' "$final_path"
