#!/usr/bin/env bash
# Simulador de semana (F8): siembra N lotes a FUNDED (binario Rust, matado en
# FUNDED para esquivar el corte de conexión tras el sleep) y los liquida con los
# agentes TS (Tesorería evalúa por silencio → Autorizador libera). Genera tx
# reales para la puerta de elegibilidad + datos vivos (reputación / solvencia).
#
# Uso:  SIM_LOTES="6 7" bash infra/scripts/simulate-week.sh
set -uo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
set -a; source "$REPO/.env" 2>/dev/null; set +a
read -r -a LOTES <<< "${SIM_LOTES:-6 7}"
WORK="$(mktemp -d)"

seed_to_funded() {
  local lote="$1" log="$WORK/seed-$lote.log"
  echo "▶ Sembrando lote $lote a FUNDED…"
  ( cd "$REPO/contracts" && SIM_LOTE_ID="$lote" cargo run --bin ohu_livenet_e2e --features livenet ) > "$log" 2>&1 &
  local pid=$!
  for _ in $(seq 1 150); do
    if grep -q "(FUNDED)" "$log" 2>/dev/null; then
      pkill -f "ohu_livenet_e2e" 2>/dev/null
      echo "  ✅ lote $lote en FUNDED"
      return 0
    fi
    if grep -qiE "panicked|error\[|could not compile|LoteAlreadyExists|LoteNotOpen" "$log" 2>/dev/null; then
      echo "  ✗ lote $lote falló al sembrar: $(grep -iE 'panicked|error|AlreadyExists|NotOpen' "$log" | tail -1)"
      kill "$pid" 2>/dev/null; pkill -f "ohu_livenet_e2e" 2>/dev/null
      return 1
    fi
    sleep 3
  done
  echo "  ✗ lote $lote: timeout sembrando"; kill "$pid" 2>/dev/null; pkill -f "ohu_livenet_e2e" 2>/dev/null
  return 1
}

echo "═══ Simulador de semana · lotes: ${LOTES[*]} ═══"
for lote in "${LOTES[@]}"; do
  seed_to_funded "$lote" || { echo "Abortando (seed de $lote falló)."; exit 1; }
done

# Argumentos --lote para los agentes.
LOTE_ARGS=()
for l in "${LOTES[@]}"; do LOTE_ARGS+=(--lote "$l"); done

echo "▶ Tesorería evalúa ${LOTES[*]} (silencio = recibido)…"
( cd "$REPO/agents" && TX_PAYMENT_MOTES=8000000000 SWARM_POLL_INTERVAL_MS=20000 \
  ./node_modules/.bin/tsx --env-file="$REPO/.env" src/tesoreria/index.ts "${LOTE_ARGS[@]}" )

echo "▶ Autorizador libera ${LOTES[*]}…"
( cd "$REPO/agents" && TX_PAYMENT_MOTES=8000000000 SWARM_POLL_INTERVAL_MS=20000 \
  ./node_modules/.bin/tsx --env-file="$REPO/.env" src/autorizador/index.ts "${LOTE_ARGS[@]}" )

echo "✅ Simulación completa: ${LOTES[*]} liquidados on-chain."
