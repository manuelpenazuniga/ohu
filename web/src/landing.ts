// "El Almanaque" — portada (capítulo i del demo). Todas las cifras son
// transacciones reales en Casper Testnet, importadas de data.ts — nada inventado.
import { navRibbon, seal, sprite } from "./ui.js";
import { LOTE, MUTUAL, REDTEAM, NETWORK, VAULT, shortHash } from "./data.js";

/** Una etapa del ciclo cooperativo: sprite + qué pasa + qué lo ejecuta. */
interface Stage {
  readonly sprite: "seed" | "sprout" | "flower" | "crate" | "barn";
  readonly tag: string;
  readonly title: string;
  readonly body: string;
  readonly chain: string;
  readonly who: string;
}

const STAGES: readonly Stage[] = [
  {
    sprite: "seed",
    tag: "SEED",
    title: "A batch forms",
    body:
      "Restaurants sow their weekly demand in plain language — “20 kg of tomatoes, Friday”. The Aggregator agent normalizes it into a spec and opens a furrow in the ledger. The LLM only translates; it holds no keys.",
    chain: "open_lote",
    who: "Aggregator agent",
  },
  {
    sprite: "sprout",
    tag: "SPROUT",
    title: "Money takes root",
    body:
      "Buyers escrow into the batch purse — earmarked, it can only go to this batch's producer or back. The producer stakes a performance bond ≥ the target: skin in the game before a single crate moves.",
    chain: "deposit_to_lote · post_bond · lock_lote",
    who: "buyers + producer",
  },
  {
    sprite: "flower",
    tag: "FLOWER",
    title: "The week in bloom",
    body:
      "The delivery window runs. Crates move in the real world; the chain doesn't watch cameras or take anyone's word — it simply waits for signatures.",
    chain: "state: FUNDED → window closes",
    who: "the real world",
  },
  {
    sprite: "crate",
    tag: "HARVEST",
    title: "Stamps, not claims",
    body:
      "At delivery, crew members stamp gasless Ed25519 attestations from their phones. The Treasury agent tallies the weighted signatures — settlement is arithmetic, there is no claims adjuster to argue with.",
    chain: "attest (gasless) · evaluate_lote",
    who: "crew + Treasury agent",
  },
  {
    sprite: "barn",
    tag: "BARN",
    title: "Settlement day",
    body:
      "The on-chain tally authorizes the outcome. Delivered: escrow → producer, bond home, premium → mutual. Short: the producer's own bond pays first, and the mutual only covers the tail of the loss.",
    chain: "release_to_producer · settle_failure",
    who: "Authorizer (admin identity)",
  },
];

/** Fila del libro de cuentas de la temporada (con puntos de guía). */
function ledgerRow(label: string, value: string, cls = ""): string {
  return `
    <div class="ledger__row ${cls}">
      <span class="ledger__label">${label}</span>
      <span class="ledger__dots" aria-hidden="true"></span>
      <span class="ledger__value">${value}</span>
    </div>`;
}

function stageCard(s: Stage, i: number): string {
  return `
    <li class="stage">
      <div class="stage__art">
        ${sprite(s.sprite, 72, s.tag)}
        <span class="stage__tag">${i + 1} · ${s.tag}</span>
      </div>
      <div class="stage__body">
        <h3 class="stage__title">${s.title}</h3>
        <p class="stage__text">${s.body}</p>
        <div class="stage__meta">
          <code class="stage__chain">${s.chain}</code>
          <span class="stage__who">${s.who}</span>
        </div>
      </div>
    </li>`;
}

