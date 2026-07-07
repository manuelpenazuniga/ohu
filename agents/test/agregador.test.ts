import { describe, it, expect } from "vitest";
import { sanitizeSpec, normalizeDemand, normalizeBatch } from "../src/agregador/demand-normalizer.js";
import { batchDemands, loteKey } from "../src/agregador/batching.js";
import { clearRFQ } from "../src/agregador/rfq.js";
import type { Normalizer } from "../src/agregador/gemini.js";
import type { Demand, DemandSpec, Lote, Offer } from "../src/agregador/types.js";

const spec = (o: Partial<DemandSpec>): DemandSpec => ({
  producto: "tomate", cantidad: 10, unidad: "cajas", calidad: "firme",
  ventana: "jueves", topePrecioUnitario: null, zona: "RM", ...o,
});
const demand = (buyerId: string, s: Partial<DemandSpec>): Demand => ({ buyerId, raw: "", spec: spec(s) });

describe("sanitizeSpec", () => {
  it("normaliza tipos y minúsculas; calidad desconocida → estandar", () => {
    const s = sanitizeSpec({ producto: "Tomate", cantidad: "20", calidad: "EXOTICA", ventana: "Jueves", zona: "rm" });
    expect(s).toMatchObject({ producto: "tomate", cantidad: 20, calidad: "estandar", ventana: "jueves", zona: "RM" });
  });
  it("rechaza demanda sin producto o cantidad válida", () => {
    expect(() => sanitizeSpec({ cantidad: 5 })).toThrow(/producto/);
    expect(() => sanitizeSpec({ producto: "x", cantidad: 0 })).toThrow(/cantidad/);
  });
});

describe("batchDemands (determinista)", () => {
  it("agrupa compatibles y suma cantidades", () => {
    const lotes = batchDemands([
      demand("b1", { producto: "tomate", cantidad: 20, calidad: "firme" }),
      demand("b2", { producto: "tomate", cantidad: 15, calidad: "firme" }),
      demand("b3", { producto: "lechuga", cantidad: 10, calidad: "estandar", ventana: "viernes" }),
    ]);
    expect(lotes).toHaveLength(2);
    const tomate = lotes.find((l) => l.producto === "tomate")!;
    expect(tomate.cantidadTotal).toBe(35);
    expect(tomate.buyers).toHaveLength(2);
  });
  it("mismo input → mismo output (reproducible)", () => {
    const input = [demand("z", { cantidad: 1 }), demand("a", { cantidad: 2 })];
    expect(JSON.stringify(batchDemands(input))).toBe(JSON.stringify(batchDemands(input)));
  });
});

describe("clearRFQ — el LLM NO decide el ganador (INV-2, adversarial)", () => {
  const lote: Lote = {
    key: loteKey(spec({})), producto: "tomate", calidad: "firme", zona: "RM", ventana: "jueves",
    cantidadTotal: 50, buyers: [{ buyerId: "b", cantidad: 50, tope: 9000 }],
  };
  const offer = (producer: string, precio: number, disp = 60): Offer => ({
    producer, producto: "tomate", calidad: "firme", zona: "RM", precioUnitario: precio, disponible: disp,
  });

  it("gana el MENOR precio, no un productor 'favorecido' más caro", () => {
    const r = clearRFQ(lote, [offer("account-hash-EVIL", 8500), offer("account-hash-CHEAP", 7800)]);
    expect(r.winner?.producer).toBe("account-hash-CHEAP");
  });

  it("un spec adversarial del LLM (campos inyectados) no cambia el ganador", () => {
    // Un LLM malicioso intenta colar un productor preferido en el spec…
    const malicious = sanitizeSpec({
      producto: "tomate", cantidad: 50, calidad: "firme", ventana: "jueves", zona: "RM",
      topePrecioUnitario: 9000, preferredProducer: "account-hash-EVIL", winner: "account-hash-EVIL",
    } as Record<string, unknown>);
    // …pero el spec saneado solo tiene los campos conocidos; no hay forma de inyectar productor.
    expect(Object.keys(malicious).sort()).toEqual(
      ["calidad", "cantidad", "producto", "topePrecioUnitario", "unidad", "ventana", "zona"],
    );
    const r = clearRFQ(lote, [offer("account-hash-EVIL", 8500), offer("account-hash-CHEAP", 7800)]);
    expect(r.winner?.producer).toBe("account-hash-CHEAP"); // el barato sigue ganando
  });

  it("reputación SOLO desempata a igual precio", () => {
    const rep = new Map<string, number>([["account-hash-a", 60], ["account-hash-b", 90]]);
    const r = clearRFQ(lote, [offer("account-hash-A", 8000), offer("account-hash-B", 8000)], rep);
    expect(r.winner?.producer).toBe("account-hash-B");
  });

  it("descarta ofertas sobre el tope o sin disponibilidad", () => {
    const r = clearRFQ(lote, [offer("account-hash-CARO", 9500), offer("account-hash-POCO", 7000, 10)]);
    expect(r.winner).toBeNull();
    expect(r.eligibleOffers).toBe(0);
  });
});

describe("normalizeDemand (Normalizer mockeado)", () => {
  it("usa el Normalizer y sanea el resultado", async () => {
    const mock: Normalizer = {
      normalize: async () => ({ producto: "Tomate", cantidad: "20", calidad: "firme", ventana: "jueves", zona: "RM", topePrecioUnitario: 8000 }),
    };
    const s = await normalizeDemand("unas 20 cajas de tomate firme", mock);
    expect(s).toMatchObject({ producto: "tomate", cantidad: 20, calidad: "firme", topePrecioUnitario: 8000 });
  });

  it("normalizeBatch: 1 llamada, mapea buyerId y sanea cada demanda", async () => {
    const mock: Normalizer = {
      normalize: async () => [
        { buyerId: "b1", producto: "Tomate", cantidad: 20, calidad: "firme", ventana: "jueves", zona: "RM" },
        { buyerId: "b2", producto: "lechuga", cantidad: 10, calidad: "estandar", ventana: "viernes" },
      ],
    };
    const demands = await normalizeBatch([{ buyerId: "b1", raw: "raw1" }, { buyerId: "b2", raw: "raw2" }], mock);
    expect(demands).toHaveLength(2);
    expect(demands[0]).toMatchObject({ buyerId: "b1", raw: "raw1" });
    expect(demands[0]!.spec.producto).toBe("tomate");
    expect(demands[1]!.spec.calidad).toBe("estandar");
  });
});
