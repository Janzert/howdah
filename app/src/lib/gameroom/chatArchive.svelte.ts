// The chat archive panel's state: the month on the calendar, the day open,
// and keeping both current as the local archive grows (archive requests,
// and the room's own log while the user is in it). One instance,
// `chatArchive`, kept across panel changes.
import { api, errorMessage } from '../api';
import type { ChatArchiveDay } from '../bindings/ChatArchiveDay';
import type { ChatArchiveMonth } from '../bindings/ChatArchiveMonth';
import { on } from '../events';

/** `YYYY-MM-DD` for a local date. */
export function isoDate(d: Date): string {
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** A local date from `YYYY-MM-DD`. */
export function parseDate(date: string): Date {
  const [y, m, d] = date.split('-').map(Number);
  return new Date(y, m - 1, d);
}

/** The day `days` after `date` (both `YYYY-MM-DD`). */
export function addDays(date: string, days: number): string {
  const d = parseDate(date);
  d.setDate(d.getDate() + days);
  return isoDate(d);
}

class ChatArchiveStore {
  /** The month on the calendar (month 1–12). */
  year = $state(new Date().getFullYear());
  month = $state(new Date().getMonth() + 1);
  days = $state<ChatArchiveMonth | null>(null);
  /** The day open, `YYYY-MM-DD`, and what the archive has of it. */
  selected = $state<string | null>(null);
  day = $state<ChatArchiveDay | null>(null);
  /** Asking arimaa.com for the open day. */
  fetching = $state(false);
  error = $state<string | null>(null);
  #started = false;

  /** Follows the local archive's changes; once. */
  start(): () => void {
    if (this.#started) return () => {};
    this.#started = true;
    const unlisten = on('chatarchive://changed', (c) => {
      const month = this.days;
      if (month && month.days.length > 0) {
        const first = parseDate(month.days[0].date).getTime() / 1000;
        const last = parseDate(addDays(month.days.at(-1)!.date, 1)).getTime() / 1000;
        if (c.start < last && c.end > first) void this.loadMonth();
      }
      if (this.day && !this.fetching && c.start < this.day.end && c.end > this.day.start) void this.reloadDay();
    });
    void this.loadMonth();
    return () => {
      this.#started = false;
      void unlisten.then((f) => f());
    };
  }

  async loadMonth() {
    try {
      this.days = await api.chatArchiveMonth(this.year, this.month);
    } catch (e) {
      this.error = errorMessage(e);
    }
  }

  /** Shows the month `delta` months from the shown one. */
  shiftMonth(delta: number) {
    const d = new Date(this.year, this.month - 1 + delta, 1);
    this.showMonth(d.getFullYear(), d.getMonth() + 1);
  }

  showMonth(year: number, month: number) {
    this.year = year;
    this.month = month;
    void this.loadMonth();
  }

  /** Opens `date`, asking arimaa.com for what the archive lacks of it. */
  async open(date: string) {
    const d = parseDate(date);
    if (d.getFullYear() !== this.year || d.getMonth() + 1 !== this.month) {
      this.showMonth(d.getFullYear(), d.getMonth() + 1);
    }
    this.selected = date;
    this.error = null;
    // What's here at once, then the rest.
    try {
      this.day = await api.chatArchiveDay(date, false);
    } catch (e) {
      this.error = errorMessage(e);
      return;
    }
    const s = this.day.summary;
    if (s.future || (s.coverage === 'full' && !s.today)) return;
    await this.fetch();
  }

  /** Asks arimaa.com for what the archive lacks of the open day. */
  async fetch() {
    const date = this.selected;
    if (!date) return;
    this.fetching = true;
    this.error = null;
    try {
      const day = await api.chatArchiveDay(date, true);
      if (this.selected === date) this.day = day;
      void this.loadMonth();
    } catch (e) {
      if (this.selected === date) this.error = errorMessage(e);
    } finally {
      this.fetching = false;
    }
  }

  async reloadDay() {
    const date = this.selected;
    if (!date) return;
    try {
      const day = await api.chatArchiveDay(date, false);
      if (this.selected === date) this.day = day;
    } catch {
      /* the next change tries again */
    }
  }
}

export const chatArchive = new ChatArchiveStore();
