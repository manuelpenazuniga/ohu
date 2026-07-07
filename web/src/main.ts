// "El Almanaque" — Swarm Control Room (capítulo iii del demo).
// UI en inglés; cada paso del lote lleva su explicación (qué hace y por qué).
import { navRibbon, seal, sprite, loadCrew, type CYCLE_SPRITES } from "./ui.js";
import {
  LOTE,
  VAULT,
  NETWORK,
  OPERATOR,
  ADMIN,
  AGENTS,
  MUTUAL,
  REDTEAM,
  INVARIANTS,
  AUDIT_LENSES,
  CASE_STUDIES,
  shortHash,
  explorerUrl,
  type LoteStep,
  type Agent,
  type RedTeamAttempt,
} from "./data.js";

/** Enlace monospace a una tx en el explorer (nueva pestaña). */
function txLink(tx: string): string {
  return `<a class="tx" href="${explorerUrl(tx)}" target="_blank" rel="noopener noreferrer" title="${tx}">${shortHash(tx)}</a>`;
}

/** Estado on-chain → etapa del ciclo de cultivo + su sprite (El Almanaque). */
const CROP: Record<string, { label: string; sprite?: keyof typeof CYCLE_SPRITES }> = {
  OPEN: { label: "Seed", sprite: "seed" },
  FUNDED: { label: "Sprout", sprite: "sprout" },
  EVAL_OK: { label: "Harvest", sprite: "crate" },
  EVAL_FAIL: { label: "Blight" },
  SETTLED_OK: { label: "Barn", sprite: "barn" },
  SETTLED_FAIL: { label: "Shortfall" },
};

/** Un paso del stepper (ciclo de cultivo) con su explicación. */
function stepCard(s: LoteStep): string {
  const agent = s.kind === "agent";
  const crop = CROP[s.state];
  const dot = crop?.sprite
    ? `<span class="step__spr">${sprite(crop.sprite, 30, crop.label)}</span>`
    : `<span aria-hidden="true">${crop ? crop.label.charAt(0) : ""}</span>`;
  return `
    <li class="step ${agent ? "step--agent" : "step--setup"}">
      <div class="step__dot">${dot}</div>
      <div class="step__body">
        <div class="step__head">
          <span class="step__state">${s.state}</span>
          ${crop ? `<span class="crop">${crop.label}</span>` : ""}
          ${agent ? `<span class="badge">CREW · ${s.column}</span>` : ""}
        </div>
        <code class="step__ep">${s.entrypoint}()</code>
        <p class="step__why">${s.explain}</p>
        <div class="step__meta"><span class="actor actor--${s.actor}">${s.actor}</span> ${txLink(s.tx)}</div>
      </div>
    </li>`;
}

/** Una columna del feed del enjambre. */
function swarmColumn(
  column: "PROPOSES" | "AUTHORIZES",
  role: string,
  account: string,
): string {
  const step = LOTE.steps.find((s) => s.column === column)!;
  return `
    <div class="swarm__col swarm__col--${column.toLowerCase()}">
      <div class="swarm__label">${column}</div>
      <div class="swarm__role">${role}</div>
      <code class="swarm__acct" title="${account}">${shortHash(account.replace("account-hash-", ""), 8)}</code>
      <div class="swarm__action">
        <code>${step.entrypoint}()</code>
        <span class="arrow">→</span>
        <span class="result">${step.result}</span>
      </div>
      ${txLink(step.tx)}
    </div>`;
}

/** Tarjeta de un agente del enjambre (live o roadmap). */
function agentCard(a: Agent): string {
  const acct = a.account
    ? `<code class="ag__acct" title="${a.account}">${shortHash(a.account.replace("account-hash-", ""), 8)}</code>`
    : `<code class="ag__acct ag__acct--none">— no account yet —</code>`;
  const last = a.lastAction
    ? `<div class="ag__last">${a.lastAction}${a.lastTx ? ` ${txLink(a.lastTx)}` : ""}</div>`
    : "";
  return `
    <div class="ag ag--${a.status}">
      <div class="ag__top">
        <span class="ag__name">${a.name}</span>
        <span class="ag__status ag__status--${a.status}">${a.status === "live" ? "LIVE" : "ROADMAP"}</span>
      </div>
      <div class="ag__role">${a.role}</div>
      ${acct}
      <p class="ag__does">${a.does}</p>
      <div class="ag__auth">${a.authority}</div>
      ${last}
    </div>`;
}

/** Sección de la mutual: reserva + primas + cola pagada. */
function mutualSection(): string {
  const { reserveCspr, premiumsCspr, tailPaidCspr, premiumEvents, note, pool } = MUTUAL;
  return `
  <section class="card">
    <div class="card__head"><h2>The mutual</h2><span class="hint">parametric backstop</span></div>
    <a class="vault" href="https://testnet.cspr.live/contract-package/${pool}" target="_blank" rel="noopener noreferrer">MutualPool · ${shortHash(pool.replace("hash-", ""), 8)}</a>
    <div class="mutual__stats">
      <div class="stat"><span class="stat__n">${reserveCspr}</span><span class="stat__l">reserve · CSPR</span></div>
      <div class="stat"><span class="stat__n">${premiumsCspr}</span><span class="stat__l">premiums in</span></div>
      <div class="stat"><span class="stat__n stat__n--good">${tailPaidCspr}</span><span class="stat__l">tail paid</span></div>
    </div>
    <div class="mutual__bar" role="img" aria-label="reserve fed by premiums, zero tail paid">
      ${premiumEvents.map((e) => `<span class="seg" style="flex:${e.cspr}" title="batch #${e.lote}: +${e.cspr} CSPR premium"></span>`).join("")}
      <span class="seg seg--empty" style="flex:0.4" title="unused tail capacity (tail = 0)"></span>
    </div>
    <p class="note">${note}</p>
  </section>`;
}

