import { readFileSync } from "node:fs";
// casper-js-sdk es CJS: bajo node-ESM sus named exports no se sintetizan
// (todo queda en el default). Se importan los VALORES vía el default y los
// TIPOS vía `import type`. Ver vault-client.ts para la misma razón.
import sdk from "casper-js-sdk";
import type { PrivateKey } from "casper-js-sdk";
const { KeyAlgorithm } = sdk;

/**
 * Carga la llave operator (Ed25519) desde el PEM definido en
 * `OPERATOR_SECRET_KEY_PATH`. NUNCA loguea el contenido del PEM ni
 * el path completo.
 *
 * **Restricción de seguridad INV-2:** este módulo solo carga la llave
 * operator; el Autorizador NUNCA debe importar esta función.
 */
export function loadOperatorKey(pemPath: string): PrivateKey {
  if (!pemPath) {
    throw new Error("tesoreria: OPERATOR_SECRET_KEY_PATH no configurado");
  }
  const pem = readFileSync(pemPath, "utf8");
  return sdk.PrivateKey.fromPem(pem, KeyAlgorithm.ED25519);
}

/**
 * Carga la llave admin (Ed25519) desde el PEM definido en
 * `ADMIN_SECRET_KEY_PATH`. NUNCA loguea el contenido del PEM ni
 * el path completo.
 *
 * **Restricción de seguridad INV-2:** este módulo solo carga la llave
 * admin; la Tesorería NUNCA debe importar esta función.
 */
export function loadAdminKey(pemPath: string): PrivateKey {
  if (!pemPath) {
    throw new Error("autorizador: ADMIN_SECRET_KEY_PATH no configurado");
  }
  const pem = readFileSync(pemPath, "utf8");
  return sdk.PrivateKey.fromPem(pem, KeyAlgorithm.ED25519);
}
