# Ohu

### Procura cooperativa agéntica + mutual paramétrica sobre Casper
**Casper Agentic Buildathon 2026 · desarrollo en profundidad · v2.1 · 2026-07-06**

> ***Ohu*** *— en māori, la cuadrilla de trabajo comunal que se junta a levantar la casa de reunión y a cosechar el kūmara: fuerza colectiva por reciprocidad, no por dinero ni por mandato. Ohu es ese gesto vuelto protocolo — muchos compradores y productores chicos que, juntando su demanda, consiguen lo que solos no alcanzan, y se respaldan entre sí.*

> *"El poder de compra de una gran cadena, para el restaurante de barrio y el productor chico — operado por un enjambre de agentes, garantizado por una mutual que se paga sola, y donde ni la IA puede tocar el capital de nadie."*

> **v2.1 (2026-07-06) — barrido de etiquetado honesto (P0-5):** las atestaciones IMPLEMENTADAS son
> **Ed25519 + domain separation** (`verifying_contract` + `chain_id` + `valid_before`), verificadas
> on-chain — el fallback pre-acordado de `techs-specs.md §8.3`. **"EIP-712 typed-data" es roadmap, NO
> lo implementado hoy**; donde este doc dice "EIP-712" para la atestación, léase "firma Ed25519 gasless
> (EIP-712 typed-data en roadmap)". (El riel x402 SÍ usa EIP-712 real para sus pagos — eso es aparte.)
>
> **v2 (2026-06-25):** corregido tras la due diligence técnica de `techs-specs.md`. Cambios de fondo: (1) **custodia sin Addressable Entity** — contrato con bóveda (`purse`) + multisig nativo a nivel cuenta + M-de-N en contrato (el modelo "contrato-cuenta" de Casper 2.0 **no está activado ni en mainnet ni en testnet**); (2) **dos rieles de valor** — settlement de escrow (contrato + firma Ed25519 gasless) vs. comercio de servicios x402 (genuino); (3) **atestaciones/onboarding *gasless*** (Ed25519 + domain separation; EIP-712 typed-data en roadmap); (4) **RFQ simple** en el MVP (subasta sellada a v2). Todo el stack es construible **hoy en testnet** (la red del hackathon) sin walk-back hacia mainnet.

Continúa la idea #1 de `docs/brainstorming/ahora-hare-el-trabajo-bien.md`. Respeta las 6 reglas de calidad: datos de circuito cerrado, liquidación paramétrica (nunca peritaje), valor ≥ complejidad, el agente solo hace lo que hace bien, exprimir la tech de Casper, beneficiario humano votable.

---

## 1. El problema, en términos que un operador reconoce

Dos lados que se necesitan y no se encuentran con eficiencia:

- **El comprador chico** (restaurante, casino institucional, almacén, panadería) compra al distribuidor pagando **15-25% sobre el precio mayorista**, porque solo no alcanza el volumen mínimo del productor ni el del mayorista de primera mano. Además absorbe el riesgo de que un día no llegue el pedido y tenga que cerrar la cocina.
- **El productor chico** vende a precio de remate a un intermediario que le paga **a 30-60 días**, porque no tiene acceso directo a una demanda agregada y estable.

La pérdida de margen en el medio **no es un problema de información** (no hace falta un oráculo de precios). Es un **problema de coordinación**: nadie agrega la demanda dispersa ni garantiza el cumplimiento. Eso es exactamente lo que un agente hace bien y un humano hace caro.

**La causa raíz por la que esto no existe ya on-chain:** el cumplimiento. Comprar juntos es fácil; el problema es *"¿y si el proveedor no entrega y ya pagamos?"*. La respuesta tradicional —un seguro con liquidador— es teatro: el liquidador hace tres preguntas y abandona el caso. Ohu resuelve ese nudo con **liquidación paramétrica por atestación multiparte**, que es la única forma honesta de automatizarlo.

