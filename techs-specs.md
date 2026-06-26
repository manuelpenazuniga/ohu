# Tech Specs — Ohu

### Due diligence de ingeniería sobre el stack Casper (rol: Senior Blockchain Engineer, especialización Casper)
**2026-06-25 · revisión técnica de `ohu.md`**

> **Mandato:** validar cada supuesto técnico contra el estado **real y actual** de las tecnologías de Casper, y garantizar que **nada de lo que pongamos en el camino crítico tenga que descartarse después** por estar incompleto, no lanzado o ser impracticable. Todo lo que esté en el *core* debe estar **vivo y usable hoy** (junio 2026). Lo que no, va detrás de una interfaz o se elimina.

---

## 0. Veredicto en una página

`ohu.md` es **construible en su totalidad sobre tecnología que existe y funciona hoy**, pero requiere **dos correcciones de fondo** y varias de precisión. Ninguna corrección reduce el alcance del producto; de hecho **simplifican la operación** y **aumentan el fit** con el toolkit oficial.

| | Hallazgo | Impacto | Acción |
|:--|:--|:--|:--|
| 🔴 **1** | El modelo **Addressable Entity** ("contratos como cuentas de primera clase" con sus propias claves asociadas) está implementado en Casper 2.0 pero **NO activado ni en mainnet ni en testnet** (mainnet 2.2.0 del 23-mar-2026 no lo activó; en testnet figura como *"not yet activated"*). | El supuesto central de `ohu.md` —"contrato-cuenta" con el agente como sub-clave del contrato— **no se puede construir tal cual, ni siquiera para el demo en testnet**. | Rediseño de custodia (§2): contrato con **purse** + multisig nativo a nivel **cuenta** + aprobación **M-de-N en el contrato**. Mismo nivel de seguridad, **disponible hoy en ambas redes**. |
| 🟠 **2** | **x402 es un protocolo de pago por servicio/recurso HTTP**, no un riel universal de transferencias. Decir que "cada pago es x402" es semánticamente incorrecto. | Sobre-venta técnica que un jurado de ingeniería detecta. | Separar **dos rieles** (§3): liquidación de escrow (contrato + EIP-712) vs. **comercio de servicios x402** (genuino y central). |
| 🟢 **3** | **Odra, x402 (vivo en mainnet), casper-eip-712, claves asociadas nativas, CSPR.trade MCP, CSPR.cloud, CSPR.click Skill** están **vivos y usables**. | El resto del diseño es sólido. | Construir con confianza; confirmar 1 endpoint x402 en testnet en 72h. |
| 🟢 **4** | **casper-eip-712** (firma typed-data con verificación on-chain en Odra) habilita **atestaciones y onboarding *gasless***. | Mejora no contemplada que sube UX, realismo y fit. | Adoptarla en el core (§4). |

**Conclusión:** seguimos adelante. El stack no nos va a dejar tirados a mitad de camino **si respetamos el registro de dependencias de §1** y construimos solo sobre lo verde.

---

## 1. Registro de dependencias (lo que verifiqué, con estado real)

Clasificación: 🟢 **CORE-SAFE** (vivo, maduro, en el camino crítico) · 🟠 **USE-CON-CUIDADO** (usable pero detrás de interfaz / con fallback) · 🔴 **NO-DEPENDER** (no lanzado / no activado / fuera de scope).

