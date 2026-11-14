// The clock times of day are written on, followed reactively: the
// setting, and the system's own clock, asked when first needed and again
// whenever the window is focused (it may have been changed meanwhile).
import { api } from './api';
import type { SystemClock } from './bindings/SystemClock';
import { settings } from './settings.svelte';
import { clockOptions, hour24 } from './timeFormat';

class Clock {
  #system = $state<SystemClock | null>(null);
  #following = false;

  #ask() {
    api
      .systemClock()
      .then((c) => (this.#system = c))
      .catch(() => {});
  }

  /** What the system says, null until it has answered. */
  get system() {
    if (!this.#following) {
      this.#following = true;
      this.#ask();
      globalThis.addEventListener?.('focus', () => this.#ask());
    }
    return this.#system;
  }

  /** Whether "System" means the 24-hour clock here. */
  get systemHour24() {
    return hour24('system', this.system);
  }

  /** Options for `toLocaleTimeString` and the like. */
  get options(): Intl.DateTimeFormatOptions {
    return clockOptions(hour24(settings.timeFormat, this.system));
  }
}

export const clock = new Clock();
