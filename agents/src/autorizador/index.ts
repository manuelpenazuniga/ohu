/**
 * **Autorizador** — proceso determinista con la llave `admin`.
 *
 * Rol en la matriz de autoridad on-chain:
 * - Tras la evaluación, ejecuta el movimiento de capital.
 * - Si el lote está en EVAL_OK → llama `release_to_producer` (paga al productor).
 * - Si el lote está en EVAL_FAIL → llama `settle_failure` (refund + slash +
 *   indemnización).
 *
 * INV-2: la llave admin es la única que puede mover capital; el Autorizador
 * JAMÁS carga la llave operator.
 *
 * Uso:
 *   tsx --env-file=../../.env src/autorizador/index.ts --lote 1 --lote 2
 *   tsx --env-file=../../.env src/autorizador/index.ts   # usa SWARM_TARGET_LOTES
 */
import "dotenv/config";
import { loadSwarmConfig } from "../casper/env.js";
import { loadAdminKey } from "../casper/keys.js";
import { releaseToProducer, settleFailure } from "../casper/vault-client.js";
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
      "autorizador: sin lotes objetivo. Pasa --lote <id> o define SWARM_TARGET_LOTES (CSV).",
    );
  }
  return csv
    .split(",")
    .map((s) => Number.parseInt(s.trim(), 10))
    .filter((n) => !Number.isNaN(n) && n > 0);
}

async function main(): Promise<void> {
  const config = loadSwarmConfig();
  const key = loadAdminKey(config.adminSecretKeyPath);
  const logger: SwarmLogger = createSwarmLogger(config.logFile);
  const agentAccount = config.adminAccountHash;
  const targets = parseTargetLotes();

  console.log(
    `Autorizador iniciado (admin: ${agentAccount.slice(0, 12)}…). ` +
      `Lotes: [${targets.join(", ")}]`,
  );

  // Delay inicial configurable para evitar tx de sondeo prematuras.
  if (config.autorizadorStartDelayMs > 0) {
    console.log(
      `  Delay inicial: ${config.autorizadorStartDelayMs}ms…`,
    );
    await new Promise((r) => setTimeout(r, config.autorizadorStartDelayMs));
  }

  const done = new Set<number>();

  for (const loteId of targets) {
    if (done.has(loteId)) continue;

    while (true) {
      console.log(`Autorizador: intentando release_to_producer para lote ${loteId}…`);
      const releaseResult = await releaseToProducer(key, loteId, config);

      if (releaseResult.success) {
        logger.log({
          ts: new Date().toISOString(),
          role: "admin",
          column: "AUTORIZA",
          agentAccount,
          entrypoint: "release_to_producer",
          loteId,
          txHash: releaseResult.txHash,
          result: "SETTLED_OK",
        });
        done.add(loteId);
        break;
      }

      if (releaseResult.userError === OHUVAULT_ERRORS.LOTE_NOT_RELEASABLE) {
        // Estado ≠ EVAL_OK → intentar settle_failure
        console.log(
          `  Lote ${loteId} no es EVAL_OK (userError=${releaseResult.userError}). ` +
            `Intentando settle_failure…`,
        );
        const failResult = await settleFailure(key, loteId, config);

        if (failResult.success) {
          logger.log({
            ts: new Date().toISOString(),
            role: "admin",
            column: "AUTORIZA",
            agentAccount,
            entrypoint: "settle_failure",
            loteId,
            txHash: failResult.txHash,
            result: "SETTLED_FAIL",
          });
          done.add(loteId);
          break;
        }

        if (failResult.userError === OHUVAULT_ERRORS.LOTE_NOT_FAILABLE) {
          // Aún no evaluado → esperar y reintentar desde release
          console.log(
            `  Lote ${loteId} aún no evaluado (sigue FUNDED). ` +
              `Esperando ${config.pollIntervalMs}ms…`,
          );
          await new Promise((r) => setTimeout(r, config.pollIntervalMs));
          continue;
        }

        if (
          failResult.userError !== null &&
          FATAL_AUTH_ERRORS.includes(failResult.userError)
        ) {
          throw new Error(
            `FATAL: la llave cargada NO es admin (userError=${failResult.userError}). ` +
              `Verifica ADMIN_SECRET_KEY_PATH. Abortando.`,
          );
        }

        // Error desconocido en settle — reintentar
        console.log(
          `  Error inesperado en settle_failure lote ${loteId} (userError=${failResult.userError}). ` +
            `Reintentando en ${config.pollIntervalMs}ms…`,
        );
        await new Promise((r) => setTimeout(r, config.pollIntervalMs));
        continue;
      }

      if (
        releaseResult.userError !== null &&
        FATAL_AUTH_ERRORS.includes(releaseResult.userError)
      ) {
        throw new Error(
          `FATAL: la llave cargada NO es admin (userError=${releaseResult.userError}). ` +
            `Verifica ADMIN_SECRET_KEY_PATH. Abortando.`,
        );
      }

      // Error desconocido — reintentar
      console.log(
        `  Error inesperado en release lote ${loteId} (userError=${releaseResult.userError}). ` +
          `Reintentando en ${config.pollIntervalMs}ms…`,
      );
      await new Promise((r) => setTimeout(r, config.pollIntervalMs));
    }
  }

  console.log(`Autorizador: procesamiento completo. ${done.size}/${targets.length} lotes autorizados.`);
}

main().catch((err) => {
  console.error("Autorizador: error fatal:", err);
  process.exit(1);
});