| Tecnología | Uso en Ohu | Estado real verificado (jun 2026) | Verdicto |
|:--|:--|:--|:--:|
| **Odra (Rust)** | Todos los contratos (`OhuVault`, `MutualPool`, `Reputation`, `CoopRegistry`) | Odra 1.0+, mantenido, en el AI Toolkit oficial, `llms.txt`, módulos CEP-18/78 listos | 🟢 |
| **x402 (Facilitator)** | Comercio de servicios + riel de settlement de pagos autorizados | **Vivo en mainnet** (jun 2026, primera L1 WASM-native). Docs: `docs.cspr.cloud/x402-facilitator-api`. Impl. de referencia: **`make-software/casper-x402`** con ejemplos. Firma **ed25519** vía header `X-Payment` | 🟢 *(confirmar testnet en 72h; fallback: facilitator local)* |
| **casper-eip-712** | Atestaciones y commits **firmados gasless**, verificados on-chain | v1.2.0+, **verificación on-chain en Odra** (ejemplo CEP-18 *permit* gasless con tests), firma Casper-native, CAIP-2 `casper-test` | 🟢 |
| **Claves asociadas / multisig nativo (a nivel cuenta)** | Protección de capital (admin/DAO co-firma) | Nativo desde Casper 1.x, **inalterado y activo** en 2.x | 🟢 |
| **CSPR.cloud APIs (REST/Streaming/Node)** | Indexado, dashboards, lectura de estado, streams de eventos | Middleware enterprise maduro; instalable como skill | 🟢 |
| **CSPR.click Agent Skill** | Creación de wallet, **build & sign de tx**, deploy de Odra | Instalable (`claude skill install cspr-click`), proxy a CSPR.cloud | 🟢 *(acelerador dev; producción usa el SDK directo)* |
| **CSPR.trade MCP** | Swap del payout al token preferido del productor (opcional) | **Hosted** en `mcp.cspr.trade/mcp`, 24 tools, sin API keys ni setup | 🟠 *(opcional; solo si hay swaps. No en camino crítico del MVP)* |
| **Casper MCP (msanlisavas)** | Lectura on-chain por lenguaje natural | Community-built, orientado a lectura | 🟠 *(conveniencia; detrás de interfaz, core usa CSPR.cloud/SDK)* |
| **Addressable Entity / "contrato-cuenta" con claves propias** | (supuesto original de custodia) | **Implementado en 2.0, NO activado en mainnet NI en testnet** (mainnet 2.2.0 no lo incluyó; testnet: *"not yet activated"*). VM 2.0 también pendiente. | 🔴 **NO-DEPENDER** |
| **ZK (Risc0→WASM)** | (idea "Sigilo", explícitamente roadmap) | Funciona con límites (4MB/Groth16); fuera del MVP por diseño | 🔴 *(excluido del MVP a propósito)* |

> **Regla de oro para todo el desarrollo:** el camino crítico solo toca filas 🟢. Las 🟠 viven detrás de una interfaz (`PaymentRail`, `ChainReader`, `Dex`) para poder degradar a una llamada directa de SDK sin tocar la lógica de negocio. Las 🔴 no entran.

---

## 2. Corrección #1 — Modelo de custodia (sin Addressable Entity)

**El problema:** `ohu.md` asume que `OhuVault` es un *contrato-cuenta* con **claves asociadas propias**, donde el agente firma como "sub-clave del contrato" de peso bajo. Eso requiere el **Addressable Entity model**, que **no está activado en mainnet**. Si construimos sobre eso, en producción no funciona. Inaceptable según el mandato.

**Lo que SÍ está disponible hoy y da exactamente la misma garantía de seguridad:**

```
   ┌─ Cuenta ADMIN (multisig NATIVO a nivel cuenta) ─┐     ┌─ Cuenta AGENTE ─┐
   │  Claves asociadas ponderadas + thresholds:      │     │  1 keypair       │
   │   Productor-rep 60 · Inversores/DAO 40          │     │  (identidad on-  │
   │   deployment threshold alto → co-firma forzada  │     │   chain propia)  │
   └───────────────┬─────────────────────────────────┘     └────────┬─────────┘
        controla   │  (privilegiado)                  opera (capado) │
                   ▼                                                 ▼
   ┌──────────────── Contrato  OhuVault  (Odra) ─────────────────┐
   │  Custodia fondos en un PURSE (esto SÍ funciona hoy, pre-2.0)     │
   │  ACCESS CONTROL en estado del contrato:                         │
   │   · operator = cuenta_agente → solo entrypoints CAPADOS         │
   │     (route_micropayment con tope, abrir lote, registrar atest.) │
   │   · admin = cuenta_multisig  → entrypoints PRIVILEGIADOS        │
   │     (reconfigurar, upgrade, retiro de emergencia)               │
   │  Aprobación M-de-N para releases grandes: approve(id)×M → exec  │
   │  Disparador paramétrico: tally de atestaciones ponderadas       │
   └─────────────────────────────────────────────────────────────────┘
```

