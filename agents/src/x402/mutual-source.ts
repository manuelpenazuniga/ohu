/**
 * Estado de solvencia de la **MutualPool**, derivado DETERMINÍSTICAMENTE del
 * historial on-chain (CSPR.cloud) con las fórmulas del contrato:
 *   - prima de un lote liberado = `funded × premium_bps / 10000`  (entra al pool)
 *   - cola de un lote fallido    = `max(0, funded × indemnity_bps/10000 − bond)` (sale del pool)
 * `bond ≥ target` se exige en `lock_lote`, por lo que la cola es 0 en la práctica
 * (el que falla paga primero desde su bono). El agente Mutual/Riesgo (P1-4) usa
 * esto para un informe de solvencia + una recomendación de prima (no ejecuta).
 *
 * Nota: comparte el patrón de fetch con `reputation-source.ts`; los helpers de
 * CSPR.cloud se mantienen locales para no acoplar los dos servicios.
 * TODO(refactor): extraer los helpers CSPR.cloud a un `cspr-cloud.ts` compartido.
 */

import type { CsprCloudConfig } from "./reputation-source.js";

export interface MutualParams {
  readonly premiumBps: number;
  readonly indemnityTargetBps: number;
}

export interface MutualState {
  /** CSPR de primas cobradas (Σ sobre lotes liberados). */
  readonly premiumsCspr: number;
  /** CSPR de cola pagada por el pool (Σ sobre lotes fallidos con déficit). */
  readonly tailPaidCspr: number;
  /** Reserva neta = primas − cola. */
  readonly reserveCspr: number;
  readonly lotesReleased: number;
  readonly lotesFailed: number;
  readonly asOfBlock: number;
}

interface CcArg {
  readonly parsed?: unknown;
}
interface CcDeploy {
  readonly entry_point_id?: number;
  readonly args?: Record<string, CcArg>;
  readonly error_message?: string | null;
  readonly block_height?: number;
}
interface CcPage<T> {
  readonly data: readonly T[];
  readonly page_count: number;
}

const MOTES_PER_CSPR = 1_000_000_000;

async function ccGet<T>(cfg: CsprCloudConfig, path: string): Promise<T> {
  const res = await fetch(`${cfg.apiUrl}${path}`, { headers: { authorization: cfg.apiKey } });
  if (!res.ok) throw new Error(`CSPR.cloud ${res.status} en ${path}`);
  return (await res.json()) as T;
}

async function contractHash(cfg: CsprCloudConfig): Promise<string> {
  const d = await ccGet<CcPage<{ contract_hash: string }>>(
    cfg,
    `/contract-packages/${cfg.packageHash}/contracts?page_size=1`,
  );
  const h = d.data[0]?.contract_hash;
  if (!h) throw new Error("CSPR.cloud: contrato del paquete no encontrado");
  return h;
}

async function entryPointMap(cfg: CsprCloudConfig, ch: string): Promise<Map<number, string>> {
  const d = await ccGet<CcPage<{ entry_point?: { id: number; name: string }; id?: number; name?: string }>>(
    cfg,
    `/contracts/${ch}/entry-points?page_size=100`,
  );
  const m = new Map<number, string>();
  for (const e of d.data) {
    const x = e.entry_point ?? (e as { id: number; name: string });
    if (typeof x.id === "number" && x.name) m.set(x.id, x.name);
  }
  return m;
}

async function allDeploys(cfg: CsprCloudConfig): Promise<CcDeploy[]> {
  const out: CcDeploy[] = [];
  for (let page = 1; page <= 40; page++) {
    const d = await ccGet<CcPage<CcDeploy>>(
      cfg,
      `/deploys?contract_package_hash=${cfg.packageHash}&page_size=50&page=${page}`,
    );
    out.push(...d.data);
    if (page >= d.page_count) break;
  }
  return out;
}

/**
 * Extrae `lote_id` (u64) de los runtime-args serializados de una llamada por
 * proxy (`deposit_to_lote` / `post_bond` transfieren tokens → sus args reales
 * viajan como `List<U8>`). Busca el nombre "lote_id" y lee el u64 LE que sigue
 * a su longitud (4 bytes). Devuelve `null` si no lo encuentra.
 */
export function parseLoteIdFromBytes(bytes: readonly number[]): number | null {
  const needle = [108, 111, 116, 101, 95, 105, 100]; // "lote_id"
  for (let i = 0; i + needle.length + 4 + 8 <= bytes.length; i++) {
    let hit = true;
    for (let j = 0; j < needle.length; j++) {
      if (bytes[i + j] !== needle[j]) { hit = false; break; }
    }
    if (!hit) continue;
    const v = i + needle.length + 4; // saltar el nombre + los 4 bytes de longitud
    let lote = 0;
    for (let k = 0; k < 8; k++) lote += (bytes[v + k] ?? 0) * 2 ** (8 * k);
    return lote;
  }
  return null;
}

/** lote_id de un deploy: directo en args (llamada normal) o en el List<U8> (proxy). */
function loteIdOf(x: CcDeploy): number | null {
  const args = x.args ?? {};
  const direct = args["lote_id"]?.parsed;
  if (typeof direct === "number") return direct;
  const inner = args["args"]?.parsed;
  if (Array.isArray(inner)) return parseLoteIdFromBytes(inner as number[]);
  return null;
}

