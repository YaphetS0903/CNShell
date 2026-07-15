#!/bin/zsh
set -euo pipefail

ROOT="${0:A:h}/.."
TMP="$(mktemp -d /tmp/cnshell-sshd.XXXXXX)"
PORT="${CNSHELL_TEST_SSH_PORT:-32222}"
PASSWORD_PORT="${CNSHELL_TEST_PASSWORD_SSH_PORT:-32223}"
USER_NAME="$(id -un)"
cleanup() {
  [[ -f "$TMP/sshd.pid" ]] && kill "$(cat "$TMP/sshd.pid")" 2>/dev/null || true
  [[ -n "${PASSWORD_PID:-}" ]] && kill "$PASSWORD_PID" 2>/dev/null || true
  rm -rf "$TMP"
}
trap cleanup EXIT

ssh-keygen -q -t ed25519 -N '' -f "$TMP/host_key"
ssh-keygen -q -t ed25519 -N '' -f "$TMP/client_key"
ssh-keygen -q -t ed25519 -N '' -f "$TMP/wrong_key"
ssh-keygen -q -t ed25519 -N '' -f "$TMP/user_ca"
ssh-keygen -q -s "$TMP/user_ca" -I cnshell-certificate-test -n "$USER_NAME" -V -1m:+5m "$TMP/client_key.pub"
cp "$TMP/client_key.pub" "$TMP/authorized_keys"
chmod 600 "$TMP/authorized_keys"

/usr/sbin/sshd -D -e \
  -p "$PORT" \
  -h "$TMP/host_key" \
  -o "PidFile=$TMP/sshd.pid" \
  -o "AuthorizedKeysFile=$TMP/authorized_keys" \
  -o "TrustedUserCAKeys=$TMP/user_ca.pub" \
  -o StrictModes=no \
  -o PasswordAuthentication=no \
  -o KbdInteractiveAuthentication=no \
  -o PubkeyAuthentication=yes \
  -o X11Forwarding=yes \
  -o X11UseLocalhost=yes \
  -o XAuthLocation=/usr/bin/true \
  -o UsePAM=no \
  -o PermitRootLogin=no \
  -o "AllowUsers=$USER_NAME" &

python3 "$ROOT/scripts/password-ssh-server.py" "$PASSWORD_PORT" &
PASSWORD_PID=$!

for _ in {1..50}; do
  nc -z 127.0.0.1 "$PORT" 2>/dev/null && break
  sleep 0.1
done
nc -z 127.0.0.1 "$PORT"
for _ in {1..50}; do
  nc -z 127.0.0.1 "$PASSWORD_PORT" 2>/dev/null && break
  sleep 0.1
done
nc -z 127.0.0.1 "$PASSWORD_PORT"

CNSHELL_TEST_SSH_PORT="$PORT" \
CNSHELL_TEST_SSH_KEY="$TMP/client_key" \
CNSHELL_TEST_SSH_BAD_KEY="$TMP/wrong_key" \
CNSHELL_TEST_SSH_CERT="$TMP/client_key-cert.pub" \
CNSHELL_TEST_SSH_USER="$USER_NAME" \
CNSHELL_TEST_PASSWORD_SSH_PORT="$PASSWORD_PORT" \
cargo test --manifest-path "$ROOT/src-tauri/Cargo.toml" "${CNSHELL_PROTOCOL_FILTER:-live_ssh_}" -- --nocapture --test-threads=1
