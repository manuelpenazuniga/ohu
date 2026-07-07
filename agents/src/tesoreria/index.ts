/**
 * **Tesorería** — proceso determinista con la llave `operator`.
 *
 * Rol en la matriz de autoridad on-chain:
 * - Observa lotes en estado FUNDED.
 * - Llama EXCLUSIVAMENTE `evaluate_lote`: lee el tally on-chain y fija
 *   EVAL_OK / EVAL_FAIL.
 * - NO puede mover capital (el contrato se lo impide).
 *
 * INV-2: el tally autoriza, no el agente.
 *
 * Uso:
 *   tsx --env-file=../../.env src/tesoreria/index.ts --lote 1 --lote 2
 *   tsx --env-file=../../.env src/tesoreria/index.ts   # usa SWARM_TARGET_LOTES
 */
import "dotenv/config";
import { loadOperatorConfig } from "../casper/env.js";
import { loadOperatorKey } from "../casper/keys.js";
import { evaluateLote } from "../casper/vault-client.js";
import { OHUVAULT_ERRORS, FATAL_AUTH_ERRORS } from "../casper/errors.js";
import { createSwarmLogger } from "../swarm/log.js";
import type { SwarmLogger } from "../swarm/log.js";

function parseTargetLotes(): number[] {
  const args = process.argv.slice(2);
  const ids: number[] = [];

  for (let i = 0; i < args.length; i++) {
    if (args[i] === "--lote" && args[i + 1]) {
      const id = Number.parseInt(args[i + 1]!, 10);
      if (!Number.isNaN(id) && id > 0) {
        ids.push(id);
      }
      i++;
    }
  }

  if (ids.length > 0) return ids;

  const csv = process.env["SWARM_TARGET_LOTES"] ?? "";
  if (csv.trim() === "") {
    throw new Error(
      "tesoreria: sin lotes objetivo. Pasa --lote <id> o define SWARM_TARGET_LOTES (CSV).",
    );
  }
  return csv
    .split(",")
    .map((s) => Number.parseInt(s.trim(), 10))
    .filter((n) => !Number.isNaN(n) && n > 0);
}

async function main(): Promise<void> {
  const config = loadOperatorConfig();
  const key = loadOperatorKey(config.operatorSecretKeyPath);
  const logger: SwarmLogger = createSwarmLogger(config.logFile);
  const agentAccount = config.operatorAccountHash;
  const targets = parseTargetLotes();

  console.log(
    `Tesorería iniciada (operator: ${agentAccount.slice(0, 12)}…). ` +
      `Lotes: [${targets.join(", ")}]`,
  );

  const done = new Set<number>();

  for (const loteId of targets) {
    if (done.has(loteId)) continue;

    while (true) {
      console.log(`Tesorería: evaluando lote ${loteId}…`);
      const result = await evaluateLote(key, loteId, config);

      if (result.success) {
        logger.log({
          ts: new Date().toISOString(),
          role: "operator",
          column: "PROPONE",
          agentAccount,
          entrypoint: "evaluate_lote",
          loteId,
          txHash: result.txHash,
          result: "EVAL_OK_OR_FAIL",
        });
        done.add(loteId);
        break;
      }

      if (result.userError === OHUVAULT_ERRORS.WINDOW_NOT_CLOSED) {
        console.log(
          `  Ventana aún no cerrada para lote ${loteId} — esperando ${config.pollIntervalMs}ms…`,
        );
        await new Promise((r) => setTimeout(r, config.pollIntervalMs));
        continue;
      }

      if (result.userError === OHUVAULT_ERRORS.LOTE_NOT_FUNDED) {
        logger.log({
          ts: new Date().toISOString(),
          role: "operator",
          column: "PROPONE",
          agentAccount,
          entrypoint: "evaluate_lote",
          loteId,
          txHash: result.txHash,
          result: "SKIP_ALREADY_EVALUATED",
        });
        done.add(loteId);
        break;
      }

      if (
        result.userError !== null &&
        FATAL_AUTH_ERRORS.includes(result.userError)
      ) {
        throw new Error(
          `FATAL: la llave cargada NO es operator (userError=${result.userError}). ` +
            `Verifica OPERATOR_SECRET_KEY_PATH. Abortando.`,
        );
      }

      // Error desconocido — reintentar tras poll interval
      console.log(
        `  Error inesperado en lote ${loteId} (userError=${result.userError}, tx=${result.txHash.slice(0, 12)}…). Reintentando en ${config.pollIntervalMs}ms…`,
      );
      await new Promise((r) => setTimeout(r, config.pollIntervalMs));
    }
  }

  console.log(`Tesorería: procesamiento completo. ${done.size}/${targets.length} lotes evaluados.`);
}

main().catch((err) => {
  console.error("Tesorería: error fatal:", err);
  process.exit(1);
});
