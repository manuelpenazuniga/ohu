import { describe, it, expect } from 'vitest';
import { AGENTS_VERSION } from '../src/index.js';

describe('agents smoke', () => {
  it('exports the scaffold version', () => {
    expect(AGENTS_VERSION).toBe('0.1.0');
  });
});