---

## 2. La idea en una máquina

```
   COMPRADORES (8-30 PyMEs)                          PRODUCTORES (2-5 por lote)
   cargan demanda semanal                            ofertan en RFQ (oferta abierta, MVP)
   (ítem, cantidad, spec, tope)                      + depositan BONO de cumplimiento
   firman commits GASLESS (Ed25519)                              │
            │                                                     │
            ▼                                                     ▼
   ┌──────────────────────────  Agente AGREGADOR ──────────────────────────┐
   │ normaliza demanda difusa · forma lotes (bin-packing por producto/      │
   │ calidad/zona/ventana) · corre RFQ + clearing determinista · adjudica   │
   └───────────────────────────────────┬───────────────────────────────────┘
                                        ▼
        ┌──── Contrato  "OhuVault" (Odra) · custodia en PURSE ─────────────┐
        │  Estado por LOTE: escrow de compradores · bono del productor          │
        │  PROTECCIÓN DE CAPITAL: cuenta admin multisig NATIVO + M-de-N en      │
        │   el contrato (la cuenta del agente solo llama entrypoints CAPADOS)   │
        │  DISPARADOR PARAMÉTRICO: tally de atestaciones ponderadas por share   │
        └───────▲────────────────────────▲───────────────────────────▲──────────┘
       settlement│ del vault (~segundos)  │ Atestación GASLESS de cada │ Prima 0.5%
       al productor│                      │ comprador "recibí/no recibí"│ a la mutual
        ┌───────┴────────┐      ┌─────────┴──────────┐       ┌──────────┴─────────┐
        │ Agente TESORERÍA│      │ Camino feliz:      │       │ Agente MUTUAL/RIESGO│
        │ libera al       │      │ paga productor +    │       │ pool auto-fondeado: │
        │ productor o     │      │ devuelve bono +     │       │ slashea bono y, de  │
        │ dispara refund  │      │ sube reputación     │       │ ser necesario, cubre│
        │ + indemnización │      │ Camino falla:       │       │ la cola del payout  │
        └─────────────────┘      │ refund + slash +    │       └─────────────────────┘
                                 │ indemniza por regla │
                                 └─────────────────────┘

   Riel B (x402, en paralelo): el ORÁCULO DE REPUTACIÓN + el libro de demanda
   se exponen como API pay-per-request → agentes de terceros pagan vía x402.
```

**Las direcciones-ejemplo del hackathon que cubre** (señal al jurado de que se entendió el brief):
- **#2 RWA Oracle Agents con identidad on-chain** → el oráculo de **reputación con colateral *slashable*** se expone como **servicio x402** (pay-per-request). Confianza sin ZK, sin dato externo inventado. *Mapea la dirección #2 de forma literal.*
- **#3 Multi-Agent Coordination** → 3 agentes (Agregador, Tesorería, Mutual) con **identidad on-chain propia** que coordinan y ejecutan.
- **x402 (la estrella de los $100k)** → riel B: oráculo-as-a-service + consumo de servicios externos por los agentes.

---

## 3. El ciclo de vida de un lote (la máquina de estados)

Cada "ronda" de compra es una máquina de estados determinista. El agente *orquesta* las transiciones; el **contrato las autoriza**. Ninguna decisión que mueve dinero la toma un LLM solo.

