/**
 * Clearing DETERMINISTA del RFQ. Dada una lista de ofertas de productores, elige
 * el ganador por REGLA — **no por juicio del LLM** (INV-2):
 *   1. elegibles: mismo producto+calidad+zona, disponible ≥ cantidad total del
 *      lote, y precio ≤ al tope MÍNIMO exigido por los compradores (si alguno lo puso);
 *   2. gana el MENOR precio unitario;
 *   3. desempate: MAYOR reputación on-chain (P1-1);
 *   4. desempate final: id de productor (orden estable).
 *
 * Esta función es PURA y no recibe ningún output del LLM → estructuralmente el
 * LLM no puede favorecer a un productor. El test adversarial lo demuestra.
 */

import type { ClearingResult, Lote, Offer } from "./types.js";

/** `reputation`: account-hash normalizado → score [0..100] (de P1-1). */
export function clearRFQ(
  lote: Lote,
  offers: readonly Offer[],
  reputation: Map<string, number> = new Map(),
): ClearingResult {
  const topes = lote.buyers.map((b) => b.tope).filter((t): t is number => t != null);
  const topeMin = topes.length > 0 ? Math.min(...topes) : null;

  const eligible = offers.filter(
    (o) =>
      o.producto === lote.producto &&
      o.calidad === lote.calidad &&
      o.zona === lote.zona &&
      o.disponible >= lote.cantidadTotal &&
      (topeMin == null || o.precioUnitario <= topeMin),
  );

  const rep = (o: Offer): number => reputation.get(o.producer.toLowerCase()) ?? 50;

  const ranked = [...eligible].sort(
    (a, b) =>
      a.precioUnitario - b.precioUnitario || // menor precio primero
      rep(b) - rep(a) || // mayor reputación primero
      a.producer.localeCompare(b.producer), // estable
  );

  const winner = ranked[0] ?? null;
  const reason =
    winner == null
      ? eligible.length === 0
        ? "sin ofertas elegibles (spec/tope/disponibilidad)"
        : "sin ganador"
      : `mejor precio ${winner.precioUnitario}` +
        (topeMin != null ? ` (≤ tope ${topeMin})` : "") +
        `, reputación ${rep(winner)}`;

  return {
    loteKey: lote.key,
    winner,
    reason,
    consideredOffers: offers.length,
    eligibleOffers: eligible.length,
  };
}
