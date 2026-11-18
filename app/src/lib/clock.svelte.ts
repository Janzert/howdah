// The clock times of day are written on, followed reactively: the
// setting, and the system's own clock, asked when first needed and again
// whenever the window is focused (it may have been changed meanwhile).
// Also today's date, so what's written as "today" changes at midnight.
import { api } from './api';
import type { SystemClock } from './bindings/SystemClock';
import { settings } from './settings.svelte';
import { clockOptions, hour24 } from './timeFormat';

class Clock {
  #system = $state<SystemClock | null>(null);
  #following = false;
  #day = $state(new Date().toDateString());
  #counting = false;

  /** Sets the day again a second past the coming midnight (so a timer
   * firing a little early still finds the new day), and so on. It's
   * started from a getter, which may be in a `$derived`, so it doesn't
   * set the day itself. */
  #untilMidnight() {
    const now = new Date();
    const next = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1);
    setTimeout(
      () => {
        this.#day = new Date().toDateString();
        this.#untilMidnight();
      },
      next.getTime() - now.getTime() + 1000,
    );
  }

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

  /** Now, changing (reactively) once a day: for telling today's times
   * from earlier ones. Asked again when the window is focused, since a
   * timer can run late across a suspend. */
  get today(): Date {
    if (!this.#counting) {
      this.#counting = true;
      this.#untilMidnight();
      globalThis.addEventListener?.('focus', () => (this.#day = new Date().toDateString()));
    }
    void this.#day;
    return new Date();
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
