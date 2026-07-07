import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import {
  loadOperatorConfig,
  loadAdminConfig,
} from "../src/casper/env.js";

/**
 * Test de separación de roles (INV-2):
 *
 * Invariante de seguridad documentada: el módulo tesoreria JAMÁS
 * debe importar `loadAdminKey`, y el módulo autorizador JAMÁS debe
 * importar `loadOperatorKey`. Cada proceso carga SOLO su propia llave.
 *
 * INV-2a (separación estructural): `loadOperatorConfig()` NO produce
 * un objeto con `adminSecretKeyPath`, y correr sin `ADMIN_SECRET_KEY_PATH`
 * no rompe a Tesorería.
 *
 * Verificación por análisis estático del fuente y tests runtime.
 */
function readModule(path: string): string {
  return readFileSync(path, "utf8");
}

describe("role separation (INV-2)", () => {
  it("tesoreria/index.ts NO importa loadAdminKey", () => {
    const src = readModule("src/tesoreria/index.ts");
    expect(src).not.toMatch(/loadAdminKey/);
  });

  it("autorizador/index.ts NO importa loadOperatorKey", () => {
    const src = readModule("src/autorizador/index.ts");
    expect(src).not.toMatch(/loadOperatorKey/);
  });

  it("keys.ts exporta loadOperatorKey", () => {
    const src = readModule("src/casper/keys.ts");
    expect(src).toMatch(/export function loadOperatorKey/);
  });

  it("keys.ts exporta loadAdminKey", () => {
    const src = readModule("src/casper/keys.ts");
    expect(src).toMatch(/export function loadAdminKey/);
  });

  it("tesoreria/index.ts importa loadOperatorKey", () => {
    const src = readModule("src/tesoreria/index.ts");
    // Debe importar solo su llave
    expect(src).toMatch(/loadOperatorKey/);
  });

  it("autorizador/index.ts importa loadAdminKey", () => {
    const src = readModule("src/autorizador/index.ts");
    expect(src).toMatch(/loadAdminKey/);
  });

  it("env.ts exporta loadOperatorConfig", () => {
    const src = readModule("src/casper/env.ts");
    expect(src).toMatch(/export function loadOperatorConfig/);
  });

  it("env.ts exporta loadAdminConfig", () => {
    const src = readModule("src/casper/env.ts");
    expect(src).toMatch(/export function loadAdminConfig/);
  });

  it("tesoreria/index.ts importa loadOperatorConfig (no loadSwarmConfig ni loadAdminConfig)", () => {
    const src = readModule("src/tesoreria/index.ts");
    expect(src).not.toMatch(/loadSwarmConfig/);
    expect(src).not.toMatch(/loadAdminConfig/);
    expect(src).toMatch(/loadOperatorConfig/);
  });

  it("autorizador/index.ts importa loadAdminConfig (no loadSwarmConfig ni loadOperatorConfig)", () => {
    const src = readModule("src/autorizador/index.ts");
    expect(src).not.toMatch(/loadSwarmConfig/);
    expect(src).not.toMatch(/loadOperatorConfig/);
    expect(src).toMatch(/loadAdminConfig/);
  });

  it("loadOperatorConfig() NO produce adminSecretKeyPath en el objeto devuelto", () => {
    const prev = process.env["ADMIN_SECRET_KEY_PATH"];
    delete process.env["ADMIN_SECRET_KEY_PATH"];
    try {
      process.env["NODE_URL"] = "http://localhost";
      process.env["ODRA_CASPER_LIVENET_EVENTS_URL"] = "http://localhost/events";
      process.env["CHAIN_NAME"] = "casper-test";
      process.env["OHUVAULT_PACKAGE_HASH"] = "a".repeat(64);
      process.env["OHUVAULT_ATTESTATION_WINDOW_MS"] = "300000";
      process.env["OHUVAULT_OPERATOR_ACCOUNT_HASH"] = "00" + "aa".repeat(32);
      process.env["OHUVAULT_ADMIN_ACCOUNT_HASH"] = "00" + "bb".repeat(32);
      process.env["OPERATOR_SECRET_KEY_PATH"] = "/tmp/op.pem";

      const config = loadOperatorConfig();
      expect(config).toHaveProperty("operatorSecretKeyPath");
      expect(config).not.toHaveProperty("adminSecretKeyPath");
    } finally {
      if (prev !== undefined) {
        process.env["ADMIN_SECRET_KEY_PATH"] = prev;
      } else {
        delete process.env["ADMIN_SECRET_KEY_PATH"];
      }
    }
  });

  it("loadOperatorConfig() no rompe sin ADMIN_SECRET_KEY_PATH en el entorno", () => {
    const prev = process.env["ADMIN_SECRET_KEY_PATH"];
    delete process.env["ADMIN_SECRET_KEY_PATH"];
    try {
      process.env["NODE_URL"] = "http://localhost";
      process.env["ODRA_CASPER_LIVENET_EVENTS_URL"] = "http://localhost/events";
      process.env["CHAIN_NAME"] = "casper-test";
      process.env["OHUVAULT_PACKAGE_HASH"] = "a".repeat(64);
      process.env["OHUVAULT_ATTESTATION_WINDOW_MS"] = "300000";
      process.env["OHUVAULT_OPERATOR_ACCOUNT_HASH"] = "00" + "aa".repeat(32);
      process.env["OHUVAULT_ADMIN_ACCOUNT_HASH"] = "00" + "bb".repeat(32);
      process.env["OPERATOR_SECRET_KEY_PATH"] = "/tmp/op.pem";

      expect(() => loadOperatorConfig()).not.toThrow();
    } finally {
      if (prev !== undefined) {
        process.env["ADMIN_SECRET_KEY_PATH"] = prev;
      } else {
        delete process.env["ADMIN_SECRET_KEY_PATH"];
      }
    }
  });

  it("loadAdminConfig() NO produce operatorSecretKeyPath en el objeto devuelto", () => {
    const prev = process.env["OPERATOR_SECRET_KEY_PATH"];
    delete process.env["OPERATOR_SECRET_KEY_PATH"];
    try {
      process.env["NODE_URL"] = "http://localhost";
      process.env["ODRA_CASPER_LIVENET_EVENTS_URL"] = "http://localhost/events";
      process.env["CHAIN_NAME"] = "casper-test";
      process.env["OHUVAULT_PACKAGE_HASH"] = "a".repeat(64);
      process.env["OHUVAULT_ATTESTATION_WINDOW_MS"] = "300000";
      process.env["OHUVAULT_OPERATOR_ACCOUNT_HASH"] = "00" + "aa".repeat(32);
      process.env["OHUVAULT_ADMIN_ACCOUNT_HASH"] = "00" + "bb".repeat(32);
      process.env["ADMIN_SECRET_KEY_PATH"] = "/tmp/ad.pem";

      const config = loadAdminConfig();
      expect(config).toHaveProperty("adminSecretKeyPath");
      expect(config).not.toHaveProperty("operatorSecretKeyPath");
    } finally {
      if (prev !== undefined) {
        process.env["OPERATOR_SECRET_KEY_PATH"] = prev;
      } else {
        delete process.env["OPERATOR_SECRET_KEY_PATH"];
      }
    }
  });
});
