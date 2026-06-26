import type { FacilitatorClient } from "@x402/core/server";
import { HTTPFacilitatorClient, type FacilitatorConfig } from "@x402/core/server";
import type { X402Config } from "./config.js";
import { FailoverFacilitatorClient } from "./failover-client.js";

/**
 * Construye el cliente facilitator del servidor de recursos.
 *
 * - Primario: facilitator hosteado (`FACILITATOR_HOSTED_URL`) si está seteado.
 * - Fallback: facilitator local (`FACILITATOR_LOCAL_URL`) que firma contra
 *   Testnet — siempre presente, así el riel B sobrevive si el hosteado cae.
 *
 * Si no hay hosteado, el fallback actúa como único facilitator. (El test
 * negativo verifica que el fallback reemplaza al primario cuando este falla.)
 */
export function buildFacilitatorClient(cfg: X402Config): FacilitatorClient {
  function buildConfig(url: string): FacilitatorConfig {
    const c: FacilitatorConfig = { url };
    if (cfg.facilitatorApiKey) {
      const auth = { Authorization: cfg.facilitatorApiKey };
      c.createAuthHeaders = async () => ({
        verify: auth,
        settle: auth,
        supported: auth,
        bazaar: auth,
      });
    }
    return c;
  }

  const fallback = new HTTPFacilitatorClient(buildConfig(cfg.facilitatorLocalUrl));
  if (cfg.facilitatorHostedUrl) {
    const primary = new HTTPFacilitatorClient(buildConfig(cfg.facilitatorHostedUrl));
    return new FailoverFacilitatorClient(primary, fallback);
  }
  return fallback;
}