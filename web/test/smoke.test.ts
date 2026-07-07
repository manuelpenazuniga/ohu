import { describe, it, expect } from 'vitest';
import { LOTE, shortHash } from '../src/data.js';

describe('swarm dashboard dataset (lote 4, real on-chain)', () => {
  it('has the full 6-step batch lifecycle', () => {
    expect(LOTE.id).toBe(4);
    expect(LOTE.steps).toHaveLength(6);
  });

  it('has exactly 2 agent steps: PROPOSES then AUTHORIZES', () => {
    const agents = LOTE.steps.filter((s) => s.kind === 'agent');
    expect(agents).toHaveLength(2);
    expect(agents.map((s) => s.column)).toEqual(['PROPOSES', 'AUTHORIZES']);
  });

  it('every step explains itself (English UI)', () => {
    for (const s of LOTE.steps) expect(s.explain.length).toBeGreaterThan(40);
  });

  it('every step carries a 64-hex tx hash', () => {
    for (const s of LOTE.steps) expect(s.tx).toMatch(/^[0-9a-f]{64}$/);
  });

  it('shortHash truncates long hashes', () => {
    expect(shortHash('a'.repeat(64), 10)).toBe(`${'a'.repeat(10)}…aaaa`);
  });
});
