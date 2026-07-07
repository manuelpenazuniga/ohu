/**
 * Normaliza una demanda en lenguaje natural a un `DemandSpec` estructurado
 * usando un `Normalizer` (Gemini en producción, mock en tests). El LLM SOLO
 * normaliza; los campos se validan/saneam determinísticamente después.
 */

import type { Normalizer } from "./gemini.js";
import type { Demand, DemandSpec } from "./types.js";

const CALIDADES = ["estandar", "premium", "firme", "organico"] as const;

const SYSTEM_INSTRUCTION =
  "Eres el normalizador de demanda de un club de compra de alimentos (PyMEs). " +
  "Convierte el mensaje del comprador a un spec estructurado. Reglas: " +
  "calidad ∈ {estandar, premium, firme, organico} (usa 'estandar' si no se especifica); " +
  "zona por defecto 'RM'; cantidad y topePrecioUnitario como enteros (interpreta jerga: " +
  "'lucas'/'luca' = miles de pesos); ventana como día de semana o fecha ISO. " +
  "NO inventes datos no dados: usa null donde corresponda. Responde SOLO el JSON del schema.";

const SCHEMA = {
  type: "object",
  properties: {
    producto: { type: "string" },
    cantidad: { type: "integer" },
    unidad: { type: "string" },
    calidad: { type: "string" },
    ventana: { type: "string" },
    topePrecioUnitario: { type: "integer" },
    zona: { type: "string" },
  },
  required: ["producto", "cantidad", "calidad", "ventana"],
} as const;

interface RawSpec {
  producto?: unknown;
  cantidad?: unknown;
  unidad?: unknown;
  calidad?: unknown;
  ventana?: unknown;
  topePrecioUnitario?: unknown;
  zona?: unknown;
}

function asInt(v: unknown): number | null {
  const n = typeof v === "number" ? v : typeof v === "string" ? Number(v) : NaN;
  return Number.isFinite(n) ? Math.trunc(n) : null;
}

/** Sanea/valida el objeto del LLM a un `DemandSpec` determinista y seguro. */
export function sanitizeSpec(raw: RawSpec): DemandSpec {
  const producto = String(raw.producto ?? "").trim().toLowerCase();
  if (!producto) throw new Error("demanda sin producto");
  const cantidad = asInt(raw.cantidad);
  if (cantidad == null || cantidad <= 0) throw new Error("demanda sin cantidad válida");
  const calidadRaw = String(raw.calidad ?? "estandar").trim().toLowerCase();
  const calidad = (CALIDADES as readonly string[]).includes(calidadRaw) ? calidadRaw : "estandar";
  const ventana = String(raw.ventana ?? "").trim().toLowerCase() || "sin-fecha";
  const zona = String(raw.zona ?? "RM").trim().toUpperCase() || "RM";
  const unidad = raw.unidad != null ? String(raw.unidad).trim().toLowerCase() : null;
  const topePrecioUnitario = asInt(raw.topePrecioUnitario);
  return { producto, cantidad, unidad, calidad, ventana, topePrecioUnitario, zona };
}

/** Normaliza un texto de demanda a `DemandSpec` vía el LLM + saneo determinista. */
export async function normalizeDemand(
  text: string,
  normalizer: Normalizer,
): Promise<DemandSpec> {
  const raw = (await normalizer.normalize(text, SCHEMA, SYSTEM_INSTRUCTION)) as RawSpec;
  return sanitizeSpec(raw);
}

const BATCH_SCHEMA = {
  type: "array",
  items: {
    type: "object",
    properties: {
      buyerId: { type: "string" },
      producto: { type: "string" },
      cantidad: { type: "integer" },
      unidad: { type: "string" },
      calidad: { type: "string" },
      ventana: { type: "string" },
      topePrecioUnitario: { type: "integer" },
      zona: { type: "string" },
    },
    required: ["buyerId", "producto", "cantidad", "calidad", "ventana"],
  },
} as const;

const SYSTEM_INSTRUCTION_BATCH =
  SYSTEM_INSTRUCTION +
  " Recibes una LISTA de demandas (cada una con su buyerId). Devuelve un ARRAY " +
  "con un objeto por demanda, conservando EXACTAMENTE su buyerId.";

/**
 * Normaliza TODAS las demandas en UNA sola llamada al LLM (evita el rate-limit
 * de N requests; atómico). Devuelve `Demand[]` con el texto crudo mapeado por id.
 */
export async function normalizeBatch(
  demands: ReadonlyArray<{ buyerId: string; raw: string }>,
  normalizer: Normalizer,
): Promise<Demand[]> {
  const text =
    "Normaliza cada demanda de esta lista:\n" +
    JSON.stringify(demands.map((d) => ({ buyerId: d.buyerId, texto: d.raw })));
  const raw = (await normalizer.normalize(text, BATCH_SCHEMA, SYSTEM_INSTRUCTION_BATCH)) as ReadonlyArray<
    RawSpec & { buyerId?: unknown }
  >;
  const rawById = new Map(demands.map((d) => [d.buyerId, d.raw]));
  return raw.map((r) => {
    const buyerId = String(r.buyerId ?? "").trim();
    return { buyerId, raw: rawById.get(buyerId) ?? "", spec: sanitizeSpec(r) };
  });
}