| Estado | Qué pasa | Quién dispara | Tx on-chain |
|:--|:--|:--|:--|
| `OPEN` | Compradores cargan demanda. Firman el commit **gasless** (Ed25519 + domain separation); el agente lo retransmite. | Compradores (firma) | commit de demanda |
| `SOURCING` | Agregador forma lotes y abre **RFQ** a productores *(MVP: oferta abierta; subasta sellada commit-reveal = v2)*. | Agente Agregador | apertura de lote |
| `BIDDING` | Productores ofertan precio + compromiso de entrega + **depositan bono**. | Productores | bid + bond deposit |
| `AWARDED` | Clearing determinista: gana precio que cumple spec, ponderado por reputación. | Contrato (regla) | award |
| `FUNDED` | Cada comprador deposita su parte en **escrow** del vault (su propio dinero, retenido). | Compradores | depósito en escrow |
| `FULFILLING` | Productor entrega; ventana de entrega + ventana de atestación abiertas. | Productor (físico) | — (off-chain) |
| `ATTESTING` | Cada comprador atesta recepción con firma **Ed25519 gasless** (domain separation). **Silencio = recibido** (ack por defecto). | Compradores (firma) | atestación verificada on-chain |
| `SETTLED_OK` | Cuota de recepción ≥ umbral → **settlement del vault al productor (~segundos)**, devuelve bono, reputación +. | Contrato + Tesorería | transfer del vault · bond return · rep update |
| `SETTLED_FAIL` | Cuota de **no-recepción** ≥ umbral → refund a compradores, **slash del bono**, **indemnización paramétrica** (desde bono + cola del pool). | Contrato + Mutual | refund · slash · indemnización |
| `DISPUTED` | Reclamo *aislado* (1 comprador contra la mayoría) → micro-disputa con su micro-bono, NO dispara slash del productor. | Contrato | dispute resolve |

> **Clave de diseño (R2 — paramétrico, no peritaje):** el sistema nunca "evalúa un siniestro". Solo cuenta atestaciones ponderadas por la *share* de cada comprador en el lote y compara contra umbrales. Es aritmética sobre evidencia multiparte, no juicio. Las atestaciones son **mensajes firmados Ed25519 con domain separation, off-chain y verificados on-chain** (EIP-712 typed-data en roadmap), así el comprador no necesita tener CSPR ni manejar gas.

---

## 4. La mutual paramétrica — diseño anti-fraude (donde un experto en seguros juzga más fuerte)

El riesgo obvio: *"si pago indemnización cuando los compradores dicen 'no llegó', alguien va a recibir la mercadería y mentir para cobrar el seguro y quedarse con todo."* El diseño hace que **la honestidad sea la estrategia dominante** por construcción:

**4.1 — El que causa la pérdida la paga primero (el bono del productor es el pagador primario).**
El productor deposita un **bono ≥ máxima exposición del lote** (refund + tope de indemnización). Si incumple, su bono se *slashea* y financia el grueso del payout. **La mutual es un backstop de cola y de *timing*, no el pagador principal.** Esto colapsa el riesgo moral y el drenaje del pool: el que falló es el que paga.

**4.2 — La indemnización deja *íntegro*, nunca enriquece.**
Pago al comprador = **devolución de su propio dinero en escrow** (que simplemente NO se libera al productor) **+ una indemnización de disrupción *acotada y fija*** estrictamente menor que el valor de haber recibido la mercadería. Por lo tanto, **un comprador que sí recibió no tiene incentivo a mentir**: renunciaría a bienes que valen más que la indemnización. No se puede "quedarse con la mercadería *y* cobrar".

**4.3 — Asimetría individual vs. colectivo (mata la mentira aislada).**
- La mutual **solo dispara con quórum** de no-recepción (p.ej. **≥ 60% de la share del lote** atesta no-recibido) → señal objetiva de que **el productor** falló, no de que un comprador miente.
- Un comprador solo que reclama contra una mayoría que confirmó → va a `DISPUTED`, **no** dispara slash del productor; se resuelve con su **micro-bono de atestación**.

**4.4 — Atestar tiene skin-in-the-game.**
Para emitir una atestación **negativa**, el comprador arriesga un **micro-bono** *(en el MVP, mínimo/simbólico, para no frenar el onboarding)*. Si su atestación negativa es contradicha por evidencia (su propia firma de recepción previa, o la entrega conjunta en su misma ruta confirmada por otros), pierde el micro-bono. **Mentir cuesta.**

