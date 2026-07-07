import { appendFileSync, mkdirSync, existsSync } from "node:fs";
import { dirname } from "node:path";

/**
 * Roles del enjambre para el logger bicolumna.
 *
 * - `operator`: Tesorería — acciona `evaluate_lote`, columna PROPONE.
 * - `admin`: Autorizador — acciona `release_to_producer` / `settle_failure`,
 *   columna AUTORIZA.
 */
export type SwarmRole = "operator" | "admin";

/** Columnas visuales del pitch INV-2. */
export type SwarmColumn = "PROPONE" | "AUTORIZA";

/**
 * Entrada del log estructurado (una línea JSONL por evento).
 * Se emite a stdout y se anexa al archivo de log.
 */
export interface SwarmLogEntry {
  /** Timestamp ISO-8601. */
  readonly ts: string;
  /** Rol del agente que genera el evento. */
  readonly role: SwarmRole;
  /** Columna visual: PROPONE (operator) o AUTORIZA (admin). */
  readonly column: SwarmColumn;
  /** Cuenta-hash del agente (para trazabilidad de identidad). */
  readonly agentAccount: string;
  /** Entrypoint llamado en el OhuVault. */
  readonly entrypoint: string;
  /** ID del lote objetivo. */
  readonly loteId: number;
  /** Hash de la transacción. */
  readonly txHash: string;
  /** Resultado resumido (ej. "EVAL_OK", "EVAL_FAIL", "SETTLED_OK", "SKIP_ALREADY_EVALUATED"). */
  readonly result: string;
  /** Narrativa opcional generada por LLM (solo si `--narrate` activo). */
  readonly narrative?: string;
}

/** Logger compartido para el enjambre. */
export interface SwarmLogger {
  /** Escribe una entrada de log (stdout + archivo). */
  log(entry: SwarmLogEntry): void;
  /** Devuelve la ruta del archivo de log. */
  readonly logFile: string;
}

/**
 * Crea un logger estructurado bicolumna.
 *
 * Emite cada evento como:
 * 1. Una línea JSONL a stdout.
 * 2. La misma línea anexada al archivo `logFile`.
 * 3. Una línea humana legible con columnas alineadas (pitch visual INV-2).
 *
 * @param logFile Ruta absoluta o relativa al archivo `.swarm-log.jsonl`.
 */
export function createSwarmLogger(logFile: string): SwarmLogger {
  return {
    logFile,
    log(entry: SwarmLogEntry): void {
      const jsonl = JSON.stringify(entry);

      // 1. stdout
      console.log(jsonl);

      // 2. Archivo
      const dir = dirname(logFile);
      if (!existsSync(dir)) {
        mkdirSync(dir, { recursive: true });
      }
      appendFileSync(logFile, jsonl + "\n", "utf8");

      // 3. Línea humana bicolumna
      const leftCol =
        entry.column === "PROPONE"
          ? `[${entry.ts}] PROPONE  │ lote=${entry.loteId} ${entry.entrypoint} → ${entry.result}  tx=${entry.txHash.slice(0, 12)}…`
          : "";
      const rightCol =
        entry.column === "AUTORIZA"
          ? `[${entry.ts}] AUTORIZA │ lote=${entry.loteId} ${entry.entrypoint} → ${entry.result}  tx=${entry.txHash.slice(0, 12)}…`
          : "";
      const line = leftCol.padEnd(80) + (rightCol ? rightCol : "");
      console.log(line.trimEnd());
    },
  };
}
