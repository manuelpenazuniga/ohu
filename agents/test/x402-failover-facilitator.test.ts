import { describe, it, expect } from "vitest";
import { FailoverFacilitatorClient } from "../src/x402/failover-client.js";
import {
  makeBrokenFacilitator,
  makeMockFacilitator,
  makeTestConfig,
} from "./x402-fixtures.js";

/**
 * Invariante de auditoría S4: si el facilitator hosteado cae, el servidor de
 * recursos debe seguir sirviendo pagos vía el facilitator **local** (fallback)
 * que firma contra Testnet.
 */
describe("FailoverFacilitatorClient — fallback local cuando el hosteado falla", () => {
  it("settle usa el fallback cuando el primario lanza error de red", async () => {
    const cfg = makeTestConfig();
    const primary = makeBrokenFacilitator("hosted-down");
    const fallback = makeMockFacilitator(cfg);
    const client = new FailoverFacilitatorClient(primary, fallback);

    const result = await client.settle({} as never, {} as never);
    expect(result.success).toBe(true);
    expect(result.transaction).toBe("deploy-hash-cep18-transfer-with-authorization");
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

  it("si ambos fallan, lanza explicando qué pasó en primario y fallback", async () => {
    const client = new FailoverFacilitatorClient(
      makeBrokenFacilitator("primary"),
      makeBrokenFacilitator("fallback"),
    );
    await expect(client.verify({} as never, {} as never)).rejects.toThrow(
      /primario falló.*fallback local también/,
    );
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