/**
 * Fuente REAL de reputación: deriva el historial on-chain de cada productor del
 * contrato `OhuVault`, leído vía **CSPR.cloud** (endpoints `/deploys` +
 * `/entry-points`). No mantiene índice propio:
 *   - `open_lote(lote_id, producer)`  → mapea lote → productor (arg `producer`)
 *   - `release_to_producer(lote_id)`  → ese lote liquidó OK
 *   - `settle_failure(lote_id)`       → ese lote falló (bono slasheado)
 * `post_bond` NO sirve como fuente: va por proxy (transfiere tokens) y su caller
 * no es el productor. Con caché TTL para no pegar a CSPR.cloud en cada request.
 */

export interface CsprCloudConfig {
  /** Base URL, p.ej. `https://api.testnet.cspr.cloud`. */
  readonly apiUrl: string;
  readonly apiKey: string;
  /** Package hash del OhuVault (64 hex, sin prefijo `hash-`). */
  readonly packageHash: string;
}

export interface ProducerHistory {
  readonly lotesAwarded: number;
  readonly settledOk: number;
  readonly settledFail: number;
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
interface CcEntryPoint {
  readonly id: number;
  readonly name: string;
}
interface CcPage<T> {
  readonly data: readonly T[];
  readonly page_count: number;
}

async function cc<T>(cfg: CsprCloudConfig, path: string): Promise<T> {
  const res = await fetch(`${cfg.apiUrl}${path}`, {
    headers: { authorization: cfg.apiKey },
  });
  if (!res.ok) {
    throw new Error(`CSPR.cloud ${res.status} en ${path}`);
  }
  return (await res.json()) as T;
}

/** Contract hash (versión más reciente) del paquete del OhuVault. */
async function contractHash(cfg: CsprCloudConfig): Promise<string> {
  const d = await cc<CcPage<{ contract_hash: string }>>(
    cfg,
    `/contract-packages/${cfg.packageHash}/contracts?page_size=1`,
  );
  const hash = d.data[0]?.contract_hash;
  if (!hash) throw new Error("CSPR.cloud: contrato del paquete no encontrado");
  return hash;
}

/** Mapa entry_point_id → name del contrato. */
async function entryPointMap(
  cfg: CsprCloudConfig,
  ch: string,
): Promise<Map<number, string>> {
  const d = await cc<CcPage<{ entry_point?: CcEntryPoint } & Partial<CcEntryPoint>>>(
    cfg,
    `/contracts/${ch}/entry-points?page_size=100`,
  );
  const m = new Map<number, string>();
  for (const e of d.data) {
    const x = e.entry_point ?? (e as CcEntryPoint);
    if (typeof x.id === "number" && x.name) m.set(x.id, x.name);
  }
  return m;
}

/** Todos los deploys del paquete (paginados; backstop de 40 páginas). */
async function allDeploys(cfg: CsprCloudConfig): Promise<CcDeploy[]> {
  const out: CcDeploy[] = [];
  for (let page = 1; page <= 40; page++) {
    const d = await cc<CcPage<CcDeploy>>(
      cfg,
      `/deploys?contract_package_hash=${cfg.packageHash}&page_size=50&page=${page}`,
    );
    out.push(...d.data);
    if (page >= d.page_count) break;
  }
  return out;
}

/** Normaliza una key de cuenta a hex crudo (sin prefijo `account-hash-`). */
export function normalizeProducer(p: string): string {
  return p.trim().replace(/^account-hash-/i, "").toLowerCase();
}

function derive(
  deploys: readonly CcDeploy[],
  epMap: Map<number, string>,
): Map<string, ProducerHistory> {
  const loteProducer = new Map<number, string>();
  const result = new Map<number, "OK" | "FAIL">();
  let asOfBlock = 0;

  for (const x of deploys) {
    const name = x.entry_point_id != null ? epMap.get(x.entry_point_id) : undefined;
    const args = x.args ?? {};
    const lidRaw = args["lote_id"]?.parsed;
    const lid = typeof lidRaw === "number" ? lidRaw : undefined;
    if (typeof x.block_height === "number") asOfBlock = Math.max(asOfBlock, x.block_height);
    if (x.error_message || lid == null) continue;

    if (name === "open_lote") {
      const prod = args["producer"]?.parsed;
      if (typeof prod === "string") loteProducer.set(lid, normalizeProducer(prod));
    } else if (name === "release_to_producer") {
      result.set(lid, "OK");
    } else if (name === "settle_failure") {
      result.set(lid, "FAIL");
    }
  }

  const acc = new Map<string, { lotesAwarded: number; settledOk: number; settledFail: number }>();
  for (const [lid, prod] of loteProducer) {
    const r = acc.get(prod) ?? { lotesAwarded: 0, settledOk: 0, settledFail: 0 };
    r.lotesAwarded += 1;
    const res = result.get(lid);
    if (res === "OK") r.settledOk += 1;
    else if (res === "FAIL") r.settledFail += 1;
    acc.set(prod, r);
  }

  const out = new Map<string, ProducerHistory>();
  for (const [prod, r] of acc) out.set(prod, { ...r, asOfBlock });
  return out;
}

/**
 * Score paramétrico [0..100] a partir del historial: base 50, +50 ponderado por
 * la tasa de éxito de lotes liquidados, −30 por la de fallos. Sin liquidaciones
 * aún → 50 (neutral).
 */
export function scoreFor(h: ProducerHistory): number {
  const settled = h.settledOk + h.settledFail;
  if (settled === 0) return 50;
  const s = 50 + 50 * (h.settledOk / settled) - 30 * (h.settledFail / settled);
  return Math.max(0, Math.min(100, Math.round(s)));
}

let cache: { at: number; map: Map<string, ProducerHistory> } | null = null;
const TTL_MS = 30_000;

/**
 * Historial de reputación de TODOS los productores (mapa por productor
 * normalizado), con caché TTL. `nowMs` se inyecta (testeable).
 */
export async function reputationHistory(
  cfg: CsprCloudConfig,
  nowMs: number,
): Promise<Map<string, ProducerHistory>> {
  if (cache && nowMs - cache.at < TTL_MS) return cache.map;
  const ch = await contractHash(cfg);
  const [epMap, deploys] = await Promise.all([entryPointMap(cfg, ch), allDeploys(cfg)]);
  const map = derive(deploys, epMap);
  cache = { at: nowMs, map };
  return map;
}

/** Solo-para-tests: limpia la caché. */
export function _resetCache(): void {
  cache = null;
}

/**
 * Construye la config de CSPR.cloud desde el entorno. Devuelve `undefined` si
 * falta `CSPRCLOUD_API_KEY` o `OHUVAULT_PACKAGE_HASH` (el server cae a seed).
 */
export function loadCsprCloudConfig(
  env: Record<string, string | undefined> = process.env,
): CsprCloudConfig | undefined {
  const apiKey = env["CSPRCLOUD_API_KEY"]?.trim();
  const packageHashRaw = env["OHUVAULT_PACKAGE_HASH"]?.trim();
  if (!apiKey || !packageHashRaw) return undefined;
  const apiUrl = env["CSPRCLOUD_API_URL"]?.trim() || "https://api.testnet.cspr.cloud";
  return { apiUrl, apiKey, packageHash: packageHashRaw.replace(/^hash-/i, "") };
}
