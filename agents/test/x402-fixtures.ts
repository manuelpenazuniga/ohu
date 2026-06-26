import type {
  FacilitatorClient,
  PaymentRequirements,
  SettleResponse,
  SupportedResponse,
  VerifyResponse,
} from "@x402/core/types";
import type { X402Config } from "../src/x402/config.js";
import { NETWORK_CASPER_TESTNET, SCHEME_EXACT } from "../src/x402/constants.js";

export const DEV_PAYEE = "00" + "ab".repeat(32);
export const DEV_ASSET = "cd".repeat(32);
export const DEV_FEEPAYER = "01" + "0f".repeat(32);
export const DEV_SIGNER_ADDR = DEV_PAYEE;

export function makeTestConfig(overrides: Partial<X402Config> = {}): X402Config {
  return {
    chainID: NETWORK_CASPER_TESTNET,
    payeeAddress: DEV_PAYEE,
    assetPackage: DEV_ASSET,
    assetName: "WCSPR",
    assetSymbol: "WCSPR",
    assetVersion: "1",
    assetDecimals: 9,
    price: "$0.001",
    facilitatorHostedUrl: "",
    facilitatorLocalUrl: "http://localhost:4022",
    facilitatorApiKey: "",
    facilitatorPemPath: "/nonexistent.pem",
    facilitatorKeyAlgo: "ed25519",
    facilitatorRpcUrl: "https://node.testnet.casperlabs.io/rpc",
    facilitatorPaymentMotes: "1000000000",
    resourcePort: 4021,
    facilitatorPort: 4022,
    ohuVaultPackage: undefined,
    ...overrides,
  };
}

/**
 * Facilitator mock: acepta cualquier firma y devuelve un settlement de CEP-18
 * (`transfer_with_authorization`). Jamás referencia al OhuVault ni a su purse.
 */
export function makeMockFacilitator(cfg: X402Config): FacilitatorClient {
  const network = cfg.chainID;
  const supported: SupportedResponse = {
    kinds: [{ x402Version: 2, scheme: SCHEME_EXACT, network, extra: { feePayer: DEV_FEEPAYER } }],
    extensions: [],
    signers: { "casper:*": [DEV_SIGNER_ADDR] },
  };
  const verify: VerifyResponse = { isValid: true, payer: DEV_PAYEE };
  const settle: SettleResponse = {
    success: true,
    transaction: "deploy-hash-cep18-transfer-with-authorization",
    network,
    amount: "1000000",
    extra: { entryPoint: "transfer_with_authorization", token: "CEP-18" },
  };
  return {
    verify: async () => verify,
    settle: async () => settle,
    getSupported: async () => supported,
  };
}

/** Facilitator roto: simula host caído (rechaza todo con error de red). */
export function makeBrokenFacilitator(label: string): FacilitatorClient {
  const err = () => Promise.reject(new Error(`broken-facilitator:${label}`));
  return { verify: err, settle: err, getSupported: err };
}

/** Decodifica un header base64 JSON. */
export function decodeB64Json(header: string): unknown {
  return JSON.parse(Buffer.from(header, "base64").toString("utf-8"));
}

/**
 * Construye un `PaymentPayload` (x402 v2) con la autorización `exact` casper
 * (campos de EIP-712). Los valores son placeholders — el facilitator mock no
 * verifica firmas; los reales sí (firmados con clave Ed25519 off-chain).
 */
export function buildStubPaymentPayload(accepted: PaymentRequirements) {
  return {
    x402Version: 2,
    accepted,
    payload: {
      signature: "0x" + "01".repeat(65),
      publicKey: "02" + "ab".repeat(32),
      authorization: {
        from: DEV_PAYEE,
        to: DEV_PAYEE,
        value: (accepted.amount as string) ?? "1000000",
        validAfter: "0",
        validBefore: String(Math.floor(Date.now() / 1000) + 600),
        nonce: "00".repeat(32),
      },
    },
  };
}