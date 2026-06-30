#!/usr/bin/env bash
# Deploy REAL de OhuVault a Casper Testnet via Odra Livenet.
#
# Flujo reproducible (los 3 pasos importan; el lowering MVP es OBLIGATORIO):
#   1. cargo odra build            -> wasm/OhuVault.wasm (con bulk-memory, NO deployable)
#   2. wasm-opt lowering -> MVP    -> el nodo Casper rechaza bulk-memory/sign-ext
#   3. cargo run livenet           -> envía el WASM + 8 init args, espera por SSE
#
# Por qué el paso 2: el toolchain Rust moderno (nightly-2026) emite bulk-memory
# (memory.copy/fill) y sign-ext en wasm32, incl. la std precompilada. La VM de
# Casper exige WASM nivel MVP. cargo-odra no honra build-std/target-feature de
# forma fiable, así que bajamos los ops con binaryen (wasm-opt) de forma
# determinista. Verificación autoritativa: re-leer con --disable-bulk-memory
# (falla si quedó algún op).
#
# Prerequisitos:
#   - casper-client, cargo-odra, binaryen (wasm-opt) instalados
#   - .env (raíz del repo) con ODRA_CASPER_LIVENET_* + OHUVAULT_* (ver .env.example)
#   - cuenta deployer fondeada (ODRA_CASPER_LIVENET_SECRET_KEY_PATH)
#
# Uso:  bash infra/scripts/deploy_testnet.sh
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CONTRACTS="$REPO_ROOT/contracts"
WASM="$CONTRACTS/wasm/OhuVault.wasm"

command -v wasm-opt >/dev/null || { echo "ERROR: falta binaryen (wasm-opt). brew install binaryen"; exit 1; }
[ -f "$REPO_ROOT/.env" ] || { echo "ERROR: falta .env en la raíz (ver .env.example)"; exit 1; }

echo "==> [1/3] cargo odra build"
( cd "$CONTRACTS" && cargo odra build )

echo "==> [2/3] wasm-opt: lowering bulk-memory + sign-ext a MVP"
# paso a: bajar los ops (sin -O para no reintroducir)
wasm-opt "$WASM" -o "$WASM.lowered" \
  --enable-bulk-memory --enable-sign-ext \
  --llvm-memory-copy-fill-lowering --signext-lowering
# paso b: optimizar con los features DESACTIVADOS (falla si quedó cualquier op -> verificación)
wasm-opt "$WASM.lowered" -o "$WASM" \
  --disable-bulk-memory --disable-sign-ext -Oz
rm -f "$WASM.lowered"
# verificación autoritativa final
wasm-opt "$WASM" --disable-bulk-memory --disable-sign-ext -o /dev/null 2>/dev/null \
  && echo "    OK: WASM MVP-limpio ($(wc -c < "$WASM") bytes)" \
  || { echo "ERROR: el WASM aún tiene ops bulk-memory/sign-ext"; exit 1; }

echo "==> [3/3] deploy livenet"
set -a; source "$REPO_ROOT/.env"; set +a
( cd "$CONTRACTS" && cargo run --bin ohu_livenet_deploy --features livenet )
