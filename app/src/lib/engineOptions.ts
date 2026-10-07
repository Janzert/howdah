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

/** "default 10", "1 to 32", ... for a field's placeholder and hint. */
export function rangeText(o: ManifestOptionView): string {
  if (o.min != null && o.max != null) return `${o.min} to ${o.max}`;
  if (o.min != null) return `at least ${o.min}`;
  if (o.max != null) return `at most ${o.max}`;
  return '';
}