/** F2 · una fila de intento de red-team (el contrato lo rechazó). */
function redAttempt(a: RedTeamAttempt): string {
  return `
    <li class="rt">
      <div class="rt__main">
        <span class="rt__attack">${a.attack}</span>
        <code class="rt__ep">${a.by} → ${a.entrypoint}</code>
      </div>
      <div class="rt__verdict">
        <span class="rt__badge">REVERT · ${a.error}</span>
        <span class="rt__prot">${a.protection}</span>
        <a class="tx rt__tx" href="${explorerUrl(a.tx)}" target="_blank" rel="noopener noreferrer">${shortHash(a.tx)}</a>
      </div>
    </li>`;
}

/** F2 · sección "Try to drain the vault": ataques reales que revirtieron. */
function redteamSection(): string {
  return `
  <section class="card">
    <div class="card__head"><h2>Red-team · try to drain the vault</h2><span class="hint">3 real attacks · all reverted on-chain</span></div>
    <p class="note">Each of these was <strong>sent to Testnet</strong> and the contract <strong>rejected it</strong> on-chain — three different protections. The most reassuring test is the one that fails on purpose.</p>
    <ol class="rt-list">
      ${REDTEAM.map(redAttempt).join("")}
    </ol>
  </section>`;
}

/** F10 · sección Trust: el proceso multi-modelo + los casos reales cazados. */
function trustSection(): string {
  return `
  <section class="card">
    <div class="card__head"><h2>Trust · why an agent can't rug you</h2><span class="hint">the process is the product</span></div>
    <div class="trust__lenses">
      <span class="trust__lead">Every change that touches funds passes three lenses:</span>
      ${AUDIT_LENSES.map((l) => `<span class="lens"><b>${l.model}</b> · ${l.lens}</span>`).join("")}
    </div>
    <div class="trust__caseshead">REAL bugs this process caught — that green tests missed:</div>
    <div class="trust__cases">
      ${CASE_STUDIES.map((c) => `
        <div class="case">
          <div class="case__bug">${c.bug}</div>
          <div class="case__meta"><span class="case__caught"><b class="case__tag">caught</b> ${c.caughtBy}</span><span class="case__missed"><b class="case__tag">missed</b> ${c.missedBy}</span></div>
          <div class="case__fix">→ ${c.fix}</div>
        </div>`).join("")}
    </div>
    <details class="trust__inv">
      <summary>The 7 invariants enforced on-chain</summary>
      <ul class="inv-list">
        ${INVARIANTS.map((i) => `<li><code>${i.id}</code> ${i.text}</li>`).join("")}
      </ul>
    </details>
  </section>`;
}

function render(): string {
  const crew = loadCrew();
  const crewChip = crew
    ? `<span class="crew-chip" title="${crew.account}">crew · ${crew.name}</span>`
    : "";
  return `
  ${navRibbon("dashboard")}
  <header class="hero">
    <div class="hero__brand">Chapter iii — the control room</div>
    <h1 class="hero__lema">The LLM orchestrates; the contract authorizes.</h1>
    <p class="hero__sub">Live ledger of batch #${LOTE.id} · <span class="net">${NETWORK}</span> · every hash below is a real on-chain transaction ${crewChip}</p>
  </header>

  <section class="card card--sealed">
    <div class="card__head">
      <h2>Batch #${LOTE.id} — one full season, hands-free</h2>
      <div class="chips">
        <span class="chip">funded ${LOTE.funded}</span>
        <span class="chip">bond ${LOTE.bond}</span>
        <span class="chip">premium ${LOTE.premiumBps / 100}%</span>
        <span class="chip">quorum-fail ${LOTE.quorumFailBps / 100}%</span>
      </div>
    </div>
    <a class="vault" href="https://testnet.cspr.live/contract-package/${VAULT}" target="_blank" rel="noopener noreferrer">OhuVault · ${shortHash(VAULT.replace("hash-", ""), 8)}</a>
    <div class="card__seal">${seal("d-settled", "SETTLED ON CASPER", `BATCH #${LOTE.id}|2026-07-07`, "OHU COOPERATIVE LEDGER", { tone: "green", rotate: 4, size: 128 })}</div>
    <ol class="stepper">
      ${LOTE.steps.map(stepCard).join("")}
    </ol>
  </section>

  <section class="card swarm">
    <div class="card__head"><h2>The swarm</h2><span class="hint">4 agents live · separated authority</span></div>
    <div class="ag__grid">
      ${AGENTS.map(agentCard).join("")}
    </div>
    <div class="swarm__sub">Last round · PROPOSES → AUTHORIZES</div>
    <div class="swarm__grid">
      ${swarmColumn("PROPOSES", "Treasury · operator", OPERATOR)}
      <div class="swarm__link" aria-hidden="true">→</div>
      ${swarmColumn("AUTHORIZES", "Authorizer · admin", ADMIN)}
    </div>
    <p class="note">The <strong>operator can only evaluate</strong> — authorized by the on-chain tally (INV-2), it cannot move capital. Capital is executed by the <strong>admin</strong> (INV-1). Even a jailbroken operator can't pay out: <code>release_to_producer</code> reverts with <code>NotAdmin</code> for anyone but the admin identity.</p>
  </section>

  ${mutualSection()}

  ${redteamSection()}

  ${trustSection()}

  <footer class="foot">
    Every hash is a real transaction on Casper Testnet · <code>error_message=None</code> · batch #${LOTE.id} settled hands-free by the agent swarm.
  </footer>`;
}

const app = document.getElementById("app");
if (app) {
  app.innerHTML = render();
  app.setAttribute("aria-busy", "false");
}
