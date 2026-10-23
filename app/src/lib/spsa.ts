// SPSA tuning runs (a test with `[spsa]`): how the lobby words and
// measures a run's progress and parameters.
import type { SpsaParamView } from './bindings/SpsaParamView';
import type { SpsaStepView } from './bindings/SpsaStepView';
import type { TestView } from './bindings/TestView';

/** How far a run is through its iterations, from 0 to 1; null for a test
 * that isn't one. */
export function spsaProgress(t: TestView): number | null {
  const s = t.spsa;
  if (s == null || s.iterations <= 0) return null;
  return Math.min(1, s.done / s.iterations);
}

/** Iterations in brief: `358/2000`. */
export function iterationsText(t: TestView): string | null {
  return t.spsa ? `${t.spsa.done}/${t.spsa.iterations}` : null;
}

/** How far a parameter has moved from its start, as a share of its range
 * (signed). */
export function moved(p: SpsaParamView): number {
  return (p.value - p.start) / (p.max - p.min);
}

/** Whether a parameter has moved further than this iteration's
 * perturbation, the size of the noise in a single step. */
export function movedPastNoise(p: SpsaParamView): boolean {
  return Math.abs(p.value - p.start) > p.c;
}

/** A parameter's value as it's shown: an integer's to two decimals (θ
 * stays real; the engine gets it rounded), others to four significant
 * figures. */
export function valueText(v: number, int = false): string {
  return signText(plainValue(v, int));
}

function plainValue(v: number, int: boolean): string {
  if (!Number.isFinite(v)) return '–';
  if (int) return v.toFixed(2);
  if (v === 0) return '0';
  const digits = Math.max(0, 3 - Math.floor(Math.log10(Math.abs(v))));
  return v.toFixed(Math.min(digits, 6));
}

/** A typographic minus in place of a leading hyphen. */
function signText(s: string): string {
  return s.startsWith('-') ? `−${s.slice(1)}` : s;
}

/** A change as a signed share of the range: `+12.3%`, `−0.4%`. */
export function shareText(x: number): string {
  const p = x * 100;
  const s = Math.abs(p) < 0.05 ? '0.0' : Math.abs(p).toFixed(1);
  return `${p < 0 && s !== '0.0' ? '−' : p > 0 && s !== '0.0' ? '+' : ''}${s}%`;
}

/** θ as option lines for an engine (`name = value`), integers rounded as
 * the engine would get them. */
export function optionsText(params: SpsaParamView[]): string {
  return params.map((p) => `${p.name} = ${p.int ? Math.round(p.value) : +p.value.toPrecision(6)}`).join('\n');
}

/** The mean distance of θ from its start, as a share of each range, after
 * each step. */
export function meanMovement(params: SpsaParamView[], steps: SpsaStepView[]): number[] {
  return steps.map((s) => {
    let sum = 0;
    params.forEach((p, i) => (sum += Math.abs(s.theta[i] - p.start) / (p.max - p.min)));
    return params.length ? sum / params.length : 0;
  });
}

/** At most `n` points from `xs`, evenly spaced, always keeping the last;
 * each as [index, value]. */
export function thin(xs: number[], n: number): [number, number][] {
  if (xs.length <= n) return xs.map((x, i) => [i, x]);
  const out: [number, number][] = [];
  for (let k = 0; k < n; k++) {
    const i = Math.round((k * (xs.length - 1)) / (n - 1));
    out.push([i, xs[i]]);
  }
  return out;
}

/** Steps looked back over for the pace. */
const PACE_STEPS = 20;

/** Iterations an hour, from the newest steps' finish times (seconds, in
 * the order taken), since the last long gap (a pause, or the daemon not
 * running): a gap over ten times the median and over a minute. Null with
 * fewer than four such steps. */
export function iterationsPerHour(steps: SpsaStepView[]): number | null {
  const times = steps
    .slice(-PACE_STEPS)
    .map((s) => s.finished)
    .reverse();
  const gaps = times.slice(1).map((t, i) => times[i] - t);
  const median = [...gaps].sort((a, b) => a - b)[Math.floor(gaps.length / 2)] ?? 0;
  const pause = gaps.findIndex((g) => g > 60 && g > 10 * median);
  const n = pause < 0 ? times.length : pause + 1;
  const span = times[0] - times[n - 1];
  return n < 4 || span <= 0 ? null : ((n - 1) / span) * 3600;
}

/** A stretch of time in brief: `40 min`, `6 h`, `3 days`. */
export function durationText(hours: number): string {
  const minutes = Math.round(hours * 60);
  if (minutes < 60) return `${Math.max(1, minutes)} min`;
  if (hours < 48) return `${+hours.toFixed(hours < 10 ? 1 : 0)} h`;
  return `${+(hours / 24).toFixed(1)} days`;
}

/** How the parameter list can be ordered. */
export type ParamOrder = 'spec' | 'moved' | 'name';

/** The parameters' indexes, matching `filter` (a substring of the name),
 * in `order`: the spec's, furthest moved first, or by name. */
export function orderParams(params: SpsaParamView[], order: ParamOrder, filter: string): number[] {
  const f = filter.trim().toLowerCase();
  const idx = params.map((_, i) => i).filter((i) => !f || params[i].name.toLowerCase().includes(f));
  if (order === 'moved') idx.sort((a, b) => Math.abs(moved(params[b])) - Math.abs(moved(params[a])));
  else if (order === 'name') idx.sort((a, b) => params[a].name.localeCompare(params[b].name));
  return idx;
}