**4.5 — Silencio = recepción (anti-grieffing y anti-gas).**
Si un comprador no hace nada en la ventana, el default es "recibido conforme". Solo una **atestación negativa activa y con bono** abre el camino de reclamo. Esto refleja la realidad (la mayoría de las entregas están bien), minimiza transacciones y elimina el ataque de paralizar por inacción.

**4.6 — Colusión productor + compradores (intentar drenar la mutual).**
No cierra para el atacante: (a) el bono del productor se *slashea* primero (pierde dinero real), (b) la indemnización ≤ fondos propios + tope chico (sin ganancia), (c) lo que se "devuelve" es **el propio dinero de los compradores** en escrow — devolverlo no drena el pool; solo la indemnización de disrupción sale del pool, y está **acotada y respaldada por el bono del propio productor**. Un anillo colusivo paga bonos + primas para extraer una indemnización mínima: **negativo neto**. La economía se cierra.

**4.7 — Solvencia del pool.**
La prima (≈0.5% por transacción exitosa) capitaliza la mutual. Como el bono del productor cubre la pérdida primaria, el pool solo absorbe la **cola** (timing entre slash y payout, y déficit si el bono quedó corto por inflación de daño). Objetivo de reserva: **≥1.5× la pérdida de cola anual esperada**. Si el pool baja del piso, la upgradability nativa sube la prima por gobernanza, sin re-tokenizar nada.

> **En una frase para el jurado:** *Ohu convierte un "seguro" (que requiere un liquidador humano y por eso es falso para un agente) en un "release de escrow condicionado a una atestación multiparte respaldada por colateral" — que es aritmética, no peritaje, y por eso un agente sí lo puede ejecutar de verdad.*

---

## 5. Arquitectura de contratos (Odra · construible HOY en testnet)

Cuatro piezas. Todo el dinero vive en código determinista; los agentes solo orquestan. **Sin dependencia del modelo Addressable Entity** (no activado en ninguna red): se usa lo que está vivo hoy.

**5.1 — `OhuVault` (contrato Odra con bóveda `purse`, actualizable).**
Custodia los compromisos de pago y los bonos por lote en un **purse** (capacidad pre-2.0, viva en testnet y mainnet). Expone entry points con permisos:
- `route_micropayment(...)` — pagos chicos acotados por llamada; **invocable por la cuenta del agente** (operativa, capada).
- `release_to_producer(lote)` — **solo** ejecuta si el contrato verifica que la cuota de recepción ≥ umbral (condición on-chain, no input del agente).
- `settle_failure(lote)` — refund + slash + gatillo de indemnización; misma lógica condicional.
- `reconfigure(...)` / `upgrade(...)` / retiro de emergencia — **requiere `caller == admin`** y **aprobación M-de-N** registrada en el contrato.

**5.2 — `MutualPool` (contrato Odra con purse).** Recibe primas, paga la cola de indemnizaciones, lleva la reserva. *(MVP: mutual cerrada, solo primas. LPs externos con fracciones CEP-18 + yield = roadmap.)*

