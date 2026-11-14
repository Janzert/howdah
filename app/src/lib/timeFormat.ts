// Times of day on a 12- or a 24-hour clock: the `timeFormat` setting, or
// else what the system says (`system_clock`), or else the webview's own
// default for its language. The reactive side is `clock.svelte.ts`.
import type { SystemClock } from './bindings/SystemClock';
import type { TimeFormat } from './settings.svelte';

/** Whether a locale's own clock is the 24-hour one; the webview's
 * language with none. */
export function localeHour24(locale?: string): boolean {
  let cycle: string | undefined;
  try {
    cycle = new Intl.DateTimeFormat(locale, { hour: 'numeric' }).resolvedOptions().hourCycle;
  } catch {
    // Not a language tag the webview knows: its own language, then.
    cycle = new Intl.DateTimeFormat(undefined, { hour: 'numeric' }).resolvedOptions().hourCycle;
  }
  return cycle === 'h23' || cycle === 'h24';
}

/** The clock to write times on. The system's answer, where it has one,
 * goes over its locale's, and that over the webview's. */
export function hour24(setting: TimeFormat, system: SystemClock | null): boolean {
  if (setting !== 'system') return setting === '24';
  return system?.hour24 ?? localeHour24(system?.locale ?? undefined);
}

/** The option that makes `toLocaleTimeString` and the like use that
 * clock (`h23` rather than `hour12: false`, which can write midnight
 * as 24:00). */
export function clockOptions(is24: boolean): Intl.DateTimeFormatOptions {
  return { hourCycle: is24 ? 'h23' : 'h12' };
}
