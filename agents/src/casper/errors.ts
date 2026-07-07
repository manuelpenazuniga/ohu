/**
 * Mapa de códigos de error de OhuVault relevantes para la coordinación del
 * enjambre (Tesorería + Autorizador). Los valores provienen de los reverts
 * de Odra con formato `"User error: <N>"`.
 */

/** Códigos de revert de OhuVault. */
export const OHUVAULT_ERRORS = {
  /** La ventana de atestación aún no cierra. */
  WINDOW_NOT_CLOSED: 54,
  /** El lote no está en estado FUNDED (o no fondeado, o ya evaluado). */
  LOTE_NOT_FUNDED: 47,
  /** El lote no está en EVAL_OK — no se puede liberar al productor. */
  LOTE_NOT_RELEASABLE: 55,
  /** El lote no está en EVAL_FAIL — aún no evaluado. */
  LOTE_NOT_FAILABLE: 56,
  /** La llave no es admin. */
  NOT_ADMIN: 3,
  /** La llave no es admin ni operator. */
  NOT_ADMIN_NOR_OPERATOR: 46,
} as const;

/** Códigos de error que indican que la identidad es incorrecta (FATAL). */
export const FATAL_AUTH_ERRORS: readonly number[] = [
  OHUVAULT_ERRORS.NOT_ADMIN,
  OHUVAULT_ERRORS.NOT_ADMIN_NOR_OPERATOR,
];

/**
 * Extrae el código numérico de un mensaje de error de Odra.
 * El formato esperado es `"User error: <N>"`.
 *
 * @returns El código numérico, o `null` si no se reconoce el formato.
 */
export function parseUserError(errorMessage: string): number | null {
  const match = /User error:\s*(\d+)/.exec(errorMessage);
  if (!match) return null;
  const code = Number.parseInt(match[1]!, 10);
  return Number.isNaN(code) ? null : code;
}
