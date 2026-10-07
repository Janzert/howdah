import type { EngineOption } from './bindings/EngineOption';
import type { ManifestOptionView } from './bindings/ManifestOptionView';

/** The options a manifest describes that the settings form shows: all but
 * buttons, which are actions rather than settings. */
export function settingOptions(options: ManifestOptionView[]): ManifestOptionView[] {
  return options.filter((o) => o.kind !== 'button');
}

/** Splits an engine's saved options into values for the manifest's fields
 * (by name) and the rest, which the form keeps as text. */
export function splitOptions(
  saved: EngineOption[],
  described: ManifestOptionView[],
): { values: Record<string, string>; other: EngineOption[] } {
  const names = new Set(described.map((o) => o.name));
  const values: Record<string, string> = {};
  const other: EngineOption[] = [];
  for (const o of saved) {
    if (names.has(o.name)) values[o.name] = o.value;
    else other.push(o);
  }
  return { values, other };
}

/** The options to save from the fields: those set to something other than
 * the engine's default, so a known engine keeps the options the app sets
 * for it (Sharp's `ignoretc` for analysis) unless the user changes them. */
export function fieldOptions(described: ManifestOptionView[], values: Record<string, string>): EngineOption[] {
  return described.flatMap((o) => {
    const value = (values[o.name] ?? '').trim();
    return value === '' || sameValue(o, value, o.default) ? [] : [{ name: o.name, value }];
  });
}

/** The options for one game from the fields: each that differs from what
 * the engine would otherwise get (its `saved` value, else the default),
 * and each the game set before (`previous`) with the value it goes back
 * to, so a running engine changes back too. A blank field with neither a
 * saved value nor a default is left out. */
export function gameOptions(
  described: ManifestOptionView[],
  values: Record<string, string>,
  saved: EngineOption[],
  previous: EngineOption[],
): EngineOption[] {
  return described.flatMap((o) => {
    const base = saved.find((s) => s.name === o.name)?.value ?? o.default;
    const value = (values[o.name] ?? '').trim() || base;
    if (value == null || value === '') return [];
    const changed = !sameValue(o, value, base);
    return changed || previous.some((p) => p.name === o.name) ? [{ name: o.name, value }] : [];
  });
}

/** `name = value` lines; blank lines are skipped, and a line without `=`
 * is a name with an empty value. */
export function parseOptions(text: string): EngineOption[] {
  return text
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line.length > 0)
    .map((line) => {
      const i = line.indexOf('=');
      return i < 0 ? { name: line, value: '' } : { name: line.slice(0, i).trim(), value: line.slice(i + 1).trim() };
    });
}

export function formatOptions(options: EngineOption[]): string {
  return options.map((o) => `${o.name} = ${o.value}`).join('\n');
}

function sameValue(o: ManifestOptionView, value: string, other: string | null): boolean {
  if (other == null) return false;
  if (o.kind === 'spin' || o.kind === 'float') return Number(value) === Number(other);
  return value === other;
}

/** Why `value` doesn't suit option `o`, or null if it does (or is empty,
 * which leaves the engine's default). */
export function optionError(o: ManifestOptionView, value: string): string | null {
  const v = value.trim();
  if (v === '') return null;
  switch (o.kind) {
    case 'check':
      return v === 'true' || v === 'false' ? null : 'must be true or false';
    case 'spin':
      if (!/^[-+]?\d+$/.test(v)) return 'must be a whole number';
      return boundsError(o, Number(v));
    case 'float': {
      const n = Number(v);
      if (!Number.isFinite(n)) return 'must be a number';
      return boundsError(o, n);
    }
    case 'combo':
      return o.choices.includes(v) ? null : `must be one of ${o.choices.join(', ')}`;
    default:
      return /[\r\n]/.test(v) ? 'must be on one line' : null;
  }
}

function boundsError(o: ManifestOptionView, n: number): string | null {
  if (o.min != null && n < o.min) return `must be at least ${o.min}`;
  if (o.max != null && n > o.max) return `must be at most ${o.max}`;
  return null;
}

/** The range and default for an option's hint: "1 to 32, default 1",
 * "default off", ... */
export function detailText(o: ManifestOptionView): string {
  const parts: string[] = [];
  if (o.min != null && o.max != null) parts.push(`${o.min} to ${o.max}`);
  else if (o.min != null) parts.push(`at least ${o.min}`);
  else if (o.max != null) parts.push(`at most ${o.max}`);
  if (o.default != null && o.default !== '') {
    const shown = o.kind === 'check' ? (o.default === 'true' ? 'on' : 'off') : o.default;
    parts.push(`default ${shown}`);
  }
  return parts.join(', ');
}