/** Monto (motes) de una llamada por proxy: `amount`/`attached_value` top-level. */
function amountMotesOf(x: CcDeploy): number {
  const raw = (x.args ?? {})["amount"]?.parsed ?? (x.args ?? {})["attached_value"]?.parsed;
  const n = typeof raw === "string" ? Number(raw) : typeof raw === "number" ? raw : 0;
  return Number.isFinite(n) ? n : 0;
}

export function deriveMutualState(
  deploys: readonly CcDeploy[],
  epMap: Map<number, string>,
  params: MutualParams,
): MutualState {
  const funded = new Map<number, number>(); // lote → motes depositados
  const bond = new Map<number, number>(); // lote → motes de bono
  const released = new Set<number>();
  const failed = new Set<number>();
  let asOfBlock = 0;

  for (const x of deploys) {
    const name = x.entry_point_id != null ? epMap.get(x.entry_point_id) : undefined;
    if (typeof x.block_height === "number") asOfBlock = Math.max(asOfBlock, x.block_height);
    if (x.error_message) continue;
    const lote = loteIdOf(x);
    if (lote == null) continue;
    if (name === "deposit_to_lote") funded.set(lote, (funded.get(lote) ?? 0) + amountMotesOf(x));
    else if (name === "post_bond") bond.set(lote, (bond.get(lote) ?? 0) + amountMotesOf(x));
    else if (name === "release_to_producer") released.add(lote);
    else if (name === "settle_failure") failed.add(lote);
  }

  let premiumsMotes = 0;
  for (const lote of released) {
    premiumsMotes += Math.floor(((funded.get(lote) ?? 0) * params.premiumBps) / 10000);
  }
  let tailMotes = 0;
  for (const lote of failed) {
    const indemnity = Math.floor(((funded.get(lote) ?? 0) * params.indemnityTargetBps) / 10000);
    tailMotes += Math.max(0, indemnity - (bond.get(lote) ?? 0));
  }

  const premiumsCspr = premiumsMotes / MOTES_PER_CSPR;
  const tailPaidCspr = tailMotes / MOTES_PER_CSPR;
  return {
    premiumsCspr,
    tailPaidCspr,
    reserveCspr: premiumsCspr - tailPaidCspr,
    lotesReleased: released.size,
    lotesFailed: failed.size,
    asOfBlock,
  };
}

export interface SolvencyReport {
  readonly state: MutualState;
  /** Objetivo de reserva (≥1.5× cola pagada; piso simbólico si aún es 0). */
  readonly targetCspr: number;
  /** reserva / objetivo. */
  readonly ratio: number;
  readonly solvent: boolean;
  /** Recomendación de prima (bps) como PROPUESTA de gobernanza — no se ejecuta. */
  readonly recommendedPremiumBps: number;
  readonly narrative: string;
}

/**
 * Informe de solvencia + recomendación de prima (propuesta, no ejecución).
 * Objetivo = 1.5× la cola pagada histórica (ohu.md §4.7); con cola 0 el objetivo
 * es un piso simbólico y la reserva de primas lo cubre.
 */
export function solvencyReport(state: MutualState, currentPremiumBps: number): SolvencyReport {
  const targetCspr = Math.max(1.5 * state.tailPaidCspr, 0.01);
  const ratio = targetCspr > 0 ? state.reserveCspr / targetCspr : Infinity;
  const solvent = state.reserveCspr >= targetCspr;
  // Sin cola pagada (bond≥target funcionando), la prima puede mantenerse; si la
  // reserva no cubre el objetivo, se propone subirla (tope 200 bps).
  const recommendedPremiumBps = solvent
    ? currentPremiumBps
    : Math.min(currentPremiumBps + 25, 200);
  const narrative =
    `Reserva ${state.reserveCspr.toFixed(4)} CSPR de ${state.lotesReleased} lote(s) liberados; ` +
    `cola pagada ${state.tailPaidCspr.toFixed(4)} CSPR sobre ${state.lotesFailed} fallo(s) ` +
    `(bond ≥ target ⇒ el que falla paga primero). Objetivo ${targetCspr.toFixed(4)} CSPR, ` +
    `ratio ${Number.isFinite(ratio) ? ratio.toFixed(2) : "∞"} ⇒ ${solvent ? "SOLVENTE" : "bajo objetivo"}. ` +
    `Prima recomendada: ${recommendedPremiumBps} bps (propuesta de gobernanza; la ejecuta el admin).`;
  return { state, targetCspr, ratio, solvent, recommendedPremiumBps, narrative };
}

/** Carga el estado + informe de la mutual desde CSPR.cloud. */
export async function mutualReport(
  cfg: CsprCloudConfig,
  params: MutualParams,
): Promise<SolvencyReport> {
  const ch = await contractHash(cfg);
  const [epMap, deploys] = await Promise.all([entryPointMap(cfg, ch), allDeploys(cfg)]);
  const state = deriveMutualState(deploys, epMap, params);
  return solvencyReport(state, params.premiumBps);
}
