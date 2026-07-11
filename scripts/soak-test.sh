#!/bin/zsh
set -euo pipefail

ROOT="${0:A:h}/.."
export CNSHELL_SOAK_SECONDS="${CNSHELL_SOAK_SECONDS:-28800}"
echo "CNshell soak test: ${CNSHELL_SOAK_SECONDS}s"
"$ROOT/scripts/protocol-test.sh"