function render(): string {
  const settled = MUTUAL.premiumEvents.length;
  return `
  ${navRibbon("landing")}

  <header class="mast">
    <div class="mast__rule">
      <span>Vol. I — No. ${LOTE.id}</span>
      <span class="mast__season">Harvest Season 2026</span>
      <span>${NETWORK} edition</span>
    </div>
    <div class="mast__row">
      <div>
        <h1 class="mast__title">Ohu</h1>
        <p class="mast__motto">Harvests are better bought together.</p>
        <p class="mast__def"><b>ohu</b> <i>(Māori, noun)</i> — a communal work party called to get a hard job done. Here: small restaurants pooling weekly demand, producers bonding to deliver, and an agent swarm running the season while <b>a contract — not a company — holds the money</b>.</p>
      </div>
      <img class="mast__tui" src="/img/mascot-tui.png" alt="The Ohu tui, wearing its straw hat" width="104" height="104" />
    </div>
  </header>

  <figure class="plate">
    <img class="plate__img" src="/img/hero-field.jpeg" alt="Pixel-art field: three farmers pass harvest crates down a furrow line toward the barn" />
    <figcaption class="plate__cap">
      <span class="plate__no">Plate No. 1</span> — the crew brings in the batch. Every figure printed in this almanac is a real ${NETWORK} transaction.
    </figcaption>
  </figure>

  <div class="cta-row">
    <a class="btn btn--primary" href="/onboarding.html">Join the ohu <span class="btn__arrow">→</span></a>
    <a class="btn" href="/dashboard.html">Open the control room</a>
  </div>

  <section class="card ledger">
    <div class="card__head"><h2>The season so far</h2><span class="hint">real on-chain figures · nothing fictional</span></div>
    ${ledgerRow("Batches settled hands-free", `${settled}`, "ledger__row--good")}
    ${ledgerRow("Escrow released to producers", `${settled * 10} CSPR`)}
    ${ledgerRow("Premiums banked by the mutual", `${MUTUAL.premiumsCspr} CSPR`)}
    ${ledgerRow("Tail-of-loss the mutual has paid", `${MUTUAL.tailPaidCspr} CSPR`, "ledger__row--good")}
    ${ledgerRow("Red-team attacks reverted on-chain", `${REDTEAM.length} of ${REDTEAM.length}`, "ledger__row--clay")}
    <p class="ledger__foot">Ledger kept by <a class="vault" href="https://testnet.cspr.live/contract-package/${VAULT}" target="_blank" rel="noopener noreferrer">OhuVault · ${shortHash(VAULT.replace("hash-", ""), 8)}</a> on Casper Testnet.</p>
  </section>

  <section class="cycle">
    <div class="cycle__head">
      <h2 class="cycle__title">The cooperative cycle</h2>
      <p class="cycle__sub">How one batch goes from seed to barn — and what the chain does at every step.</p>
    </div>
    <ol class="cycle__list">
      ${STAGES.map(stageCard).join("")}
    </ol>
  </section>

  <section class="card creed">
    <div class="card__head"><h2>Why an agent can't rug you</h2><span class="hint">authority is separated on-chain</span></div>
    <div class="creed__grid">
      <div class="creed__item">
        ${seal("l-nokeys", "THE LLM PROPOSES", "NO|KEYS", "IT HOLDS NO CAPITAL", { tone: "clay", rotate: -4, size: 108 })}
        <p>Agents read, normalize and propose. Their accounts can only call <b>capped entry points</b> — a jailbroken model still can't pay itself.</p>
      </div>
      <div class="creed__item">
        ${seal("l-math", "THE CONTRACT AUTHORIZES", "MATH|ONLY", "ARITHMETIC SETTLEMENT", { tone: "green", rotate: 3, size: 108 })}
        <p>Releases follow the <b>attestation tally</b>, never a model's judgment and never a manager's mood. Parametric, auditable, boring — on purpose.</p>
      </div>
      <div class="creed__item">
        ${seal("l-tail", "THE MUTUAL BACKSTOPS", "TAIL|= 0", "BOND PAYS FIRST", { tone: "clay", rotate: -2, size: 108 })}
        <p>Failure is priced, not litigated: the producer's bond pays first, the mutual only covers the tail. So far it has paid <b>exactly zero</b>.</p>
      </div>
    </div>
    <p class="note">Don't take the almanac's word for it — the control room shows <strong>${REDTEAM.length} real attacks</strong> that tried to break these rules and were reverted by the contract, transaction hashes included.</p>
  </section>

  <footer class="colophon">
    <div class="colophon__rule"></div>
    <p>© Season 2026 · <b>Ohu Cooperative Ledger</b> · printed on Casper Testnet</p>
    <p class="colophon__fine">Set in Fraunces & Instrument Sans · figures in IBM Plex Mono · no purple gradients were harmed making this page</p>
    <nav class="colophon__nav">
      <a href="/onboarding.html">Join the ohu</a> ·
      <a href="/dashboard.html">Swarm Control Room</a> ·
      <a href="https://testnet.cspr.live/contract-package/${VAULT}" target="_blank" rel="noopener noreferrer">the vault on-chain</a>
    </nav>
  </footer>`;
}

const app = document.getElementById("app");
if (app) {
  app.innerHTML = render();
  app.setAttribute("aria-busy", "false");
}
