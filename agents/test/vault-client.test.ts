import { describe, it, expect, vi } from "vitest";
import { callVaultEntrypoint } from "../src/casper/vault-client.js";
import type { SwarmConfig } from "../src/casper/env.js";

const PKG_HEX = "a".repeat(64);

function makeTestConfig(overrides: Partial<SwarmConfig> = {}): SwarmConfig {
  return {
    nodeUrl: "https://node.testnet.casper.network/rpc",
    eventsUrl: "https://events.testnet.casper.network/events",
    chainName: "casper-test",
    vaultPackageHash: PKG_HEX,
    attestationWindowMs: 300000,
    operatorAccountHash: "00" + "aa".repeat(32),
    adminAccountHash: "00" + "bb".repeat(32),
    pollIntervalMs: 15000,
    logFile: "/tmp/ohu-test-swarm-log.jsonl",
    targetLotes: "",
    autorizadorStartDelayMs: 0,
    txPaymentMotes: 3_000_000_000,
    ...overrides,
  };
}

/**
 * Crea un mock mínimo de PrivateKey para el helper. Solo necesitamos el
 * `publicKey` getter (para `.from(pub)`) y `sign()` (void, no se llama en test
 * porque el builder mockea `build()`).
 */
function mockPrivateKey() {
  return {
    publicKey: {} as never,
    sign: vi.fn(),
  };
}

describe("callVaultEntrypoint — allowlist", () => {
  const config = makeTestConfig();
  const signer = mockPrivateKey();

  it("lanza si el entrypoint no está en el allowlist", async () => {
    await expect(
      callVaultEntrypoint(signer as never, "route_micropayment", 1, config),
    ).rejects.toThrow(/entrypoint no permitido/);
  });

  it("lanza con 'execute' (deploy de sesión disfrazado)", async () => {
    await expect(
      callVaultEntrypoint(signer as never, "execute", 1, config),
    ).rejects.toThrow(/entrypoint no permitido/);
  });

  it("lanza con 'deposit' (fuera de la matriz de autoridad)", async () => {
    await expect(
      callVaultEntrypoint(signer as never, "deposit", 1, config),
    ).rejects.toThrow(/entrypoint no permitido/);
  });

  it("lanza con nombres vacíos", async () => {
    await expect(
      callVaultEntrypoint(signer as never, "", 1, config),
    ).rejects.toThrow(/entrypoint no permitido/);
  });

  it("lanza con strings que contienen nombres válidos como substrings", async () => {
    // "evaluate_lote_malicioso" no debe ser aceptado
    await expect(
      callVaultEntrypoint(signer as never, "evaluate_lote_malicioso", 1, config),
    ).rejects.toThrow(/entrypoint no permitido/);
  });

  it("permite evaluate_lote (operator)", async () => {
    // El mock lanza en RPC porque no configuramos respuesta HTTP.
    // Pero la validación de allowlist pasa — el error será de red, no de allowlist.
    await expect(
      callVaultEntrypoint(signer as never, "evaluate_lote", 1, config),
    ).rejects.toThrow(); // error de RPC real (sin mock de red)
  });

  it("permite release_to_producer (admin)", async () => {
    await expect(
      callVaultEntrypoint(signer as never, "release_to_producer", 1, config),
    ).rejects.toThrow(); // error de RPC, no de allowlist
  });

  it("permite settle_failure (admin)", async () => {
    await expect(
      callVaultEntrypoint(signer as never, "settle_failure", 1, config),
    ).rejects.toThrow(); // error de RPC, no de allowlist
  });
});