**5.3 — `Reputation` (contrato).** Lleva score on-chain de productores y compradores: lotes cumplidos, atestaciones honestas, slashes. Es el "oráculo de confianza" (dirección #2) **sin ZK y sin dato externo**, y se **expone como servicio x402** (Riel B): la reputación se construye con el propio historial verificable.

**5.4 — `CoopRegistry` (contrato).** Membresías, KYC ligero, parámetros de gobernanza (prima, umbrales, topes de indemnización), actualizables.

### El esquema de seguridad — con precisión de ingeniero (corregido v2)

Casper **no tiene activado** el modelo "contrato-cuenta con claves propias" (Addressable Entity) ni en mainnet ni en testnet. La misma garantía de seguridad se logra con primitivos **vivos hoy**, repartidos entre **cuentas** y **lógica de contrato**:

1. **Protección de capital = cuenta admin multisig NATIVO.** Existe una **cuenta** `admin` configurada con **claves asociadas ponderadas + thresholds** (p.ej. Productor-rep 60 · Inversores/DAO 40, threshold de deploy alto → co-firma forzada). Esto es nativo desde Casper 1.x, **inalterado y activo**. El contrato gatea sus entrypoints privilegiados a `caller == admin`.
2. **El agente tiene su PROPIA cuenta** (identidad on-chain), registrada en el contrato como `operator`. **NO es una sub-clave del contrato.** Solo está autorizado para entrypoints **capados** (micropagos con tope, abrir lote, registrar atestaciones). *Un LLM comprometido toca, como mucho, micropagos acotados — jamás el capital.*
3. **Releases grandes = M-de-N dentro del contrato.** El movimiento de capital relevante exige `approve(id)` de M firmantes distintos antes de `execute(id)` — patrón determinista en estado del contrato, funciona en cualquier versión de la VM.
4. **Disparador paramétrico = tally ponderado de atestaciones** en el estado del contrato (cada atestación pesa por la *share* de pago del comprador). Aquí vive el patrón "acuerdo multiparte codificado" — como **votos ponderados verificables on-chain**, complementado por las claves asociadas nativas de la cuenta admin.

> Distinción que demuestra dominio del stack: los **thresholds de cuenta son un único número por acción** — la granularidad "monto chico vs. grande" la da **el código del contrato** (entrypoints capados + M-de-N), no los thresholds nativos. Y el "contrato-cuenta" de Casper 2.0 **no se usa**, porque no está activado en ninguna red.

---

## 6. Los tres agentes (qué es LLM y qué es determinista)

Crítico para R4 y para el pitch de seguridad: **la lógica que mueve dinero es código; el LLM hace lo que un LLM hace bien.** Cada agente tiene **cuenta Casper propia** (identidad on-chain).

| Agente | Cuenta / autorización | Hace (LLM) | Hace (determinista) | Tech Casper |
|:--|:--|:--|:--|:--|
| **Agregador** | cuenta propia | normaliza demanda difusa ("unas 20 cajas de tomate, lo que esté bueno") → spec estructurada · **forma lotes** (bin-packing) · redacta RFQ · **conversa con productores** · explica decisiones | clearing de RFQ · validación de spec | CSPR.click Skill / SDK · CSPR.cloud |
| **Tesorería** | cuenta propia, `operator` del vault (capada) | monitorea ventanas de entrega/atestación · maneja excepciones | dispara `release_to_producer` / `settle_failure` (gateado por el contrato) · **settlement del vault** | SDK (ed25519) · CSPR.cloud Streaming · *(CSPR.trade MCP para swap, opcional/roadmap)* |
| **Mutual/Riesgo** | cuenta propia, `operator` del vault | propone ajustes de prima/tope a gobernanza | cobra prima · slashea bono · paga cola de indemnización · vigila reserva | SDK · `MutualPool` |

**Dónde el LLM gana de verdad su lugar:** (1) traducir demanda humana caótica a specs comprables, (2) negociar/comunicar en lenguaje natural con productores, (3) resolver excepciones y explicar el porqué a personas, (4) optimizar la formación de lotes. **Dónde NO decide:** ningún release de capital depende del "juicio" del modelo; siempre lo autoriza una condición on-chain.

---

## 7. Los dos rieles de valor (semántica honesta de x402)

x402 es un **protocolo de pago por servicio/recurso HTTP**, no un riel universal de transferencias. Liberar fondos que **ya están en escrow** no es x402: es una transferencia del contrato. Ohu usa **dos rieles**, cada uno en su lugar.

**Riel A — Liquidación de escrow (contrato `OhuVault` + firma Ed25519 gasless).** Custodia y settlement de los fondos del lote:
- Depósito en escrow del comprador y bono del productor → entran al purse del vault.
- `release_to_producer` / `settle_failure` / refund / slash / indemnización → **transferencias del contrato**, gateadas por el tally paramétrico + autorizaciones **Ed25519 con domain separation** firmadas por las partes (gasless; EIP-712 typed-data en roadmap).
- Finalidad rápida de Casper → el productor ve la plata en **segundos**. *(En el demo se muestra como "settlement instantáneo", no como x402.)*

**Riel B — Comercio de servicios x402 (genuino, central, alineado con los $100k).** x402 para lo que es:
1. **Oráculo de reputación + libro de demanda agregada como API x402** → agentes de terceros y productores **pagan por consulta**. Es la dirección-ejemplo **#2 "RWA Oracle Agent con identidad on-chain"** ejecutada literal, y pay-per-request canónico.
2. **Los agentes consumen servicios externos vía x402** (vetting de productores, cotizaciones de logística, analítica) — el caso M2M del enunciado.
3. **(Opcional)** el **facilitator de x402 como motor de settlement** para pagos *autorizados* salientes (el agente firma, el facilitator liquida en Casper).

> Resultado: x402 sigue **central y prominente** (oráculo-as-a-service + consumo de servicios), pero **honesto**. Sube la credibilidad ante jueces técnicos y mapea una dirección-ejemplo de forma literal.

---

## 8. Economía unitaria (demostrar valor ≥ complejidad, en pesos)

Ejemplo ilustrativo de **un lote semanal** (números a calibrar con datos reales):

| Concepto | Sin Ohu | Con Ohu |
|:--|:--:|:--:|
| 8 restaurantes, canasta semanal | ~\$4.800 (distribuidor, +20%) | **~\$4.000** (mayorista de primera mano) |
| Ahorro del lado comprador | — | **~\$800/sem (~17%)** |
| Lo que recibe el productor | ~\$3.400 a 45 días (intermediario) | **~\$4.000 en segundos** |
| Comisión plataforma (1.5% comprador) | — | ~\$60 |
| Prima mutual (0.5%) | — | ~\$20 al pool |
| Bono del productor (10%, reembolsable) | — | ~\$400 (cubre peor caso) |

**Resultado:** el comprador ahorra ~17%, el productor cobra **más** y **al instante**, y la plataforma gana ~2% del GMV sin tomar riesgo de crédito (el bono y el escrow lo cubren). El ahorro es **obvio y medible** → la regla R3 se cumple con holgura.

A escala: 50 lotes/semana × \$4.000 = **\$200k GMV/sem**; ~\$4k/sem de ingreso de plataforma; ~\$1k/sem capitalizando la mutual. Y, lo importante para el hackathon: **miles de transacciones on-chain reales por semana** (escrow, atestación, settlement) + el volumen x402 del oráculo.

---

## 9. Superficie de transacciones on-chain (la puerta de elegibilidad)

Rica y repetible por diseño — esto es decisivo para pasar la puerta y para el voto:

`commit demanda` · `apertura lote` · `bid + bond deposit` · `award` · `depósito en escrow (×N compradores)` · `settlement del vault al productor` · `atestación verificada (×N)` · `slash de bono` · `prima a la mutual (×lote)` · `refund (×N)` · `indemnización` · `update de reputación` · `consulta x402 al oráculo (×terceros)` · `upgrade de parámetros`.

→ **Decenas de transacciones reales por lote.** El on-chain *es* el producto, no un hash decorativo al final.

---

## 10. Modelo de confianza y seguridad

- **Agente comprometido no drena capital:** cuenta del agente con autorización solo a entrypoints capados + protección de capital en la **cuenta admin multisig nativo** + M-de-N en contrato (§5). El blast-radius de un LLM hackeado es, como mucho, micropagos acotados — nunca el vault.
- **Productor malicioso:** pierde su bono (paga la pérdida que causó) + reputación → expulsión económica.
- **Comprador mentiroso:** micro-bono + asimetría individual/colectivo + silencio=ack → mentir es negativo en valor esperado.
- **Sybil en atestaciones:** las atestaciones pesan por **share real de pago en el lote** (dinero en escrow), no por cabeza → crear cuentas falsas no da poder de voto sin poner dinero real.
- **Datos:** todo es de **circuito cerrado** (órdenes, escrow, atestaciones, historial) → no hay oráculo externo que envenenar (R1).

---

## 11. Plan de 30 días

**Sprint de validación 48-72h (antes de comprometerse) — orden por riesgo, con fallback pre-acordado:**
- [ ] Deploy Odra en **Testnet**: `OhuVault` con purse; depósito y transferencia end-to-end.
- [ ] **Multisig nativo de capital:** cuenta admin con claves asociadas ponderadas + threshold alto; cuenta agente que llama un entrypoint capado pero **NO** puede ejecutar un retiro. Valida "el agente no drena".
- [ ] **Verificación EIP-712 on-chain:** portar el ejemplo `permit` de `casper-eip-712` a Odra (atestación firmada off-chain, verificada en contrato). *Fallback: firma ed25519 simple validada en contrato.*
- [ ] **Un cobro x402 real en Testnet** (`make-software/casper-x402`). *Fallback pre-acordado: facilitator de referencia local apuntando a Testnet → nunca quedamos bloqueados.*
- [ ] Conectar **CSPR.cloud** (lectura/streaming de eventos).
- [ ] Empezar a anclar **una cooperativa / grupo de restaurantes reales** (ventaja decisiva).

**Semana 1 — Núcleo de liquidación.** `OhuVault` + escrow por lote + cuenta admin multisig + un settlement al productor funcionando en Testnet. *Hito: un lote feliz liquida end-to-end.*

**Semana 2 — Atestación gasless + mutual.** Atestación Ed25519 ponderada (gasless; EIP-712 typed-data en roadmap) + camino `SETTLED_FAIL` (refund + slash + indemnización) + `MutualPool` con prima automática. *Hito: un lote que falla indemniza por regla.*

**Semana 3 — Agentes + RFQ + oráculo x402.** Los 3 agentes orquestando; RFQ simple; `Reputation` expuesto como **API x402**. Dashboard con CSPR.cloud. *Hito: ronda completa autónoma + una consulta x402 real al oráculo.*

**Semana 4 — Anclaje real + UX + vídeo + redes.** Traer 1 testimonio real, pulir la app (onboarding gasless), grabar el demo, abrir Telegram/X. *Hito: vídeo que se comparte solo + camino de voto activado.*

---

## 12. Alcance del MVP — qué es real y qué se simula (con honestidad)

- **100% real en el demo:** los contratos, el multisig nativo, las atestaciones Ed25519 (domain separation) verificadas on-chain, los pagos x402 al oráculo en Testnet, y **la liquidación paramétrica ejecutándose en vivo** (feliz y fallida).
- **Honestamente acotado:** un panel chico de compradores/productores (sembrados, o reales si los anclas). **La entrega física se representa por atestaciones firmadas — que es el mecanismo real de todos modos, no un atajo.** No se inventa telemetría ni precios.
- El mensaje al jurado: *"la lógica de liquidación es 100% real y corre en Testnet hoy; lo único chico es la cantidad de participantes del mundo real, y eso se resuelve con go-to-market, no con código."*

---

## 13. Riesgos y mitigaciones (honestidad de ingeniero)

| Riesgo | Mitigación |
|:--|:--|
| x402 en Testnet no listo / inestable | Validar en 72h; **fallback pre-acordado: facilitator local** apuntando a Testnet. No bloquea. |
| ~~Modelo contrato-cuenta no activado~~ | **Resuelto en v2:** custodia con purse + multisig nativo a nivel cuenta + M-de-N (todo vivo hoy). Sin walk-back. |
| Calibrar bono/prima/umbrales sin datos | MVP con parámetros conservadores + upgradability para ajustar por gobernanza; tu experiencia en seguros calibra el peor caso. |
| Madurez de MCP community-built | Core usa **CSPR.cloud/SDK directo**; MCP detrás de interfaz, degradable. |
| No conseguir anclaje real en 4 semanas | Empezar el día 1; con tus premios y red en innovación alimentaria es tu moat. |
| Adopción B2B exige confidencialidad de volúmenes | Roadmap: capa ZK opcional (idea "Sigilo"), **fuera del scope del MVP**. |

---

## 14. Guion del vídeo-demo (gana votos)

1. **Rostro humano (5s):** el dueño de un restaurante de barrio o un productor real. *"Compro caro y a veces no me llega el pedido."*
2. **El click (10s):** 8 restaurantes cargan su demanda **firmando con un toque, sin tener cripto** (gasless) → el agente arma el lote y corre el RFQ → adjudica al productor.
3. **El momento mágico (10s):** el productor entrega, los compradores confirman con un toque, y el **settlement le llega a su wallet en segundos** — en pantalla, el contador de la transacción.
4. **El giro (15s):** simula que un proveedor **no entrega** → los compradores atestan "no llegó" → **la mutual les devuelve la plata y los indemniza sola, sin liquidador** → el bono del proveedor se *slashea* en vivo.
5. **El cierre (10s):** *"Ni siquiera la IA puede tocar el capital de la cooperativa"* → muestra el intento de retiro del agente **rechazado** porque exige la co-firma de la cuenta admin multisig. Cripto invisible, valor visible.

---

## 15. Decisiones abiertas para vos (donde tu dominio decide)

1. **Vertical de arranque:** ¿restaurantes urbanos (ciclo semanal rápido, muy viral) o cooperativa de productores que compra insumos (tu terreno de premio)? Recomiendo **restaurantes** para el demo (velocidad + voto) y mencionar productores en el roadmap.
2. **Tamaño del bono del productor:** ¿10% fijo, o escalado por reputación (productor con historial deposita menos)? Tu experiencia en seguros define la curva.
3. **¿LPs externos en la mutual desde el MVP, o mutual cerrada (solo primas) al inicio?** Recomiendo **cerrada en MVP, LPs en roadmap** (ya reflejado en §5.2).
4. **Logística:** ¿la plataforma coordina couriers o el productor entrega y solo se atesta? Recomiendo **solo atestación en MVP**.

---

## 16. Fuentes

Análisis técnico completo y due diligence del stack en **`techs-specs.md`** (raíz). Fuentes clave verificadas (jun 2026):
- [Casper AI Toolkit (oficial)](https://www.casper.network/ai) — x402 (ed25519, `X-Payment`), CSPR.click Skill, CSPR.trade MCP, Odra.
- [x402 Facilitator API — CSPR.cloud](https://docs.cspr.cloud/x402-facilitator-api/reference) · [`make-software/casper-x402`](https://github.com/make-software/casper-x402)
- [`casper-ecosystem/casper-eip-712`](https://github.com/casper-ecosystem/casper-eip-712) — atestaciones gasless verificadas on-chain en Odra.
- [Casper v2.0 "Condor" — Addressable Entity *no activado*](https://insight.originstake.com/casper/casper-v2-0-release-major-changes-at-a-glance/) (mainnet **y** testnet).
- [Multisig nativo en Casper (claves asociadas)](https://www.casper.network/news/how-to-use-multisig-functionality-on-casper) · [Odra docs](https://odra.dev/docs/) · [CSPR.cloud](https://docs.cspr.cloud/)

> **Recordatorio:** la única incertidumbre residual es el **soporte testnet del facilitator x402** — con fallback local pre-acordado (§13). Todo lo demás está vivo en Testnet hoy.
