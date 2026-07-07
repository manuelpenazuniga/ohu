/**
 * Bin-packing DETERMINISTA de demandas en lotes. Agrupa por compatibilidad
 * (producto + calidad + zona + ventana). Sin LLM: es una función pura y
 * reproducible (misma entrada → mismos lotes).
 */

import type { Demand, DemandSpec, Lote } from "./types.js";

/** Clave de compatibilidad de un spec. */
export function loteKey(spec: DemandSpec): string {
  return `${spec.producto}|${spec.calidad}|${spec.zona}|${spec.ventana}`;
}

/** Agrupa demandas compatibles en lotes, ordenados de forma estable. */
export function batchDemands(demands: readonly Demand[]): Lote[] {
  const groups = new Map<
    string,
    { producto: string; calidad: string; zona: string; ventana: string; buyers: Lote["buyers"][number][] }
  >();

  for (const d of demands) {
    const key = loteKey(d.spec);
    const g =
      groups.get(key) ??
      {
        producto: d.spec.producto,
        calidad: d.spec.calidad,
        zona: d.spec.zona,
        ventana: d.spec.ventana,
        buyers: [] as Lote["buyers"][number][],
      };
    g.buyers.push({ buyerId: d.buyerId, cantidad: d.spec.cantidad, tope: d.spec.topePrecioUnitario });
    groups.set(key, g);
  }

  const lotes: Lote[] = [];
  for (const [key, g] of groups) {
    // buyers ordenados por id para reproducibilidad
    const buyers = [...g.buyers].sort((a, b) => a.buyerId.localeCompare(b.buyerId));
    lotes.push({
      key,
      producto: g.producto,
      calidad: g.calidad,
      zona: g.zona,
      ventana: g.ventana,
      cantidadTotal: buyers.reduce((s, b) => s + b.cantidad, 0),
      buyers,
    });
  }
  return lotes.sort((a, b) => a.key.localeCompare(b.key));
}
