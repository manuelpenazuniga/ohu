# Ohu — Riel B (x402): oráculo de reputación pago-por-request

> **INV-4:** x402 es un protocolo de pago por request HTTP. **No** es el rail de
> settlement de escrow. El settlement de escrow (rail A) es una transferencia
> del contrato `OhuVault` autorizada por condiciones on-chain (tally de
> atestaciones ponderadas) + firmas Ed25519 gasless (domain separation;
> EIP-712 typed-data en roadmap) — nunca un flujo x402.

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
pnpm --filter @ohu/agents test   # 5 archivos, 18 tests (verdes)
```

- `x402-402-shape.test.ts` — 402 con `PAYMENT-REQUIRED` sobre el token CEP-18 (Rail B), asset ≠ OhuVault.
- `x402-paid-flow.test.ts` — flujo 402→firma→settle→recurso; **negativo**: firma inválida (facilitator la rechaza) no sirve reputación.
- `x402-failover-facilitator.test.ts` — el **fallback local** reemplaza al hosteado caído para `verify`/`getSupported`; **S4-b**: `settle` NO reintenta en el fallback (anti-doble-pago) y propaga el error etiquetado.
- `x402-non-escrow-invariant.test.ts` — INV-4: config rechaza asset==OhuVault; el recurso es reputación sin entrypoints de escrow.

La verificación **on-chain** (settle real en Testnet, visible en CSPR.cloud) es
manual y requiere material fuera de git (cuentas fondeadas + token CEP-18); los
tests CI validan la forma del flujo, los invariantes y el fallback sin tocar la
red.

## Idempotencia de `settle` (S4-b) — anti-doble-pago

`FailoverFacilitatorClient.settle` **NO reintenta en el fallback**. Solo
`verify` y `getSupported` hacen failover. Motivo:

- `settle` emite un deploy `transfer_with_authorization` on-chain y *espera* la
  confirmación. Si el primario **ya envió** el deploy y muere devolviendo la
  respuesta (timeout/crash/5xx tardío), reintentar el **mismo** payload contra
  el facilitator local dispararía un segundo deploy con la misma autorización.
- El mensaje EIP-712 `TransferWithAuthorization` incluye `nonce` (32 bytes) +
  `validBefore`/`validAfter`. El `verify` del `ExactCasperScheme`
  (`@make-software/casper-x402` 1.0.0) rechaza `validBefore` vencido o con < 6 s
  de ventana (frescura) y exige `nonce` de 32 bytes — pero el `validBefore` no
  vence entre el primer y el segundo settle (misma ventana de minutos): NO
  protege contra replay dentro de la ventana.
- La unicidad del nonce la impone el **contrato CEP-18 asset** dentro de su
  entry point `transfer_with_authorization` (patrón ERC-3009). Se verificó que
  el `Cep18X402.wasm` que shippea `make-software/casper-x402`
  (`infra/local/deployer/`) mantiene un diccionario `used_nonces` y emite
  `event_AuthorizationUsed` — para **ese** token un segundo settle con la misma
  autorización reverte on-chain. PERO esta garantía es **dependiente del
  contrato asset desplegado**, no del facilitator; no se asume a esta capa.
- Regla de seguridad tomada (alineada con el brief S4-b): **ante la duda, no
  reintentar settle**. La capa superior reintenta **idempotente** con una
  autorización **nueva** (nonce aleatorio + `validBefore` fresco). Es preferible
  un pago que se reintenta arriba a un doble pago.

El error que propaga `settle` se etiqueta `FailoverFacilitatorClient(settle)`
con el mensaje del primario y `cause` preservando la excepción original (ver
`agents/src/x402/failover-client.ts` y `agents/test/x402-failover-facilitator.test.ts`).

## TODO(audit)

- Verificar `limitedPaymentMotes` por defecto del facilitator contra la
  tarifa vigente de Testnet (`FACILITATOR_PAYMENT_MOTES`) al correr la demo live.
- **S4-b (abierto):** al desplegar la demo live, confirmar on-chain (CSPR.cloud)
  que el `ASSET_PACKAGE` usado reverte un segundo `transfer_with_authorization`
  con el mismo nonce (inspeccionar el `used_nonces` del contrato desplegado
  — p.ej. con el `Cep18X402.wasm` de `make-software/casper-x402`). Confirmado
  eso, evaluar relajar `settle` a un reintento **solo tras error demostrablemente
  anterior al `putTransaction`**; mientras tanto, settle no reintenta en el
  fallback (decisión S4-b, ya implementada y testeada).