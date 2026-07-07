import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";

/**
 * Test de separación de roles (INV-2):
 *
 * Invariante de seguridad documentada: el módulo tesoreria JAMÁS
 * debe importar `loadAdminKey`, y el módulo autorizador JAMÁS debe
 * importar `loadOperatorKey`. Cada proceso carga SOLO su propia llave.
 *
 * Verificación por análisis estático del fuente.
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
});
