#!/usr/bin/env bash
# Deploy OhuVault to Casper Testnet.
# TODO(S2): add admin associated keys + operator gating before production deploy.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO_ROOT}"

if [[ ! -f .env ]]; then
    echo "ERROR: .env not found. Copy infra/.env.example to .env and fill secrets." >&2
    exit 1
fi

# shellcheck source=/dev/null
source .env

: "${NODE_URL:?NODE_URL is not set}"
: "${CHAIN_NAME:?CHAIN_NAME is not set}"

echo "TODO: implement cargo-odra / casper-client deploy of OhuVault to ${NODE_URL} on chain ${CHAIN_NAME}"
echo "  Deployer key path: ${DEPLOYER_SECRET_KEY_PATH:-<not set>}"
echo "  Admin key path:    ${ADMIN_SECRET_KEY_PATH:-<not set>}"
echo "  Operator key path: ${OPERATOR_SECRET_KEY_PATH:-<not set>}"
