#!/usr/bin/env bash
# Deploy OhuVault (S2) a Casper Testnet.
#
# Pasos:
#   1. (Opcional) configurar la cuenta admin como multisig nativo:
#      `bash infra/scripts/setup_admin_account.sh`
#   2. Deployar OhuVault con los init args de S2:
#      admin, operator, approvers, required_approvals, micropayment_cap.
#
# Anti-alucinación: la sintaxis exacta de `cargo-odra`/`casper-client` para
# inyectar runtime args al init debe confirmarse contra la versión instalada.
# TODO(audit): verificar el flujo de deploy de cargo-odra 0.1.7 + pass-through
# de init args (Odra.toml / `cargo odra deploy`). Mientras tanto, se imprime
# el plan y no se toca la red.
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

echo "==> Deploy de OhuVault a ${NODE_URL} on chain ${CHAIN_NAME}"
echo "    Deployer key path: ${DEPLOYER_SECRET_KEY_PATH:-<not set>}"
echo "    Admin key path:    ${ADMIN_SECRET_KEY_PATH:-<not set>}"
echo "    Operator key path: ${OPERATOR_SECRET_KEY_PATH:-<not set>}"
echo
echo "    Init args (S2):"
echo "      admin              = ${OHUVAULT_ADMIN_ACCOUNT_HASH:-<not set>}"
echo "      operator           = ${OHUVAULT_OPERATOR_ACCOUNT_HASH:-<not set>}"
echo "      approvers          = ${OHUVAULT_APPROVER_ACCOUNT_HASHES:-<not set>}"
echo "      required_approvals = ${OHUVAULT_REQUIRED_APPROVALS:-2}"
echo "      micropayment_cap   = ${OHUVAULT_MICROPAYMENT_CAP_MOTES:-1000000000} motes"
echo
echo "==> (Recomendado) configura primero la cuenta admin como multisig nativo:"
echo "      bash infra/scripts/setup_admin_account.sh"
echo

# TODO(audit): implementar el deploy real con cargo-odra 0.1.7 + casper-client.
# Flujo esperado (confirmar flags contra la versión instalada):
#   1. `cargo odra build` para generar el WASM optimizado de OhuVault.
#   2. `casper-client put-deploy` con --session-path=<ohu_vault.wasm> y los
#      runtime args del init:
#        admin, operator, approvers (Vec<PublicKey/AccountHash>),
#        required_approvals (u8), micropayment_cap (U512)
#      + los args internos de Odra (odra_cfg_*).
# El init de OhuVault valida el setup (admin!=operator, approvers distintos,
# required_approvals en [1, len(approvers)], cap>0); un deploy mal parametrizado
# revierte on-chain con Error::InvalidSetup.
echo "TODO(audit): implementar cargo-odra / casper-client deploy de OhuVault"
echo "      con los init args de S2 (ver TODO arriba)."
