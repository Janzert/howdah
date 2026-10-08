// Display preferences, per viewer. Kept in localStorage as one JSON object;
// a missing or unreadable store just means the defaults. Settings that
// matter to the backend (engines) live in the app config dir instead.
import { STEP_MS } from './board/boardModel.svelte';
import { findTheme } from './theme';

export type Coordinates = 'none' | 'traps' | 'all';
/** Input from pointer movement without a button: none, arrows for a hovered
 * piece's legal steps, or step mode (the step toward the pointer, by click). */
export type HoverInput = 'off' | 'arrows' | 'step';
/** Times in the move list: none, each move's time, or the time from the
 * start of the game to each move. */
export type MoveTimes = 'off' | 'move' | 'game';
/** Light or dark colors, or whichever the system prefers. */
export type Appearance = 'system' | 'light' | 'dark';

/** The slowest step animation the setting allows, in ms. */
export const MAX_STEP_MS = 600;

export interface SettingsData {
  appearance: Appearance;
  theme: string;
  /** Board labels: none, the four trap squares, or files and ranks along the edges. */
  coordinates: Coordinates;
  sound: boolean;
  /** Sound volume, 0 to 100. */
  volume: number;
  /** How long a piece takes to slide one step, in ms; 0 shows moves at once. */
  stepMs: number;
  hoverInput: HoverInput;
  /** After a turn's fourth step, keep entering steps for the other side
   * (the turn is finished, as a plan when it's your move in a match). */
  continueTurns: boolean;
  /** Turn the board so a lone human player is at the bottom when a game starts. */
  humanAtBottom: boolean;
  /** The engine analysis last used (an engine id). */
  analysisEngine: string | null;
  moveTimes: MoveTimes;
}

const KEY = 'settings';

export const DEFAULTS: SettingsData = {
  appearance: 'system',
  theme: findTheme(null).id,
  coordinates: 'traps',
  sound: true,
  volume: 100,
  stepMs: STEP_MS,
  hoverInput: 'off',
  continueTurns: true,
  humanAtBottom: true,
  analysisEngine: null,
  moveTimes: 'off',
};

function read(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

/** Stored values over the defaults, ignoring anything malformed. Reads the
 * separate `theme` and `muted` keys older versions used. */
export function parse(stored: string | null, legacy: { theme: string | null; muted: string | null }): SettingsData {
  const s: SettingsData = { ...DEFAULTS };
  if (legacy.theme) s.theme = legacy.theme;
  if (legacy.muted != null) s.sound = legacy.muted !== '1';
  let raw: Partial<Record<keyof SettingsData | 'hoverArrows', unknown>> = {};
  try {
    raw = stored ? JSON.parse(stored) : {};
  } catch {
    /* defaults */
  }
  if (raw.appearance === 'system' || raw.appearance === 'light' || raw.appearance === 'dark') {
    s.appearance = raw.appearance;
  }
  if (typeof raw.theme === 'string') s.theme = raw.theme;
  if (raw.coordinates === 'none' || raw.coordinates === 'traps' || raw.coordinates === 'all') {
    s.coordinates = raw.coordinates;
  }
  if (typeof raw.sound === 'boolean') s.sound = raw.sound;
  if (typeof raw.volume === 'number' && Number.isFinite(raw.volume)) {
    s.volume = Math.round(Math.min(100, Math.max(0, raw.volume)));
  }
  if (typeof raw.stepMs === 'number' && Number.isFinite(raw.stepMs)) {
    s.stepMs = Math.round(Math.min(MAX_STEP_MS, Math.max(0, raw.stepMs)));
  }
  if (typeof raw.continueTurns === 'boolean') s.continueTurns = raw.continueTurns;
  if (typeof raw.humanAtBottom === 'boolean') s.humanAtBottom = raw.humanAtBottom;
  if (typeof raw.analysisEngine === 'string') s.analysisEngine = raw.analysisEngine;
  if (raw.moveTimes === 'off' || raw.moveTimes === 'move' || raw.moveTimes === 'game') s.moveTimes = raw.moveTimes;
  if (raw.hoverArrows === true) s.hoverInput = 'arrows'; // before step mode existed
  if (raw.hoverInput === 'off' || raw.hoverInput === 'arrows' || raw.hoverInput === 'step') {
    s.hoverInput = raw.hoverInput;
  }
  s.theme = findTheme(s.theme).id;
  return s;
}

class Settings {
  #data = $state<SettingsData>(parse(read(KEY), { theme: read('theme'), muted: read('muted') }));

  get appearance() {
    return this.#data.appearance;
  }
  set appearance(v: Appearance) {
    this.update({ appearance: v });
  }
  get theme() {
    return this.#data.theme;
  }
  set theme(v: string) {
    this.update({ theme: v });
  }
  get coordinates() {
    return this.#data.coordinates;
  }
  set coordinates(v: Coordinates) {
    this.update({ coordinates: v });
  }
  get sound() {
    return this.#data.sound;
  }
  set sound(v: boolean) {
    this.update({ sound: v });
  }
  get volume() {
    return this.#data.volume;
  }
  set volume(v: number) {
    this.update({ volume: v });
  }
  get stepMs() {
    return this.#data.stepMs;
  }
  set stepMs(v: number) {
    this.update({ stepMs: v });
  }

  get hoverInput() {
    return this.#data.hoverInput;
  }
  set hoverInput(v: HoverInput) {
    this.update({ hoverInput: v });
  }

  get continueTurns() {
    return this.#data.continueTurns;
  }
  set continueTurns(v: boolean) {
    this.update({ continueTurns: v });
  }

  get humanAtBottom() {
    return this.#data.humanAtBottom;
  }
  set humanAtBottom(v: boolean) {
    this.update({ humanAtBottom: v });
  }

  get analysisEngine() {
    return this.#data.analysisEngine;
  }
  set analysisEngine(v: string | null) {
    this.update({ analysisEngine: v });
  }

  get moveTimes() {
    return this.#data.moveTimes;
  }
  set moveTimes(v: MoveTimes) {
    this.update({ moveTimes: v });
  }

  update(patch: Partial<SettingsData>) {
    this.#data = { ...this.#data, ...patch };
    try {
      localStorage.setItem(KEY, JSON.stringify(this.#data));
    } catch {
      /* settings just won't persist */
    }
  }
}

export const settings = new Settings();
