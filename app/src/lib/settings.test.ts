import { describe, expect, it } from 'vitest';
import { DEFAULTS, parse } from './settings.svelte';

const none = { theme: null, muted: null };

describe('settings parse', () => {
  it('falls back to the defaults', () => {
    expect(parse(null, none)).toEqual(DEFAULTS);
    expect(parse('not json', none)).toEqual(DEFAULTS);
  });

  it('keeps valid values and drops invalid ones', () => {
    const s = parse(JSON.stringify({ coordinates: 'all', sound: false, theme: 'placeholder' }), none);
    expect(s).toEqual({ theme: 'placeholder', coordinates: 'all', sound: false });
    const bad = parse(JSON.stringify({ coordinates: 'diagonal', sound: 'yes', theme: 'gone' }), none);
    expect(bad).toEqual(DEFAULTS);
  });

  it('reads the keys older versions used', () => {
    expect(parse(null, { theme: 'placeholder', muted: '1' })).toMatchObject({ theme: 'placeholder', sound: false });
    // The settings object wins over them.
    expect(parse(JSON.stringify({ sound: true }), { theme: null, muted: '1' }).sound).toBe(true);
  });
});
