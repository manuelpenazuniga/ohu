import { describe, it, expect } from "vitest";
import { FailoverFacilitatorClient } from "../src/x402/failover-client.js";
import {
  makeBrokenFacilitator,
  makeMockFacilitator,
  makeTestConfig,
} from "./x402-fixtures.js";
import type { FacilitatorClient } from "@x402/core/server";
import type {
  PaymentPayload,
  PaymentRequirements,
  SettleResponse,
} from "@x402/core/types";

/**
 * Invariante de auditoría S4 / S4-b:
 *  - S4:   si el facilitator hosteado cae, `verify`/`getSupported` siguen
 *          sirviéndose vía el facilitator **local** (fallback).
 *  - S4-b: `settle` **NO** reintenta en el fallback — evita un doble pago si el
 *          primario ya liquidó on-chain y muere devolviendo la respuesta.
 *          El error se propaga etiquetado para reintento idempotente arriba.
 */
describe("FailoverFacilitatorClient", () => {
  describe("verify / getSupported — fallback local cuando el hosteado falla", () => {
    it("verify NO llama al fallback si el primario responde", async () => {
      const cfg = makeTestConfig();
      const primary = makeMockFacilitator(cfg);
      let fallbackCalled = false;
      const fallback = makeMockFacilitator(cfg);
      const trackingFallback: FacilitatorClient = {
        verify: async (...args) => {
          fallbackCalled = true;
          return fallback.verify(...(args as [PaymentPayload, PaymentRequirements]));
        },
        settle: async (...args) => fallback.settle(...(args as [PaymentPayload, PaymentRequirements])),
        getSupported: async () => fallback.getSupported(),
      };
      const client = new FailoverFacilitatorClient(primary, trackingFallback);
      await client.verify({} as never, {} as never);
      expect(fallbackCalled).toBe(false);
    });

    it("getSupported resuelve desde el fallback si el primario no responde", async () => {
      const cfg = makeTestConfig();
      const client = new FailoverFacilitatorClient(
        makeBrokenFacilitator("no-supported"),
        makeMockFacilitator(cfg),
      );
      const supported = await client.getSupported();
      expect(supported.kinds[0]?.scheme).toBe("exact");
      expect(supported.kinds[0]?.network).toBe("casper:casper-test");
    });

    it("verify/getSupported: si ambos fallan, lanza explicando qué pasó en primario y fallback", async () => {
      const client = new FailoverFacilitatorClient(
        makeBrokenFacilitator("primary"),
        makeBrokenFacilitator("fallback"),
      );
      await expect(client.verify({} as never, {} as never)).rejects.toThrow(
        /primario falló.*fallback local también/,
      );
      await expect(client.getSupported()).rejects.toThrow(/primario falló.*fallback local también/);
    });

    it("cuando el primario responde, NO toca el fallback", async () => {
      const cfg = makeTestConfig();
      let fallbackCalled = false;
      const fallback = makeMockFacilitator(cfg);
      const trackingFallback = new Proxy(fallback, {
        get(target, prop) {
          const orig = Reflect.get(target, prop);
          if (typeof orig === "function") {
            return (...args: unknown[]) => {
              fallbackCalled = true;
              return (orig as Function).apply(target, args);
            };
          }
          return orig;
        },
      });
      const primary = makeMockFacilitator(cfg);
      const client = new FailoverFacilitatorClient(primary, trackingFallback);
      await client.getSupported();
      expect(fallbackCalled).toBe(false);
    });
  });

  describe("S4-b — settle NO reintenta en el fallback (anti-doble-pago)", () => {
    /**
     * Caso crítico: el primario ya envió el deploy on-chain y falla al
     * devolver (timeout/crash/5xx tardío). El fallback **no debe** verse — un
     * segundo settle con la misma autorización podría duplicar el pago si el
     * contrato CEP-18 asset no tracks `used_nonces`. El error se propaga
     * etiquetado `(settle)`.
     */
    it("primario lanza tras enviar → fallback NO es llamado → no hay doble settle", async () => {
      const cfg = makeTestConfig();

      let fallbackSettleCalls = 0;
      const fallback = makeMockFacilitator(cfg);
      const trackingFallback: FacilitatorClient = {
        verify: ((...args: unknown[]) => fallback.verify(...(args as [PaymentPayload, PaymentRequirements]))) as FacilitatorClient["verify"],
        settle: ((...args: unknown[]) => {
          fallbackSettleCalls += 1;
          return fallback.settle(...(args as [PaymentPayload, PaymentRequirements]));
        }) as FacilitatorClient["settle"],
        getSupported: () => fallback.getSupported(),
      };

      // Primario emite el deploy y luego rompe (simula timeout tras envío).
      const primarySettleErr = new Error("hosted settle: response timeout after putTransaction");
      const primary: FacilitatorClient = {
        verify: (() => Promise.reject(new Error("primary-verify-broken"))) as FacilitatorClient["verify"],
        settle: (() => Promise.reject(primarySettleErr)) as FacilitatorClient["settle"],
        getSupported: (() => Promise.reject(new Error("primary-supported-broken"))) as FacilitatorClient["getSupported"],
      };

      const client = new FailoverFacilitatorClient(primary, trackingFallback);
      const payload = {} as PaymentPayload;
      const reqs = {} as PaymentRequirements;

      await expect(client.settle(payload, reqs)).rejects.toThrow(/FailoverFacilitatorClient\(settle\)/);
      await expect(client.settle(payload, reqs)).rejects.toThrow(
        /NO se reintenta en el fallback/,
      );
      // El mensaje original del primario debe.preservarse para diagnóstico.
      await expect(client.settle(payload, reqs)).rejects.toThrow(
        /response timeout after putTransaction/,
      );
      expect(fallbackSettleCalls).toBe(0);
    });

    it("settle exitoso en el primario → NO toca el fallback", async () => {
      const cfg = makeTestConfig();
      const expected: SettleResponse = {
        success: true,
        transaction: "deploy-hash-cep18-transfer-with-authorization",
        network: cfg.chainID,
        amount: "1000000",
        extra: { entryPoint: "transfer_with_authorization", token: "CEP-18" },
      };
      let fallbackSettleCalls = 0;
      const fallback = makeMockFacilitator(cfg);
      const trackingFallback: FacilitatorClient = {
        verify: ((...args: unknown[]) => fallback.verify(...(args as [PaymentPayload, PaymentRequirements]))) as FacilitatorClient["verify"],
        settle: ((...args: unknown[]) => {
          fallbackSettleCalls += 1;
          return fallback.settle(...(args as [PaymentPayload, PaymentRequirements]));
        }) as FacilitatorClient["settle"],
        getSupported: () => fallback.getSupported(),
      };
      const client = new FailoverFacilitatorClient(makeMockFacilitator(cfg), trackingFallback);

      const res = await client.settle({} as never, {} as never);
      expect(res).toEqual(expected);
      expect(fallbackSettleCalls).toBe(0);
    });

    it("aunque ambos facilitators estén sanos, settle del primario NO reintenta en fallback", async () => {
      // Resistencia a una tentación de "fallback como hot-spare para settle":
      // incluso con un fallback válido, no se toca para settle cuando el
      // primario lanza. Verifica el invariante estructural.
      const cfg = makeTestConfig();
      let fallbackSettleCalls = 0;
      const fallback = makeMockFacilitator(cfg);
      const trackingFallback: FacilitatorClient = {
        verify: ((...args: unknown[]) => fallback.verify(...(args as [PaymentPayload, PaymentRequirements]))) as FacilitatorClient["verify"],
        settle: ((...args: unknown[]) => {
          fallbackSettleCalls += 1;
          return fallback.settle(...(args as [PaymentPayload, PaymentRequirements]));
        }) as FacilitatorClient["settle"],
        getSupported: () => fallback.getSupported(),
      };
      const primary: FacilitatorClient = {
        verify: ((...args: unknown[]) => Promise.reject(new Error("v"))) as FacilitatorClient["verify"],
        settle: ((...args: unknown[]) => Promise.reject(new Error("s"))) as FacilitatorClient["settle"],
        getSupported: (() => Promise.reject(new Error("g"))) as FacilitatorClient["getSupported"],
      };
      const client = new FailoverFacilitatorClient(primary, trackingFallback);

      await expect(client.settle({} as never, {} as never)).rejects.toThrow();
      // verify y getSupported SÍ hacen failover (uso legítimo del fallback):
      expect((await client.verify({} as never, {} as never)).isValid).toBe(true);
      expect((await client.getSupported()).kinds[0]?.scheme).toBe("exact");
      expect(fallbackSettleCalls).toBe(0);
    });

    it("verify del fallback sí se invoca cuando el primario lanza (failover legítimo)", async () => {
      const cfg = makeTestConfig();
      let fallbackVerifyCalls = 0;
      const fallback = makeMockFacilitator(cfg);
      const trackingFallback: FacilitatorClient = {
        verify: ((...args: unknown[]) => {
          fallbackVerifyCalls += 1;
          return fallback.verify(...(args as [PaymentPayload, PaymentRequirements]));
        }) as FacilitatorClient["verify"],
        settle: ((...args: unknown[]) => fallback.settle(...(args as [PaymentPayload, PaymentRequirements]))) as FacilitatorClient["settle"],
        getSupported: () => fallback.getSupported(),
      };
      const client = new FailoverFacilitatorClient(makeBrokenFacilitator("v"), trackingFallback);

      const r = await client.verify({} as never, {} as never);
      expect(r.isValid).toBe(true);
      expect(fallbackVerifyCalls).toBe(1);
    });
  });
});