#!/usr/bin/env bash
# =============================================================================
# S2 — Configura la cuenta `admin` de OhuVault como multisig NATIVO de Casper
# (associated keys + weights + action thresholds), SIN Addressable Entity (INV-3).
#
# Por qué: INV-1 se defiende en DOS capas:
#   (1) capa on-chain (contrato OhuVault): `execute` exige `caller == admin`
#       + M aprobaciones distintas (approve/execute). Testeada en `cargo odra test`.
#   (2) capa nativa (este script): la cuenta `admin` es un multisig Casper. Para
#       que un deploy que llame `execute` siquiera se someta on-chain, hace falta
#       co-firma off-chain: la suma de pesos de las firmas debe ser
#       >= `deployment_threshold`. Con `deployment_threshold` > peso de cualquier
#       clave individual, NINGUNA clave sola puede deployar → co-firma forzada.
#
# Plan de pesos (por defecto, configurable vía .env):
#   - clave principal del admin (identity key):  peso 1  (presente al crear la cuenta)
#   - N co-firmantes (approvers nativos):         peso 1  cada uno
#   - deployment_threshold      = 3   (necesita 3 de 4 firmas para deployar)
#   - key_management_threshold  = 4   (necesita las 4 firmas para cambiar claves)
#   Con esto, el agente (cuenta `operator`, ajena a esta cuenta) NO puede firmar
#   un deploy del admin; y ni siquiera una clave única del admin basta.
#
# Modelo de cuenta objetivo: pre-Addressable Entity (Casper 1.x). El estado se
# verifica con `casper-client query-global-state`, que devuelve
#   action_thresholds: { deployment, key_management }
#   associated_keys:   [ { account_hash, weight }, ... ]
#
# ADVERTENCIA (anti-alucinación): `casper-client` NO tiene un subcomando directo
# "add-associated-key". La gestión de claves asociadas se hace enviando un deploy
# cuya *session* llama a las host functions del sistema:
#     add_associated_key(public_key, weight)
#     set_threshold(ActionThreshold::Deployment, weight)
#     set_threshold(ActionThreshold::KeyManagement, weight)
# Eso requiere un WASM de gestión de claves (`KEYS_MANAGER_WASM`). Su interfaz
# esperada está documentada más abajo. Mientras no se provea, el script imprime
# el plan y sale sin tocar la red.
#
# TODO(audit): verificar los flags exactos de `casper-client put-deploy`
# (--session-path, --session-arg "<name>:<TYPE>=<value>") contra la versión
# instalada (repo: casper-ecosystem/casper-client-rs, v5.0.1). Los subcomandos
# usados aquí (put-deploy, query-global-state, get-account, keygen,
# account-address) están confirmados en el README oficial.
# =============================================================================
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO_ROOT}"

if [[ -f .env ]]; then
    # shellcheck source=/dev/null
    source .env
fi

: "${NODE_URL:=https://node.testnet.casperlabs.io/rpc}"
: "${CHAIN_NAME:=casper-test}"

# Plan de pesos (defaults seguros; sobreescribir en .env).
: "${DEPLOYMENT_THRESHOLD:=3}"
: "${KEY_MANAGEMENT_THRESHOLD:=4}"
: "${APPROVER_WEIGHT:=1}"

echo "==> Plan multisig nativo de la cuenta admin (INV-1, capa 2)"
echo "    Red:                   ${NODE_URL} (${CHAIN_NAME})"
echo "    deployment_threshold:  ${DEPLOYMENT_THRESHOLD}  (co-firma mínima para deployar)"
echo "    key_management_threshold: ${KEY_MANAGEMENT_THRESHOLD}  (para cambiar claves)"
echo "    peso por approver:     ${APPROVER_WEIGHT}"
echo

# -----------------------------------------------------------------------------
# Prerequisitos. Si falta casper-client o el WASM de gestión de claves, no se
# toca la red: se imprime el plan y se sale limpio (amigable para CI/dev).
# -----------------------------------------------------------------------------
need_cmd() { command -v "$1" >/dev/null 2>&1; }

if ! need_cmd casper-client; then
    echo "WARN: 'casper-client' no está instalado." >&2
    echo "      Instálalo desde https://github.com/casper-ecosystem/casper-client-rs" >&2
    echo "      Plan impreso arriba; no se modificó la red." >&2
    exit 0
fi

if [[ -z "${ADMIN_SECRET_KEY_PATH:-}" ]]; then
    echo "WARN: ADMIN_SECRET_KEY_PATH no está definido en .env." >&2
    echo "      Genera claves con: casper-client keygen <dir>" >&2
    echo "      Plan impreso arriba; no se modificó la red." >&2
    exit 0
fi

if [[ ! -f "${ADMIN_SECRET_KEY_PATH}" ]]; then
    echo "ERROR: no existe el archivo de clave ${ADMIN_SECRET_KEY_PATH}" >&2
    exit 1
fi

# Interfaz esperada del WASM de gestión de claves (KEYS_MANAGER_WASM):
#   entry point `add_key`:    args  public_key:PublicKey, weight:u8
#   entry point `set_thresholds`: args deployment_threshold:u8, key_management_threshold:u8
# El WASM debe llamar a las host functions del sistema:
#   casper_contract::contract_api::system::add_associated_key(public_key, weight)
#   casper_contract::contract_api::system::set_threshold(action, weight)
# TODO(audit): confirmar la disponibilidad de un WASM de referencia (p.ej. en
# casper-network/casper-node utils) o construir uno mínimo en contracts/.
if [[ -z "${KEYS_MANAGER_WASM:-}" || ! -f "${KEYS_MANAGER_WASM:-}" ]]; then
    echo "WARN: KEYS_MANAGER_WASM no provisto (path a un WASM de gestión de claves)." >&2
    echo "      Interfaz esperada documentada en este script." >&2
    echo "      Plan impreso arriba; no se modificó la red." >&2
    exit 0