**Por qué esto es correcto y suficiente:**
1. **El agente NO puede drenar el vault.** Su cuenta solo está autorizada para entrypoints capados; los movimientos de capital exigen que `caller == admin`, y `admin` es una **cuenta multisig nativa** cuyo `deployment threshold` obliga a co-firma humana. Un LLM comprometido toca, como mucho, micropagos acotados. *La propiedad de seguridad que enamora al jurado se mantiene íntegra — solo cambia dónde viven las llaves: en cuentas, no en el contrato.*
2. **El contrato custodia fondos** vía purse — capacidad **pre-2.0**, sin dependencia de features no activados.
3. **El "weighted multisig multiparte"** (el mecanismo rescatado de `semi-final.md`) se implementa de dos formas combinadas, ambas vivas hoy:
   - **Capital:** claves asociadas **nativas** ponderadas en la cuenta admin.
   - **Disparador paramétrico:** **tally ponderado de atestaciones** en el estado del contrato (cada atestación pesa por la *share* de pago del comprador).
4. **Migración futura sin romper nada:** el día que se active Addressable Entity, podemos *opcionalmente* colapsar admin+vault en una sola entidad. Pero **no lo necesitamos** para lanzar. Cero deuda bloqueante.

> Nota de precisión (esto es lo que distingue un diseño senior): en Casper los **thresholds de cuenta son un único número por acción** — no dan granularidad "monto chico vs. grande" por sí solos. Esa granularidad la da **el código del contrato** (entrypoints capados + M-de-N), no los thresholds nativos. `ohu.md` ya lo intuía en §5, pero hay que quitar toda referencia a "el agente es una sub-clave del contrato".

---

## 3. Corrección #2 — Los dos rieles de valor (semántica honesta de x402)

**El problema:** `ohu.md` dice "cada pago, prima, refund e indemnización es un micropago x402". Técnicamente x402 es un **protocolo de pago por recurso HTTP** (cliente pide → `402` con monto → cliente firma autorización ed25519 → el *facilitator* liquida on-chain → se entrega el recurso). Liberar fondos que **ya están en escrow** no es un flujo x402: es una transferencia del contrato. Conflar todo bajo "x402" es la clase de sobre-venta que un jurado técnico penaliza.

**El diseño correcto: dos rieles, cada uno en su lugar.**

### Riel A — Liquidación de escrow (contrato `OhuVault` + EIP-712)
Custodia y settlement de los fondos del lote. **No es x402**; es lógica de contrato:
- Depósito en escrow del comprador, bono del productor → entran al purse del vault.
- `release_to_producer` / `settle_failure` / refund / slash / indemnización → transferencias del contrato, **gateadas por el tally paramétrico + autorizaciones EIP-712** firmadas por las partes (gasless, §4).
- Finalidad rápida de Casper → el productor ve la plata en segundos. *(En el demo se muestra "pago instantáneo"; la etiqueta correcta es "settlement del vault", no "x402".)*

### Riel B — Comercio de servicios x402 (genuino, central, alineado con los $100k)
x402 usado **para lo que es**, generando volumen real de micropagos:
1. **Oráculo de reputación + libro de demanda agregada como API x402** → agentes de terceros y productores **pagan por consulta** (`402` → ed25519 → facilitator). Esto **es** la dirección-ejemplo **#2 "RWA Oracle Agent con identidad on-chain"** ejecutada literal, y es pay-per-request canónico.
2. **Los agentes consumen servicios externos vía x402** (vetting de productores, cotizaciones de logística, analítica) — el caso de uso M2M del enunciado.
3. **Prima de la mutual / fee de membresía** cobrados vía x402 cuando el pagador interactúa por HTTP.
4. **(Opcional)** el **facilitator de x402 como motor de settlement** para pagos *autorizados* salientes (el agente firma, el facilitator liquida en Casper). Legítimo, pero secundario al Riel A.

