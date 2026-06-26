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
 */
export class FailoverFacilitatorClient implements FacilitatorClient {
  private readonly primary: FacilitatorClient;
  private readonly fallback: FacilitatorClient;

  constructor(primary: FacilitatorClient, fallback: FacilitatorClient) {
    this.primary = primary;
    this.fallback = fallback;
  }

  private async run<T>(
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
    return this.run((c) => c.verify(payload, reqs), "verify");
  }

  settle(payload: PaymentPayload, reqs: PaymentRequirements): Promise<SettleResponse> {
    return this.run((c) => c.settle(payload, reqs), "settle");
  }

  getSupported(): Promise<SupportedResponse> {
    return this.run((c) => c.getSupported(), "supported");
  }
}