fi

if [[ -z "${APPROVER_PUBLIC_KEYS_HEX:-}" ]]; then
    echo "WARN: APPROVER_PUBLIC_KEYS_HEX no definido en .env." >&2
    echo "      Lista de claves públicas (hex) de los co-firmantes, separadas por espacio." >&2
    echo "      Plan impreso arriba; no se modificó la red." >&2
    exit 0
fi

# -----------------------------------------------------------------------------
# Derivar la identidad de la cuenta admin desde su clave pública.
# TODO(audit): confirmar el flag --public-key de `account-address` (README lo
# lista como subcomando para generar account-hash desde una public key).
# -----------------------------------------------------------------------------
ADMIN_PUBKEY_HEX="$(cat "$(dirname "${ADMIN_SECRET_KEY_PATH}")/public_key_hex" 2>/dev/null || true)"
if [[ -z "${ADMIN_PUBKEY_HEX}" ]]; then
    echo "ERROR: no se encontró public_key_hex junto a ${ADMIN_SECRET_KEY_PATH}" >&2
    exit 1
fi
ADMIN_ACCOUNT_HASH="$(casper-client account-address --public-key="${ADMIN_PUBKEY_HEX}" 2>/dev/null || true)"
echo "    Admin public key:  ${ADMIN_PUBKEY_HEX}"
echo "    Admin account hash: ${ADMIN_ACCOUNT_HASH:-<pendiente>}"
echo

# -----------------------------------------------------------------------------
# 1) Añadir cada co-firmante como associated key (bootstrap: la clave principal
#    del admin basta mientras key_management_threshold sigue siendo 1).
# TODO(audit): sintaxis exacta de --session-arg "<name>:<TYPE>=<value>" para
# PublicKey; ajustar según la versión instalada.
# -----------------------------------------------------------------------------
read -r -a APPROVER_KEYS <<< "${APPROVER_PUBLIC_KEYS_HEX}"
echo "==> Añadiendo ${#APPROVER_KEYS[@]} co-firmantes (weight=${APPROVER_WEIGHT}) a la cuenta admin"
for pk in "${APPROVER_KEYS[@]}"; do
    echo "    + associated key ${pk} (weight ${APPROVER_WEIGHT})"
    casper-client put-deploy \
        --node-address="${NODE_URL}" \
        --chain-name="${CHAIN_NAME}" \
        --secret-key="${ADMIN_SECRET_KEY_PATH}" \
        --session-path="${KEYS_MANAGER_WASM}" \
        --session-entry-point="add_key" \
        --session-arg="public_key:PublicKey='${pk}'" \
        --session-arg="weight:u8='${APPROVER_WEIGHT}'" \
        --payment-amount=10000000000 \
        --ttl="1h"
done
echo

# -----------------------------------------------------------------------------
# 2) Subir los thresholds al plan (último paso: con key_management_threshold=1
#    la clave principal aún puede actuar sola; al subirlo se bloquea).
# -----------------------------------------------------------------------------
echo "==> Subiendo action thresholds: deployment=${DEPLOYMENT_THRESHOLD}, key_management=${KEY_MANAGEMENT_THRESHOLD}"
casper-client put-deploy \
    --node-address="${NODE_URL}" \
    --chain-name="${CHAIN_NAME}" \
    --secret-key="${ADMIN_SECRET_KEY_PATH}" \
    --session-path="${KEYS_MANAGER_WASM}" \
    --session-entry-point="set_thresholds" \
    --session-arg="deployment_threshold:u8='${DEPLOYMENT_THRESHOLD}'" \
    --session-arg="key_management_threshold:u8='${KEY_MANAGEMENT_THRESHOLD}'" \
    --payment-amount=10000000000 \
    --ttl="1h"
echo

# -----------------------------------------------------------------------------
# 3) Verificar on-chain el estado de la cuenta admin.
#    Se espera: action_thresholds.deployment == DEPLOYMENT_THRESHOLD y
#               |associated_keys| == 1 + #APPROVER_KEYS.
# TODO(audit): confirmar flags de query-global-state (--key con public key hex).
# -----------------------------------------------------------------------------
echo "==> Verificando estado de la cuenta admin en ${NODE_URL}"
STATE_ROOT_HASH="$(casper-client get-block --node-address="${NODE_URL}" 2>/dev/null \
    | python3 -c 'import sys,json;print(json.load(sys.stdin)["result"]["block"]["header"]["state_root_hash"])' 2>/dev/null || true)"
if [[ -n "${STATE_ROOT_HASH}" ]]; then
    casper-client query-global-state \
        --node-address="${NODE_URL}" \
        --state-root-hash="${STATE_ROOT_HASH}" \
        --key="${ADMIN_PUBKEY_HEX}"
else
    echo "WARN: no se pudo obtener state_root_hash para verificar; revisa a mano con:" >&2
    echo "      casper-client query-global-state --node-address=${NODE_URL} --key=${ADMIN_PUBKEY_HEX}" >&2
fi
echo
echo "==> Listo. La cuenta admin ahora exige co-firma nativa (deployment_threshold=${DEPLOYMENT_THRESHOLD})."
echo "    Recordatorio: esto COMPLEMENTA el M-de-N on-chain de OhuVault (approve/execute)."