> Resultado: x402 sigue **central y prominente** (oráculo-as-a-service + consumo de servicios + facilitator), pero **honesto**. Esto *sube* la credibilidad ante jueces técnicos y mapea una dirección-ejemplo de forma literal. No perdemos fit; ganamos rigor.

---

## 4. Mejora nueva — Atestaciones y onboarding *gasless* con casper-eip-712

Hallazgo que no estaba en `ohu.md` y que **mejora realismo, UX y fit** a la vez:

`casper-eip-712` (v1.2.0+, con **verificación on-chain en Odra** ya probada en un ejemplo CEP-18 *permit*) permite que un comprador/productor **firme un mensaje typed-data off-chain** y que **el agente lo retransmita on-chain pagando el gas**. Aplicado a Ohu:

- **Atestación de recepción** ("recibí / no recibí el lote #123") = un mensaje EIP-712 firmado por el comprador. El agente lo sube; el contrato **verifica la firma** y actualiza el tally. El comprador **no necesita tener CSPR ni manejar gas** — solo firma.
- **Commit de demanda y de pago** = mismo patrón.
- **Onboarding sin fricción:** un restaurante entra con una wallet y una firma, sin comprar CSPR para gas. Esto es **decisivo para adopción real** (R6) y elimina la barrera #1 del B2B on-chain.

Es, además, **otra pieza del toolkit oficial exhibida** y técnicamente hermosa (meta-transacciones verificadas en el contrato). Entra al core.

---

## 5. Arquitectura de agentes (con tooling real, sin atarse a lo frágil)

| Agente | Identidad | Firma / ejecución | Lectura on-chain | DeFi (opcional) |
|:--|:--|:--|:--|:--|
| **Agregador** | cuenta propia | CSPR.click Skill / SDK | CSPR.cloud REST | — |
| **Tesorería** | cuenta propia (operator) | SDK directo (ed25519) + x402 client | CSPR.cloud Streaming (eventos de atestación) | CSPR.trade MCP (swap payout) |
| **Mutual/Riesgo** | cuenta propia | SDK directo | CSPR.cloud REST | — |

