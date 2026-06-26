import { describe, it, expect } from 'vitest';
import { WEB_VERSION } from '../src/index.js';

describe('web smoke', () => {
  it('exports the scaffold version', () => {
    expect(WEB_VERSION).toBe('0.1.0');
  });
});
