import { describe, expect, it } from 'vitest';
import { clockOptions, hour24, localeHour24 } from './timeFormat';

describe('time format', () => {
  it('takes the setting over the system', () => {
    expect(hour24('24', { hour24: false, locale: 'en-US' })).toBe(true);
    expect(hour24('12', { hour24: true, locale: 'de-DE' })).toBe(false);
  });

  it("takes the system's clock over its locale's", () => {
    expect(hour24('system', { hour24: true, locale: 'en-US' })).toBe(true);
    expect(hour24('system', { hour24: false, locale: 'de-DE' })).toBe(false);
  });

  it("falls back to the locale's clock", () => {
    expect(hour24('system', { hour24: null, locale: 'de-DE' })).toBe(true);
    expect(hour24('system', { hour24: null, locale: 'en-US' })).toBe(false);
    expect(localeHour24('not a locale')).toBe(localeHour24());
    expect(hour24('system', null)).toBe(localeHour24());
  });

  it('writes times on the clock chosen', () => {
    const d = new Date(2026, 9, 18, 21, 5);
    const show = (is24: boolean) =>
      d.toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit', ...clockOptions(is24) });
    expect(show(true)).toBe('21:05');
    expect(show(false)).toMatch(/^9:05\sPM$/);
    const midnight = new Date(2026, 9, 18, 0, 5);
    expect(midnight.toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit', ...clockOptions(true) })).toBe(
      '00:05',
    );
  });
});
