/**
 * Helpers compartidos de lectura de CSPR.cloud (deploys + entry-points de un
 * contrato). Usados por los lectores derivados on-chain (lote-status, y a futuro
 * reputation/mutual, que aún llevan su propia copia).
 */

import type { CsprCloudConfig } from "./reputation-source.js";
export type { CsprCloudConfig };

export interface CcDeploy {
  readonly entry_point_id?: number;
  readonly args?: Record<string, { readonly parsed?: unknown }>;
  readonly error_message?: string | null;
  readonly block_height?: number;
}
interface CcPage<T> {
  readonly data: readonly T[];
  readonly page_count: number;
}

export async function ccGet<T>(cfg: CsprCloudConfig, path: string): Promise<T> {
  const res = await fetch(`${cfg.apiUrl}${path}`, { headers: { authorization: cfg.apiKey } });
  if (!res.ok) throw new Error(`CSPR.cloud ${res.status} en ${path}`);
  return (await res.json()) as T;
}

export async function contractHash(cfg: CsprCloudConfig): Promise<string> {
  const d = await ccGet<CcPage<{ contract_hash: string }>>(
    cfg,
    `/contract-packages/${cfg.packageHash}/contracts?page_size=1`,
  );
  const h = d.data[0]?.contract_hash;
  if (!h) throw new Error("CSPR.cloud: contrato del paquete no encontrado");
  return h;
}

export async function entryPointMap(cfg: CsprCloudConfig, ch: string): Promise<Map<number, string>> {
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

export async function allDeploys(cfg: CsprCloudConfig): Promise<CcDeploy[]> {
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