**Principios para no quedar atrapados:**
- **Firma y deploy en producción: SDK de Casper directo** (maduro). El **CSPR.click Skill** es acelerador de desarrollo, no dependencia de runtime.
- **Lectura: CSPR.cloud** (enterprise, estable). Los **MCP** (Casper MCP community, CSPR.trade MCP) van **detrás de una interfaz** `ChainReader`/`Dex` — si su madurez molesta, se degrada a REST sin tocar la lógica.
- **Los agentes tienen identidad on-chain propia** (cuentas Casper) → alinea con "agents operate with their own on-chain identity" del toolkit y con la narrativa de reputación verificable (#2).
- **Ninguna decisión que mueve dinero la toma el LLM**: el modelo orquesta; el contrato autoriza (ya correcto en `ohu.md` §6, se mantiene).

---

## 6. Simplificaciones operativas para el MVP (mandato: no complejizar en extremo)

Cosas de `ohu.md` que son correctas a futuro pero **innecesariamente complejas para el MVP** — las difiero explícitamente para no inflar la operación:

| Feature | En el MVP | Por qué |
|:--|:--|:--|
| **Subasta inversa sellada (commit-reveal)** | ➡️ **RFQ simple** (oferta abierta, gana mejor precio que cumple spec + reputación) | El commit-reveal solo hace falta contra *bid-sniping* a escala. Para el demo añade complejidad sin valor. Sellado = hardening v2. |
| **Micro-bono de atestación negativa** | ➡️ Parámetro **mínimo** (o "stake simbólico") | Mantiene el incentivo anti-fraude sin fricción de onboarding. Se sube con datos reales. |
| **LPs externos en `MutualPool`** | ➡️ **Mutual cerrada** (solo primas) | Más simple, más defendible. LPs/CEP-18 = roadmap (ya recomendado en `ohu.md` §15). |
| **Swap del payout (CSPR.trade MCP)** | ➡️ Pago en **CSPR/stable única** | Quita una dependencia 🟠 del camino crítico. Swap = opción posterior. |
| **Logística / couriers x402** | ➡️ **Fuera de scope**; solo atestación | Ya recomendado en §15. Mantiene el foco en aggregation + settlement. |

> El núcleo demostrable se reduce a: **aggregation (RFQ) → escrow → atestación gasless → liquidación paramétrica → reputación**, más **x402 oracle-as-a-service** como exhibición del riel B. Eso es robusto, pulido y construible en 30 días.

---

## 7. Fit con el hackathon — cómo queda después de las correcciones

| Criterio del jurado | Cómo lo cubrimos (post-corrección) |
|:--|:--|
| **Working Smart Contracts** | 4 contratos Odra desplegados en Testnet, con tx ricas (escrow, atestación, settlement, slash, prima). |
| **Use of AI / Agentic Systems** | 3 agentes con identidad on-chain propia, coordinando (dirección **#3**). |
| **Real-World Applicability (DeFi & RWA)** | Procura cooperativa real + mutual paramétrica + factoring (roadmap); beneficiario humano. |
| **Innovation** | Mutual paramétrica por atestación multiparte gasless + oráculo de reputación x402. |
| **Technical Execution** | Stack 100% sobre tech viva; semántica x402 correcta; multisig nativo; EIP-712 on-chain. |
| **Direcciones-ejemplo** | **#2** (RWA oracle x402, literal) + **#3** (multi-agente). x402 central vía Riel B. |
| **Long-Term Launch** | Onboarding gasless = adopción real; tu anclaje de cooperativa/restaurantes reales. |
| **Puerta de elegibilidad** | Decenas de tx on-chain por lote — el on-chain *es* el producto. |

**Neto:** las correcciones **no bajan** el fit; lo **suben** (semántica honesta + una dirección-ejemplo mapeada literal + dos piezas más del toolkit exhibidas: casper-eip-712 y CSPR.cloud streaming).

---

## 8. Sprint de validación 72h — reescrito para blindar las dependencias

Orden por riesgo. Cada check tiene **fallback pre-acordado** para que **jamás** haya que descartar nada a mitad de camino:

1. **[CORE] Deploy Odra en Testnet** — `OhuVault` *hello world* con purse; depósito y transferencia. *Fallback: ninguno necesario (maduro).*
2. **[CORE] Multisig nativo de capital** — cuenta admin con claves asociadas ponderadas + threshold alto; cuenta agente que llama un entrypoint capado pero **NO** puede ejecutar un retiro. *Valida la propiedad "el agente no drena".*
3. **[CORE] Verificación EIP-712 on-chain** — portar el ejemplo `permit` de `casper-eip-712` a Odra: una atestación firmada off-chain, verificada en el contrato. *Fallback: si la verificación on-chain costara, atestación con firma ed25519 simple validada en contrato (downgrade menor).*
4. **[CORE] Un cobro x402 real en Testnet** — levantar `make-software/casper-x402` contra el facilitator. *Fallback pre-acordado: si el facilitator hosted es mainnet-only, correr el **facilitator de referencia local** apuntando a Testnet. Nunca quedamos bloqueados.*
5. **[OPC] CSPR.cloud Streaming** — suscribir eventos de atestación para el dashboard. *Fallback: polling REST.*
6. **[OPC] CSPR.trade MCP** — un quote de swap. *Fallback: omitir (fuera del MVP).*
7. **[GTM] Anclar la cooperativa/restaurantes reales** — empezar día 1.

> Si 1-4 pasan (todo 🟢), hay **luz verde técnica total**. Y como cada uno tiene fallback, el peor caso degrada calidad de demo, **no viabilidad**.

---

## 9. Correcciones concretas a aplicar en `ohu.md`

Checklist preciso (avísame y las aplico):

1. **§2, §5.1, §7, §0 — "Contrato-Cuenta" / "contrato-como-cuenta (Casper 2.0)":** reemplazar por **"contrato con bóveda (purse) + multisig nativo a nivel cuenta"**. Quitar toda dependencia del Addressable Entity model (no activado en mainnet).
2. **§5 punto 1 y §6 (tabla, columna "Llaves"):** el agente **no es una sub-clave del contrato**. Tiene **cuenta propia** autorizada solo para entrypoints capados. El capital lo protege la **cuenta admin multisig** + **M-de-N en el contrato**. Reescribir el esquema de claves según §2 de este doc.
3. **§3 (state machine), §6, §7, §8, §14 — "paga al productor vía x402" / "cada pago es x402":** corregir a **dos rieles** (§3 aquí). Settlement de escrow = transferencia del vault (no x402). Reservar la etiqueta "x402" para el Riel B (oráculo-as-a-service + consumo de servicios + facilitator). En el guion del vídeo (§14), el payout al productor se muestra como **"settlement instantáneo"**, no como x402.
4. **§7 (Por qué x402 es el riel correcto):** reescribir distinguiendo Riel A vs Riel B; añadir el **oráculo de reputación x402 (#2)** como uso estrella.
5. **Nuevo (§4 aquí) — atestaciones/onboarding gasless con casper-eip-712:** incorporar al diseño (es mejora de UX/fit, no opcional).
6. **§4.x y §3 — subasta sellada:** marcar **RFQ simple para el MVP**, commit-reveal a v2 (simplificación operativa).
7. **§16 (Fuentes):** añadir las fuentes verificadas de §10 aquí (facilitator docs, casper-x402, casper-eip-712, estado de Addressable Entity).

Ninguna de estas correcciones cambia la **idea** ni el **pitch**; ajustan la **implementación** para que sea 100% construible hoy y semánticamente correcta.

---

## 10. Fuentes (verificadas en esta revisión, jun 2026)

- [Casper AI Toolkit (oficial) — x402, MCP, CSPR.click, Odra](https://www.casper.network/ai) — mecanismo x402 (ed25519, `X-Payment`), facilitator docs, CSPR.click Skill, CSPR.trade MCP.
- [Casper lanza AI Toolkit con x402 vivo en mainnet — Chainwire](https://chainwire.org/2026/06/04/casper-network-launches-ai-toolkit-becoming-first-webassembly-native-blockchain-with-live-x402-payments/)
- [x402 Facilitator API — CSPR.cloud docs](https://docs.cspr.cloud/x402-facilitator-api/reference)
- [`make-software/casper-x402` — impl. de referencia + ejemplos](https://github.com/make-software/casper-x402)
- [`casper-ecosystem/casper-eip-712` — typed-data signing + verificación on-chain en Odra](https://github.com/casper-ecosystem/casper-eip-712)
- [Casper v2.2.0 en mainnet (23-mar-2026)](https://www.casper.network/news/casper-2-2-0-goes-live-on-mainnet) — **no** activó Addressable Entity.
- [Casper v2.0 "Condor" — cambios principales (OriginStake)](https://insight.originstake.com/casper/casper-v2-0-release-major-changes-at-a-glance/) — Addressable Entity **implementado pero no activado**.
- [Casper v2.0 Release Notes / Condor docs](https://docs.casper.network/condor/index)
- [CSPR.trade MCP — 24 tools, hosted](https://mcp.cspr.trade/)
- [Odra framework — docs](https://odra.dev/docs/) · [`odra.dev/llms.txt`](https://odra.dev/llms.txt)
- [CSPR.cloud — APIs REST/Streaming/Node](https://docs.cspr.cloud/)

> Estado a **junio 2026**. Antes de comprometer arquitectura, re-confirmar en el sprint el **soporte testnet del facilitator x402** (check #4) — es la única variable con incertidumbre residual, y tiene fallback local pre-acordado.
