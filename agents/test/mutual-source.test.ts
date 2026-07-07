import { describe, it, expect } from "vitest";
import {
  parseLoteIdFromBytes,
  deriveMutualState,
  solvencyReport,
  type MutualState,
} from "../src/x402/mutual-source.js";

// "lote_id" (7 bytes) + longitud u64 (4 bytes: 8,0,0,0) + valor u64 LE + type tag
const loteIdBytes = (lote: number): number[] => {
  const le: number[] = [];
  for (let k = 0; k < 8; k++) le.push(Math.floor(lote / 2 ** (8 * k)) % 256);
  return [108, 111, 116, 101, 95, 105, 100, 8, 0, 0, 0, ...le, 5];
};

describe("parseLoteIdFromBytes", () => {
  it("extrae el lote_id del List<U8> del proxy", () => {
    expect(parseLoteIdFromBytes(loteIdBytes(4))).toBe(4);
    expect(parseLoteIdFromBytes([0, 0, ...loteIdBytes(42)])).toBe(42);
  });
  it("null si no hay lote_id", () => {
    expect(parseLoteIdFromBytes([1, 2, 3, 4])).toBeNull();
  });
});

describe("deriveMutualState", () => {
  const ep = new Map<number, string>([
    [1, "deposit_to_lote"],
    [2, "post_bond"],
    [3, "release_to_producer"],
    [4, "settle_failure"],
  ]);

  it("prima = funded × premium_bps/10000 por lote liberado", () => {
    const deploys = [
      { entry_point_id: 1, args: { amount: { parsed: "10000000000" }, args: { parsed: loteIdBytes(2) } }, block_height: 10 },
      { entry_point_id: 3, args: { lote_id: { parsed: 2 } }, block_height: 11 },
    ];
    const s = deriveMutualState(deploys, ep, { premiumBps: 50, indemnityTargetBps: 8000 });
    expect(s.premiumsCspr).toBeCloseTo(0.05, 6); // 10 CSPR × 0.5%
    expect(s.tailPaidCspr).toBe(0);
    expect(s.lotesReleased).toBe(1);
    expect(s.asOfBlock).toBe(11);
  });

  it("cola = 0 cuando bond ≥ indemnity (bond≥target)", () => {
    const deploys = [
      { entry_point_id: 1, args: { amount: { parsed: "10000000000" }, args: { parsed: loteIdBytes(1) } }, block_height: 1 },
      { entry_point_id: 2, args: { attached_value: { parsed: "10000000000" }, args: { parsed: loteIdBytes(1) } }, block_height: 2 },
      { entry_point_id: 4, args: { lote_id: { parsed: 1 } }, block_height: 3 },
    ];
    const s = deriveMutualState(deploys, ep, { premiumBps: 50, indemnityTargetBps: 8000 });
    // indemnity = 10 × 80% = 8 CSPR; bond = 10 CSPR ⇒ tail = max(0, 8 − 10) = 0
    expect(s.tailPaidCspr).toBe(0);
    expect(s.lotesFailed).toBe(1);
  });

  it("ignora deploys con error_message", () => {
    const deploys = [
      { entry_point_id: 1, args: { amount: { parsed: "10000000000" }, args: { parsed: loteIdBytes(9) } }, block_height: 1 },
      { entry_point_id: 3, args: { lote_id: { parsed: 9 } }, error_message: "Out of gas error", block_height: 2 },
    ];
    const s = deriveMutualState(deploys, ep, { premiumBps: 50, indemnityTargetBps: 8000 });
    expect(s.lotesReleased).toBe(0); // el release falló ⇒ no cuenta ⇒ sin prima
    expect(s.premiumsCspr).toBe(0);
  });
});

describe("solvencyReport", () => {
  const st = (reserve: number, tail: number): MutualState => ({
    premiumsCspr: reserve + tail,
    tailPaidCspr: tail,
    reserveCspr: reserve,
    lotesReleased: 2,
    lotesFailed: tail > 0 ? 1 : 0,
    asOfBlock: 1,
  });

  it("solvente y mantiene prima cuando reserva ≥ objetivo", () => {
    const r = solvencyReport(st(0.1, 0), 50);
    expect(r.solvent).toBe(true);
    expect(r.recommendedPremiumBps).toBe(50);
  });

  it("propone subir prima (+25 bps) cuando está bajo objetivo", () => {
    const r = solvencyReport(st(0.01, 0.1), 50);
    expect(r.solvent).toBe(false);
    expect(r.recommendedPremiumBps).toBe(75);
  });
});
