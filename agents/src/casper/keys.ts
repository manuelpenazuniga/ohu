import { readFileSync } from "node:fs";
import { PrivateKey, KeyAlgorithm } from "casper-js-sdk";

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
  return PrivateKey.fromPem(pem, KeyAlgorithm.ED25519);
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
  return PrivateKey.fromPem(pem, KeyAlgorithm.ED25519);
}
