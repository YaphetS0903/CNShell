#!/bin/zsh
set -euo pipefail

ROOT="${0:A:h}/.."
LEVEL="${CNSHELL_TRANSFER_ACCEPTANCE_LEVEL:-full}"

if [[ "$LEVEL" != "full" && "$LEVEL" != "quick" ]]; then
  print -u2 "CNSHELL_TRANSFER_ACCEPTANCE_LEVEL must be full or quick"
  exit 2
fi

for command in ssh-keygen sshd nc python3 cargo npm; do
  if ! command -v "$command" >/dev/null 2>&1; then
    print -u2 "transfer acceptance prerequisite missing: $command"
    exit 2
  fi
done

print "CNshell transfer acceptance: $LEVEL"
if [[ "$LEVEL" == "full" ]]; then
  # Includes the 1 GiB SHA-256 round trip, 100k-entry directory, proxies,
  # disconnect recovery, directory transfers, and the queue scenarios below.
  "$ROOT/scripts/protocol-test.sh"
else
  CNSHELL_PROTOCOL_FILTER="live_ssh_transfer_queue_pause_cancel_conflicts_retry_and_hash" \
    "$ROOT/scripts/protocol-test.sh"
fi

# These deterministic fault-injection checks complement the real SSH fixture.
cargo test --manifest-path "$ROOT/src-tauri/Cargo.toml" --lib "sftp::tests::" -- --test-threads=1
cargo test --manifest-path "$ROOT/src-tauri/Cargo.toml" --lib \
  "db::tests::completed_transfer_persists_its_renamed_destination" -- --exact
npm run --prefix "$ROOT" test -- \
  src/features/files/TransferQueue.test.ts \
  src/lib/transfer-sync.test.tsx \
  src/lib/window-close-protection.test.ts

print "CNshell transfer acceptance passed: $LEVEL"
