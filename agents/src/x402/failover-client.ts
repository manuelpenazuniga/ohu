import type {
  PaymentPayload,
  PaymentRequirements,
  SettleResponse,
  SupportedResponse,
  VerifyResponse,
} from "@x402/core/types";
import type { FacilitatorClient } from "@x402/core/server";

/**
 * `FailoverFacilitatorClient` — implementa {@link FacilitatorClient} intentando
 * primero el facilitator hosteado (primario) y, ante un fallo de disponibilidad
 * (error de red/no-2xx), reintenta contra el facilitator **local** (fallback)
 * que firma deploys contra Casper Testnet.
 *
 * No es lógica de arbitraje de precios: solo de disponibilidad. Ambos
 * facilitators deben aceptar la misma red/asset/precio — son réplicas del mismo
 * verificador/asentador.
 *
 * INV-4: el settlement sigue siendo un `transfer_with_authorization` del token
 * CEP-18, nunca un movimiento del `purse` del `OhuVault`.
 *
 * ---
 *
 * ## S4-b — Idempotencia de `settle` (NO se reintenta en el fallback)
 *
 * `settle` **NO** hace failover automático: si el primario lanza, el error se
 * propaga envuelto para que la capa superior reintente **idempotente** con una
 * autorización **nueva** (nonce + `validBefore` frescos). Solo `verify` y
 * `getSupported` reintentan contra el fallback — son operaciones de lectura
 * (verificar una firma / listar schemes) y no mueven on-chain.
 *
 * ### Riesgo que se evita
 * El primario puede **emitir el deploy `transfer_with_authorization` on-chain y
 * fallar al devolver la respuesta** (timeout, corte de conexión, 5xx tardío,
 * crash del proceso hosteado tras firmar/enviar). En ese estado el pago YA está
 * liquidado; si el failover reintenta el **mismo** payload contra el facilitator
 * local, dispara un **segundo** deploy con la misma autorización.
 *
 * ### Anti-replay on-chain: verificado pero NO asumido a nivel facilitator
 * El mensaje EIP-712 `TransferWithAuthorization` sí incluye `nonce` (32 bytes)
 * y `validBefore`/`validAfter`, y el `verify` del `ExactCasperScheme`
 * (`@make-software/casper-x402` 1.0.0) rechaza `validBefore` vencido o con
 * < 6 s de ventana (frescura), además de exigir `nonce` de 32 bytes.
 *
 * La unicidad del nonce (rechazo del segundo deploy con el mismo nonce) la
 * impone el **contrato CEP-18 desplegado como `ASSET_PACKAGE`** dentro de su
 * entry point `transfer_with_authorization` (patrón ERC-3009). Se verificó que
 * el `Cep18X402.wasm` que shippea `make-software/casper-x402`
 * (`infra/local/deployer/`) mantiene un diccionario `used_nonces` y emite
 * `event_AuthorizationUsed`, i.e. para **ese** token un segundo settle con la
 * misma autorización revierte on-chain. PERO esta garantía es
 * **dependiente del contrato asset**, no del facilitator — si `ASSET_PACKAGE`
 * apunta a un CEP-18 que no trackea `used_nonces`, el replay sí duplica el
 * pago.
 *
 * Como no podemos garantizar anti-replay on-chain desde esta capa, y la regla
 * de seguridad (S4-b brief) dice "si no está garantizado, no reintentes settle
 * automáticamente", preferimos no reintentar y propagar. Es preferible un pago
 * perdido (reintentable idempotente arriba con autorización nueva) a un doble
 * pago.
 *
 * // TODO(audit): al desplegar la demo live, confirmar on-chain (CSPR.cloud)
 *   que el `ASSET_PACKAGE` usado reverte un segundo `transfer_with_authorization`
 *   con el mismo nonce (ver `used_nonces` del contrato). Si se confirma,
 *   evaluar relajar reintento solo tras un error que se demuestre anterior al
 *   `putTransaction`; mientras tanto settle no reintenta.
 */
export class FailoverFacilitatorClient implements FacilitatorClient {
  private readonly primary: FacilitatorClient;
  private readonly fallback: FacilitatorClient;

  constructor(primary: FacilitatorClient, fallback: FacilitatorClient) {
    this.primary = primary;
    this.fallback = fallback;
  }

  /**
   * Ejecuta `op` primero en el primario y, si lanza, en el fallback. Se usa
   * solo para operaciones **no-destructivas y re-ejecutables sin riesgo de
   * doble efecto** (`verify`, `getSupported`). Ver arriba por qué `settle` no
   * usa este path.
   */
  private async runFailover<T>(
    op: (c: FacilitatorClient) => Promise<T>,
    label: string,
  ): Promise<T> {
    try {
      return await op(this.primary);
    } catch (primaryErr) {
      const reason = primaryErr instanceof Error ? primaryErr.message : String(primaryErr);
      // TODO(audit): el umbral de "fallo de disponibilidad" (network/no-2xx)
      //   vs "pago inválido" es deliberadamente amplio aquí. Un VerifyResponse
      //   con isValid=false NO se considera fallo de facilitator y se retorna
      //   tal cual desde el primario (el fallback no reescribiría el veredicto).
      try {
        return await op(this.fallback);
      } catch (fallbackErr) {
        const fmsg = fallbackErr instanceof Error ? fallbackErr.message : String(fallbackErr);
        throw new Error(
          `FailoverFacilitatorClient(${label}): primario falló ("${reason}") y el fallback local también ("${fmsg}")`,
        );
      }
    }
  }

  verify(payload: PaymentPayload, reqs: PaymentRequirements): Promise<VerifyResponse> {
    // Solo verificación de firma/límites — re-ejecutable sin riesgo de doble
    // pago: failover OK.
    return this.runFailover((c) => c.verify(payload, reqs), "verify");
  }

  /**
   * `settle` → `transfer_with_authorization` on-chain. **NO reintenta en el
   * fallback**: si el primario lanza, el pago puede ya estar liquidado y un
   * segundo deploy con la misma autorización podría duplicar el pago. Ver
   * bloque JSDoc de la clase para el análisis anti-replay.
   *
   * Se propaga un error explícito etiquetado `(settle)` con el mensaje del
   * primario para que la capa superior decida el reintento **idempotente**
   * (autorización nueva: nonce + `validBefore` frescos).
   */
  settle(payload: PaymentPayload, reqs: PaymentRequirements): Promise<SettleResponse> {
    const label = "settle";
    const op = (c: FacilitatorClient) => c.settle(payload, reqs);
    return op(this.primary).catch((primaryErr: unknown) => {
      const reason = primaryErr instanceof Error ? primaryErr.message : String(primaryErr);
      const err = new Error(
        `FailoverFacilitatorClient(${label}): el primario falló tras posible envío ("${reason}"); ` +
          `NO se reintenta en el fallback para evitar doble settle. ` +
          `Reintentar idempotente arriba con autorización nueva (nonce + validBefore frescos).`,
      );
      // Preservar la causa original para diagnóstico del upper layer.
      (err as Error & { cause?: unknown }).cause = primaryErr;
      throw err;
    });
  }

  getSupported(): Promise<SupportedResponse> {
    // Solo metadatos de schemes soportados — re-ejecutable: failover OK.
    return this.runFailover((c) => c.getSupported(), "supported");
  }
}