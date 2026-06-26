# Ohu — Riel B (x402): oráculo de reputación pago-por-request

> **INV-4:** x402 es un protocolo de pago por request HTTP. **No** es el rail de
> settlement de escrow. El settlement de escrow (rail A) es una transferencia
> del contrato `OhuVault` autorizada por condiciones on-chain (tally de
> atestaciones ponderadas) + firmas EIP-712 gasless — nunca un flujo x402.

Este directorio (`agents/src/x402/`) implementa el **S4 (Fase 0)**: un cobro
x402 real sobre Casper Testnet usando [`make-software/casper-x402`](https://github.com/make-software/casper-x402)
(`@make-software/casper-x402` 1.0.0, `@x402/express` 2.16.0, `@x402/fetch` 2.16.0,
`@x402/core` 2.16.0). Es el futuro **oráculo de reputación vendido por request**.

## Flujo x402 (genuino, no escrow)

1. El cliente hace `GET /reputation/:producer` al servidor de recursos.
2. El servidor responde `402 Payment Required` con un header `PAYMENT-REQUIRED`
   (base64 JSON) que describe `scheme=exact`, `network=casper:casper-test`,
   `asset=<CEP-18>`, `payTo`, `amount` y el dominio EIP-712 del token.
3. El cliente construye y **firma una autorización EIP-712** con su clave
   **Ed25519** del Casper (gasless para el firmante, off-chain) y reintenta con
   el header `Payment-Signature` (base64 del `PaymentPayload`).
4. El servidor reenvía el payload al **facilitator**, que verifica la firma,
   monta un deploy de `transfer_with_authorization` sobre el token CEP-18, lo
   firma con su propia identidad y lo envía a **Testnet**.
5. El servidor entrega `200` con el recurso de reputación y un header
   `PAYMENT-RESPONSE` con el resultado del settlement.

El asset es **un token CEP-18** (no el `OhuVault`). El settle es
`transfer_with_authorization` del token, nunca un movimiento del `purse` del
`OhuVault`.

## Componentes

| Archivo | Rol |
|:--|:--|
| `config.ts` | Carga/valida `.env`. **Lanza** si `ASSET_PACKAGE` apunta al `OhuVault` (INV-4). |
| `reputation-server.ts` | App Express con `paymentMiddleware` (`ExactCasperScheme` server). Sirve reputación. |
| `facilitator.ts` | App Express del **facilitator local** (fallback) que firma contra Testnet. |
| `failover-client.ts` | `FailoverFacilitatorClient`: primario hosteado → fallback local ante fallos. |
| `facilitator-client.ts` | Arma el cliente facilitator del servidor (hosteado + fallback local). |
| `client.ts` / `pay.ts` | Cliente pagador (`wrapFetchWithPayment` + `ExactCasperScheme` client). |
| `serve-resource.ts` / `serve-facilitator.ts` | Entrypoints. |
| `constants.ts` | `ESCROW_FORBIDDEN_TOKENS` y la declaración no-escrow (ancla de tests). |

## Setup

```bash
cp infra/.env.example .env          # o agents/.env.example si prefieres
# Edita: ASSET_PACKAGE, PAYEE_ADDRESS, FACILITATOR_PEM_PATH,
#        CLIENT_PRIVATE_KEY_PATH  (cuentas Ed25519 fondeadas en Testnet)
```

Requisitos: un token CEP-18 desplegado en Testnet (hash de paquete 64 hex, lo
puedes usar como `ASSET_PACKAGE` — por ejemplo WCSPR) y dos cuentas Ed25519
fondeadas (facilitator y pagador).

## Run (demo live)

```bash
# T1 — facilitator local (inicia primero)
just x402-facilitator
# T2 — servidor de recursos
just x402-resource
# T3 — paga y recibe el recurso
just x402-pay
```

El settle queda on-chain: busca el deploy del facilitator en
[CSPR.cloud](https://testnet.cspr.live) (se verá un `transfer_with_authorization`
sobre el token CEP-18). **No** vas a ver movimiento del `OhuVault` — esto es
Rail B, no escrow.

## Tests (offline, reproducible desde clon limpio)

```bash
pnpm --filter @ohu/agents test   # 5 archivos, 14 tests (verdes)
```

- `x402-402-shape.test.ts` — 402 con `PAYMENT-REQUIRED` sobre el token CEP-18 (Rail B), asset ≠ OhuVault.
- `x402-paid-flow.test.ts` — flujo 402→firma→settle→recurso; **negativo**: firma inválida (facilitator la rechaza) no sirve reputación.
- `x402-failover-facilitator.test.ts` — el **fallback local** reemplaza al hosteado caído.
- `x402-non-escrow-invariant.test.ts` — INV-4: config rechaza asset==OhuVault; el recurso es reputación sin entrypoints de escrow.

La verificación **on-chain** (settle real en Testnet, visible en CSPR.cloud) es
manual y requiere material fuera de git (cuentas fondeadas + token CEP-18); los
tests CI validan la forma del flujo, los invariantes y el fallback sin tocar la
red.

## TODO(audit)

- Verificar `limitedPaymentMotes` por defecto del facilitator contra la
  tarifa vigente de Testnet (`FACILITATOR_PAYMENT_MOTES`) al correr la demo live.
- Confirmar que el `ExactCasperScheme` facilitator exige `validBefore` fresco
  (anti-replay) y que el nonce es único por `transfer_with_authorization` —
  depende de `casper-eip-712`/`casper-x402`, no asumido en este spike.