# Wrapper para agentes opencode — Ohu

> **Cómo usarlo:** copia TODO el bloque de abajo y pégaselo al agente. Al final, reemplaza
> `<<< PEGA AQUÍ EL BRIEF DE LA TAREA >>>` por el "Brief para opencode" de la tarea (S0…S4) de
> `docs/plan/fase-0-derisk.md`. Con esto, **wrapper + un brief = contexto completo**: el agente NO
> necesita que le copies el resto del plan (lo transversal ya está inlineado aquí).
>
> Lo único que el wrapper NO trae a propósito: la **asignación de modelo** (la eliges tú al abrir
> opencode) y el **Definition of Done de la fase** (no aplica por-tarea; el DoD del agente son los
> criterios de aceptación de su tarea).

---

```
Trabajas en el repositorio Ohu (Casper Agentic Buildathon). Antes de codear:
1) Lee CLAUDE.md (raíz del repo) y, en docs/plan/fase-0-derisk.md, la SECCIÓN COMPLETA de la
   tarea que te asigno abajo (sus criterios de aceptación detallados, que pueden ser más que el
   brief resumido).

== INVARIANTES DE SEGURIDAD (no se rompen; valen para toda tarea) ==
- INV-1: El AGENTE nunca mueve capital relevante. Su cuenta solo puede llamar entrypoints CAPADOS
  (micropago con tope por llamada). Todo release grande exige caller==admin + aprobación M-de-N
  registrada en el contrato.
- INV-2: Ningún release de capital depende del input del agente/LLM: SIEMPRE lo autoriza una
  CONDICIÓN ON-CHAIN (p.ej. tally de atestaciones ponderadas ≥ umbral).
- INV-3: NO usar Addressable Entity (no está activado en Casper, ni mainnet ni testnet).
  Custodia = contrato con `purse` + multisig NATIVO de cuenta (claves asociadas + threshold) +
  M-de-N dentro del contrato.
- INV-4: x402 SOLO para servicios/oráculo HTTP (pay-per-request). El settlement de escrow es una
  TRANSFERENCIA del contrato, NO un flujo x402.
- INV-5: Las atestaciones son mensajes EIP-712 firmados off-chain y verificados ON-CHAIN (gasless
  para el firmante). Silencio = recibido.
- INV-6: Datos de circuito cerrado; liquidación por ARITMÉTICA sobre evidencia multiparte, nunca
  juicio humano.

== STACK Y LAYOUT (monorepo) ==
  contracts/ = Odra (Rust)            agents/ = TypeScript (casper-js-sdk)
  web/       = TypeScript (después)   infra/  = deploy testnet, .env.example, justfile/Makefile
  docs/plan/ = el plan
Toolchain: Rust stable + cargo-odra + casper-client ; Node 20+ + pnpm + casper-js-sdk.
Red objetivo: Casper TESTNET. Secrets fuera de git (usa .env, commitea solo .env.example).

== REGLA ANTI-ALUCINACIÓN ==
No inventes APIs de Odra / casper-eip-712 / casper-x402 / casper-js-sdk. Si no conoces la firma
exacta, consúltala en el doc oficial; si queda duda, deja `// TODO(audit): verificar contra <doc>`.
Un hueco marcado es MEJOR que una API inventada.
Docs: Odra https://odra.dev/docs/ · github.com/casper-ecosystem/casper-eip-712 ·
github.com/make-software/casper-x402 · CSPR.cloud https://docs.cspr.cloud/ ·
Casper AI Toolkit https://www.casper.network/ai

== CÓMO TRABAJAR ==
- Crea y trabaja en una rama: spike/<id-tarea>  (ej.: spike/s0-scaffold).
- Cumple los criterios de aceptación AL PIE DE LA LETRA y ESCRIBE TESTS, incluidos los NEGATIVOS
  (ej.: el agente NO puede drenar; una firma manipulada/replay revierte; x402 no toca el escrow).
- Corre el build y los tests (just build && just test, o el equivalente) y déjalos en verde antes
  de declarar la tarea lista.
- No metas lógica de otras tareas. No toques los invariantes.
- Al terminar, RESUME: qué hiciste, qué tests pasan, y cada TODO(audit) que dejaste pendiente.

== SERÁS AUDITADO CONTRA ==
invariantes aplicables + criterios de aceptación de tu tarea + existencia y paso de los tests
(sobre todo los negativos) + cero APIs inventadas + reproducible desde un clon limpio.

<<< PEGA AQUÍ EL BRIEF DE LA TAREA (S0…S4) >>>
```
