import { describe, it, expect } from "vitest";
import { parseUserError, OHUVAULT_ERRORS } from "../src/casper/errors.js";

describe("parseUserError", () => {
  it("extrae el código de un mensaje User error estándar", () => {
    expect(parseUserError("User error: 54")).toBe(54);
  });

  it("funciona con espacios extra", () => {
    expect(parseUserError("User error:  47")).toBe(47);
  });

  it("devuelve null si el mensaje no tiene formato User error", () => {
    expect(parseUserError("Something went wrong")).toBeNull();
  });

  it("devuelve null si el mensaje es undefined o vacío", () => {
    expect(parseUserError("")).toBeNull();
  });

  it("extrae otros códigos de error de Odra no mapeados explícitamente", () => {
    expect(parseUserError("User error: 99")).toBe(99);
  });

  it("devuelve null si el código no es numérico", () => {
    expect(parseUserError("User error: abc")).toBeNull();
  });
});

describe("OHUVAULT_ERRORS", () => {
  it("contiene los códigos esperados", () => {
    expect(OHUVAULT_ERRORS.WINDOW_NOT_CLOSED).toBe(54);
    expect(OHUVAULT_ERRORS.LOTE_NOT_FUNDED).toBe(47);
    expect(OHUVAULT_ERRORS.LOTE_NOT_RELEASABLE).toBe(55);
    expect(OHUVAULT_ERRORS.LOTE_NOT_FAILABLE).toBe(56);
    expect(OHUVAULT_ERRORS.NOT_ADMIN).toBe(3);
    expect(OHUVAULT_ERRORS.NOT_ADMIN_NOR_OPERATOR).toBe(46);
  });
});
