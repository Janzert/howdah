// Display preferences, per viewer. Kept in localStorage as one JSON object;
// a missing or unreadable store just means the defaults. Settings that
// matter to the backend (engines) live in the app config dir instead.
import { findTheme } from './theme';

export type Coordinates = 'none' | 'traps' | 'all';

export interface SettingsData {
  theme: string;
  /** Board labels: none, the four trap squares, or files and ranks along the edges. */
  coordinates: Coordinates;
  sound: boolean;
}

const KEY = 'settings';

export const DEFAULTS: SettingsData = { theme: findTheme(null).id, coordinates: 'traps', sound: true };

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
  let raw: Partial<Record<keyof SettingsData, unknown>> = {};
  try {
    raw = stored ? JSON.parse(stored) : {};
  } catch {
    /* defaults */
  }
  if (typeof raw.theme === 'string') s.theme = raw.theme;
  if (raw.coordinates === 'none' || raw.coordinates === 'traps' || raw.coordinates === 'all') {
    s.coordinates = raw.coordinates;
  }
  if (typeof raw.sound === 'boolean') s.sound = raw.sound;
  s.theme = findTheme(s.theme).id;
  return s;
}

class Settings {
  #data = $state<SettingsData>(parse(read(KEY), { theme: read('theme'), muted: read('muted') }));

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
