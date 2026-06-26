import { describe, it, expect, afterEach, beforeEach } from "vitest";
import { loadX402Config } from "../src/x402/config.js";

/**
 * INV-4 — fail-closed (tarea S4-a): el centinela que impide que x402 cobre
 * contra el `OhuVault` no puede ser opt-in. En entornos non-test,
 * `OHU_VAULT_PACKAGE` es REQUERIDO; si falta, `loadX402Config` lanza. Solo hay
 * escape para tests (NODE_ENV=test / VITEST / OHU_X402_TEST_CONFIG=1).
 */
describe("Rail B x402 — config fail-closed (INV-4, S4-a)", () => {
  // Set de vars mínimas válidas para que loadX402Config pase todo lo demás
  // excepto la var objeto de la prueba. OHU_VAULT_PACKAGE se controla por test.
  const BASE_ENV = {
    PAYEE_ADDRESS: "00" + "ab".repeat(32),
    ASSET_PACKAGE: "cd".repeat(32),
    ASSET_NAME: "WCSPR",
    X402_PRICE: "$0.001",
    FACILITATOR_PEM_PATH: "/x.pem",
  };

  let savedEnv: NodeJS.ProcessEnv;

  beforeEach(() => {
    savedEnv = { ...process.env };
  });

  afterEach(() => {
    process.env = savedEnv;
  });

  function setBaseEnv(): void {
    for (const [k, v] of Object.entries(BASE_ENV)) {
      process.env[k] = v;
    }
  }

  /**
   * Fuerza modo producción: neutraliza TODAS las señales de test
   * (NODE_ENV=test, VITEST=true, OHU_X402_TEST_CONFIG=1) para que
   * isTestEnvironment() devuelva false dentro de la corrida de vitest.
   */
  function forceProductionMode(): void {
    process.env["NODE_ENV"] = "production";
    // vitest setea VITEST=true; lo quitamos para simular prod fielmente.
    delete process.env["VITEST"];
    delete process.env["OHU_X402_TEST_CONFIG"];
  }

  it("sin OHU_VAULT_PACKAGE en modo producción, loadX402Config lanza (fail-closed)", () => {
    forceProductionMode();
    setBaseEnv();
    delete process.env["OHU_VAULT_PACKAGE"];

    expect(() => loadX402Config()).toThrow(/OHU_VAULT_PACKAGE.*requerido/i);
  });

  it("con OHU_VAULT_PACKAGE válido en modo producción, carga la config (positivo)", () => {
    forceProductionMode();
    setBaseEnv();
    // Paquete distinto al asset (no colisión) — válido para el centinela.
    process.env["OHU_VAULT_PACKAGE"] = "ff".repeat(32);

    const cfg = loadX402Config();
    expect(cfg.ohuVaultPackage).toBe("ff".repeat(32));
    expect(cfg.assetPackage).toBe("cd".repeat(32));
  });

  it("OHU_VAULT_PACKAGE con formato inválido se rechaza también en producción", () => {
    forceProductionMode();
    setBaseEnv();
    process.env["OHU_VAULT_PACKAGE"] = "no-es-un-hash";

    expect(() => loadX402Config()).toThrow(/OHU_VAULT_PACKAGE inválido/i);
  });

  it("en modo test, OHU_VAULT_PACKAGE es opcional (escape para tests)", () => {
    // Reproducimos el escape explícito: NODE_ENV=test y unset de la var.
    process.env["NODE_ENV"] = "test";
    setBaseEnv();
    delete process.env["OHU_VAULT_PACKAGE"];

    const cfg = loadX402Config();
    expect(cfg.ohuVaultPackage).toBeUndefined();
  });

  it("OHU_X402_TEST_CONFIG=1 habilita el escape sin necesidad de NODE_ENV=test", () => {
    forceProductionMode();
    process.env["OHU_X402_TEST_CONFIG"] = "1";
    setBaseEnv();
    delete process.env["OHU_VAULT_PACKAGE"];

    const cfg = loadX402Config();
    expect(cfg.ohuVaultPackage).toBeUndefined();
  });

  it("la colisión ASSET_PACKAGE == OHU_VAULT_PACKAGE sigue lanzando INV-4 (regresión)", () => {
    forceProductionMode();
    setBaseEnv();
    const vaultPkg = "cd".repeat(32); // == ASSET_PACKAGE => colisión pura
    process.env["OHU_VAULT_PACKAGE"] = vaultPkg;

    expect(() => loadX402Config()).toThrow(/INV-4/);
  });
});