/** Tipos del Agregador (P1-3): demanda NL → spec → lote → RFQ → clearing. */

/** Spec estructurado de una demanda (salida del LLM — SOLO normalización). */
export interface DemandSpec {
  readonly producto: string;
  readonly cantidad: number;
  readonly unidad: string | null;
  /** estandar | premium | firme | organico */
  readonly calidad: string;
  /** fecha ISO o día de la semana */
  readonly ventana: string;
  /** tope de precio por unidad (motes/CSPR según el mercado), o null */
  readonly topePrecioUnitario: number | null;
  readonly zona: string;
}

/** Demanda de un comprador: texto crudo + spec normalizado. */
export interface Demand {
  readonly buyerId: string;
  readonly raw: string;
  readonly spec: DemandSpec;
}

/** Lote agregado: demandas compatibles (mismo producto+calidad+zona+ventana). */
export interface Lote {
  /** Clave de compatibilidad: producto|calidad|zona|ventana. */
  readonly key: string;
  readonly producto: string;
  readonly calidad: string;
  readonly zona: string;
  readonly ventana: string;
  readonly cantidadTotal: number;
  readonly buyers: ReadonlyArray<{
    readonly buyerId: string;
    readonly cantidad: number;
    readonly tope: number | null;
  }>;
}

/** Oferta de un productor a un lote (off-chain, RFQ). */
export interface Offer {
  readonly producer: string; // account-hash
  readonly producto: string;
  readonly calidad: string;
  readonly zona: string;
  readonly precioUnitario: number;
  readonly disponible: number;
}

/** Resultado del clearing determinista de un lote. */
export interface ClearingResult {
  readonly loteKey: string;
  readonly winner: Offer | null;
  readonly reason: string;
  readonly consideredOffers: number;
  readonly eligibleOffers: number;
}
