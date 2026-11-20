import { describe, expect, it } from 'vitest';
import { plainCopy } from './plainCopy';

// A stand-in for Svelte's `$state`, which is a Proxy over the object.
const proxied = <T extends object>(v: T): T => new Proxy(v, {});

describe('plainCopy', () => {
  it('makes proxied arguments cloneable for postMessage', () => {
    const args = {
      filter: { player: 'x', position: proxied({ short: 'g Ra1', sideToMove: 'gold' }) },
      exclude: proxied([1, 2]),
      offset: 0,
    };
    expect(() => structuredClone(args)).toThrow();
    const copy = plainCopy(args);
    expect(structuredClone(copy)).toEqual({
      filter: { player: 'x', position: { short: 'g Ra1', sideToMove: 'gold' } },
      exclude: [1, 2],
      offset: 0,
    });
  });

  it('keeps primitives, null and non-plain objects', () => {
    const bytes = new Uint8Array([1, 2]);
    expect(plainCopy(null)).toBeNull();
    expect(plainCopy(3)).toBe(3);
    expect(plainCopy('s')).toBe('s');
    expect(plainCopy({ bytes }).bytes).toBe(bytes);
    expect(plainCopy({ a: undefined })).toEqual({ a: undefined });
  });
});
