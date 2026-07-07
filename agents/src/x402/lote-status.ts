/**
 * Estado de los lotes derivado del historial on-chain (CSPR.cloud). Para el MCP
 * server (F9) y cualquier lector read-only. El estado es la transición más
 * avanzada que el lote alcanzó (con tx exitosa):
 *   open_lote → OPEN · lock_lote → FUNDED · evaluate_lote → EVAL ·
 *   release_to_producer → SETTLED_OK · settle_failure → SETTLED_FAIL
 */

import { allDeploys, contractHash, entryPointMap, type CsprCloudConfig, type CcDeploy } from "./cspr-cloud.js";

export type LoteState = "OPEN" | "FUNDED" | "EVAL" | "SETTLED_OK" | "SETTLED_FAIL" | "UNKNOWN";

export interface LoteStatus {
  readonly loteId: number;
  readonly state: LoteState;
  readonly producer: string | null;
}

const RANK: Record<Exclude<LoteState, "UNKNOWN">, number> = {
  OPEN: 1, FUNDED: 2, EVAL: 3, SETTLED_OK: 4, SETTLED_FAIL: 4,
};

const ENTRY_STATE: Record<string, Exclude<LoteState, "UNKNOWN">> = {
  open_lote: "OPEN",
  lock_lote: "FUNDED",
  evaluate_lote: "EVAL",
  release_to_producer: "SETTLED_OK",
  settle_failure: "SETTLED_FAIL",
};

function loteIdOf(x: CcDeploy): number | null {
  const direct = (x.args ?? {})["lote_id"]?.parsed;
  return typeof direct === "number" ? direct : null;
}

export async function loteStatuses(cfg: CsprCloudConfig): Promise<Map<number, LoteStatus>> {
  const ch = await contractHash(cfg);
  const [epMap, deploys] = await Promise.all([entryPointMap(cfg, ch), allDeploys(cfg)]);

  const states = new Map<number, Exclude<LoteState, "UNKNOWN">>();
  const producers = new Map<number, string>();

  for (const x of deploys) {
    if (x.error_message) continue;
    const name = x.entry_point_id != null ? epMap.get(x.entry_point_id) : undefined;
    if (!name) continue;
    const lote = loteIdOf(x);
    if (lote == null) continue;
    if (name === "open_lote") {
      const prod = (x.args ?? {})["producer"]?.parsed;
      if (typeof prod === "string") producers.set(lote, prod.replace(/^account-hash-/i, ""));
    }
    const st = ENTRY_STATE[name];
    if (!st) continue;
    const prev = states.get(lote);
    if (!prev || RANK[st] >= RANK[prev]) states.set(lote, st);
  }

  const out = new Map<number, LoteStatus>();
  for (const [loteId, state] of states) {
    out.set(loteId, { loteId, state, producer: producers.get(loteId) ?? null });
  }
  return out;
}

export async function loteStatus(cfg: CsprCloudConfig, loteId: number): Promise<LoteStatus> {
  const all = await loteStatuses(cfg);
  return all.get(loteId) ?? { loteId, state: "UNKNOWN", producer: null };
}

export async function openLotes(cfg: CsprCloudConfig): Promise<LoteStatus[]> {
  const all = await loteStatuses(cfg);
  return [...all.values()].filter((l) => l.state === "OPEN").sort((a, b) => a.loteId - b.loteId);
}
