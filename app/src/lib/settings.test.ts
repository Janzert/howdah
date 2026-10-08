import { describe, expect, it } from 'vitest';
import { DEFAULTS, parse } from './settings.svelte';

const none = { theme: null, muted: null };

describe('settings parse', () => {
  it('falls back to the defaults', () => {
    expect(parse(null, none)).toEqual(DEFAULTS);
    expect(parse('not json', none)).toEqual(DEFAULTS);
  });

  it('keeps valid values and drops invalid ones', () => {
    const s = parse(
      JSON.stringify({
        appearance: 'dark',
        coordinates: 'all',
        sound: false,
        theme: 'placeholder',
        hoverInput: 'step',
        humanAtBottom: false,
        analysisEngine: 'sharp-1',
        volume: 40,
        stepMs: 120,
        continueTurns: false,
        moveTimes: 'game',
      }),
      none,
    );
    expect(s).toEqual({
      appearance: 'dark',
      theme: 'placeholder',
      coordinates: 'all',
      sound: false,
      hoverInput: 'step',
      humanAtBottom: false,
      analysisEngine: 'sharp-1',
      volume: 40,
      stepMs: 120,
      continueTurns: false,
      moveTimes: 'game',
    });
    const bad = parse(
      JSON.stringify({ appearance: 'dim', coordinates: 'diagonal', sound: 'yes', theme: 'gone', analysisEngine: 3, volume: 'loud', moveTimes: 'all' }),
      none,
    );
    expect(bad).toEqual(DEFAULTS);
    const out = parse(JSON.stringify({ volume: 250, stepMs: -5 }), none);
    expect([out.volume, out.stepMs]).toEqual([100, 0]);
  });

  it('reads the keys older versions used', () => {
    expect(parse(JSON.stringify({ hoverArrows: true }), none).hoverInput).toBe('arrows');
    expect(parse(null, { theme: 'placeholder', muted: '1' })).toMatchObject({ theme: 'placeholder', sound: false });
    // The settings object wins over them.
    expect(parse(JSON.stringify({ sound: true }), { theme: null, muted: '1' }).sound).toBe(true);
  });
});
