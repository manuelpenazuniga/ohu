import { describe, it, expect } from "vitest";
import * as ed from "@noble/ed25519";
import { signAttestation, buildMessage } from "../src/attest.js";

const hexToBytes = (h: string): Uint8Array => {
  const out = new Uint8Array(h.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(h.slice(i * 2, i * 2 + 2), 16);
  return out;
};

describe("F3 · atestación gasless firmada en el navegador", () => {
  it("el mensaje tiene el layout exacto del contrato (80 bytes)", () => {
    const msg = buildMessage(4n, 7n, true, 999n);
    // "OhuAttestation:"(15) + lote(8) + nonce(8) + received(1) + contract(32) + chain(8) + valid(8)
    expect(msg.length).toBe(80);
    expect(new TextDecoder().decode(msg.slice(0, 15))).toBe("OhuAttestation:");
    expect(msg[31]).toBe(1); // received=true: prefix(15)+lote_id(8)+nonce(8) → byte 31
  });

  it("produce una firma Ed25519 VÁLIDA sobre ese mensaje", async () => {
    const a = await signAttestation(4n, true, 1000n);
    expect(a.publicKey).toMatch(/^[0-9a-f]{64}$/); // 32 bytes
    expect(a.signature).toMatch(/^[0-9a-f]{128}$/); // 64 bytes
    const msg = buildMessage(4n, 1000n, true, 1000n + 60000n + 30000n);
    const ok = await ed.verifyAsync(hexToBytes(a.signature), msg, hexToBytes(a.publicKey));
    expect(ok).toBe(true);
  });

  it("✓ y ✗ producen firmas distintas (received cambia el mensaje)", async () => {
    const yes = await signAttestation(4n, true, 5n);
    const no = await signAttestation(4n, false, 5n);
    expect(yes.signature).not.toBe(no.signature);
    expect(yes.publicKey).toBe(no.publicKey); // misma llave del comprador
  });
});
