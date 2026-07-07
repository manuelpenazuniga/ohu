/**
 * F3 · Atestación móvil gasless. El comprador ve "¿recibiste el pedido?", toca
 * ✓/✗ y firma LOCALMENTE (Ed25519, en el navegador) el MISMO mensaje que el
 * contrato verifica. El agente lo retransmite (paga el gas). El comprador jamás
 * tuvo CSPR. La llave privada NUNCA sale del navegador.
 *
 * Mensaje (idéntico a `build_attestation_message` on-chain, 80 bytes):
 *   "OhuAttestation:" + lote_id(8 BE) + nonce(8 BE) + received(1) +
 *   verifying_contract(32) + chain_id(8 BE) + valid_before(8 BE)
 */

import * as ed from "@noble/ed25519";

const PREFIX = new TextEncoder().encode("OhuAttestation:");
// verifying_contract = self_address() del OhuVault (package hash, 32 bytes) —
// el fail-E2E on-chain confirmó que este es el valor que el contrato verifica.
const VAULT_HASH_HEX = "94c4d7b466a035e0aac9bb60daeaa179432ad2df93de3dfe2759812676bf3b6c";
const CHAIN_ID = 1n;
const WINDOW_MS = 60_000n;
// Llave Ed25519 SOLO-DEMO (en producción es la del comprador, gestionada por su wallet).
const DEMO_PRIV = hexToBytes("9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60");

function hexToBytes(h: string): Uint8Array {
  const clean = h.replace(/^0x/, "");
  const out = new Uint8Array(clean.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(clean.slice(i * 2, i * 2 + 2), 16);
  return out;
}
function bytesToHex(b: Uint8Array): string {
  return Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");
}
function u64be(n: bigint): Uint8Array {
  const b = new Uint8Array(8);
  new DataView(b.buffer).setBigUint64(0, n, false);
  return b;
}

export function buildMessage(loteId: bigint, nonce: bigint, received: boolean, validBefore: bigint): Uint8Array {
  const parts = [
    PREFIX, u64be(loteId), u64be(nonce), new Uint8Array([received ? 1 : 0]),
    hexToBytes(VAULT_HASH_HEX), u64be(CHAIN_ID), u64be(validBefore),
  ];
  const msg = new Uint8Array(parts.reduce((s, p) => s + p.length, 0));
  let o = 0;
  for (const p of parts) { msg.set(p, o); o += p.length; }
  return msg;
}

export interface SignedAttestation {
  readonly loteId: string;
  readonly nonce: string;
  readonly received: boolean;
  readonly validBefore: string;
  readonly publicKey: string;
  readonly signature: string;
}

/** Firma la atestación localmente (Ed25519). La llave nunca sale del navegador. */
export async function signAttestation(
  loteId: bigint, received: boolean, nowMs: bigint,
): Promise<SignedAttestation> {
  const nonce = nowMs; // demo nonce (u64)
  const validBefore = nowMs + WINDOW_MS + 30_000n;
  const msg = buildMessage(loteId, nonce, received, validBefore);
  const pub = await ed.getPublicKeyAsync(DEMO_PRIV);
  const sig = await ed.signAsync(msg, DEMO_PRIV);
  return {
    loteId: loteId.toString(), nonce: nonce.toString(), received,
    validBefore: validBefore.toString(),
    publicKey: bytesToHex(pub), signature: bytesToHex(sig),
  };
}

// ── UI ──────────────────────────────────────────────────────────────────────
const app = typeof document !== "undefined" ? document.getElementById("app") : null;
const loteId =
  typeof location !== "undefined"
    ? BigInt(new URLSearchParams(location.search).get("lote") ?? "4")
    : 4n;
const short = (h: string) => `${h.slice(0, 10)}…${h.slice(-6)}`;

function screenAsk(): string {
  return `
    <div class="q">¿Recibiste el pedido del <b>lote #${loteId}</b>?</div>
    <div class="btns">
      <button class="b b--ok" data-r="1">✓ Sí, recibido</button>
      <button class="b b--no" data-r="0">✗ No llegó</button>
    </div>
    <p class="foot">Firmas localmente. <b>Nunca tocas CSPR</b> — el agente retransmite y paga el gas.</p>`;
}

function screenDone(a: SignedAttestation): string {
  const relay = (window as unknown as { RELAY_URL?: string }).RELAY_URL;
  return `
    <div class="done ${a.received ? "done--ok" : "done--no"}">${a.received ? "✓ Recepción atestada" : "✗ Falta atestada"}</div>
    <p class="sub">Firmado <b>Ed25519 en tu navegador</b> (la llave nunca salió).</p>
    <div class="payload">
      <div><span>lote</span><code>#${a.loteId}</code></div>
      <div><span>pubkey</span><code>${short(a.publicKey)}</code></div>
      <div><span>firma</span><code>${short(a.signature)}</code></div>
    </div>
    <p class="foot">${relay ? "Enviado al agente para retransmitir…" : "El agente relayer toma este payload y llama <code>verify_attestation</code> pagando el gas."}</p>
    <button class="b b--again" data-again="1">Atestar de nuevo</button>`;
}

function bind(): void {
  if (!app) return;
  app.querySelectorAll<HTMLButtonElement>("button[data-r]").forEach((btn) => {
    btn.addEventListener("click", async () => {
      app.setAttribute("aria-busy", "true");
      const received = btn.dataset["r"] === "1";
      const a = await signAttestation(loteId, received, BigInt(Date.now()));
      // Relay opcional: si hay RELAY_URL, POST; si no, se muestra el payload.
      const relay = (window as unknown as { RELAY_URL?: string }).RELAY_URL;
      if (relay) {
        try { await fetch(relay, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(a) }); } catch { /* demo: ignora */ }
      }
      app.innerHTML = screenDone(a);
      app.setAttribute("aria-busy", "false");
      bind();
      return;
    });
  });
  app.querySelector<HTMLButtonElement>("button[data-again]")?.addEventListener("click", () => {
    app.innerHTML = screenAsk();
    bind();
  });
}

if (app) {
  app.innerHTML = screenAsk();
  bind();
}
