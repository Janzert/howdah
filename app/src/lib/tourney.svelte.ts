// The engine test daemon's state for the lobby window: whether it runs,
// and its tests, kept up to date from `tourney://changed`. Shared by the
// Engine tests panel, each test's panel and the sidebar's summaries.
import { api, errorMessage } from './api';
import type { TestView } from './bindings/TestView';
import type { TourneyStatusView } from './bindings/TourneyStatusView';
import { on } from './events';

class TourneyStore {
  status = $state<TourneyStatusView | null>(null);
  tests = $state<TestView[]>([]);
  error = $state<string | null>(null);
  /** What the last archive, delete or restore did. */
  notice = $state<string | null>(null);

  /** Games playing now, across the tests. */
  playing = $derived(this.tests.reduce((n, t) => n + t.running.length, 0));

  test(id: string): TestView | undefined {
    return this.tests.find((t) => t.id === id);
  }

  async refresh() {
    try {
      this.status = await api.tourneyStatus();
      this.tests = this.status.connected ? await api.tourneyTests() : [];
      this.error = null;
    } catch (e) {
      this.error = errorMessage(e);
    }
  }

  /** Follows the daemon; returns the function that stops it. */
  start(): () => void {
    void this.refresh();
    const unlisten = on('tourney://changed', (u) => {
      if (this.status && u.connected !== this.status.connected) {
        void this.refresh();
        return;
      }
      if (u.test) {
        const t = u.test;
        const i = this.tests.findIndex((x) => x.id === t.id);
        if (i >= 0) this.tests[i] = t;
        else this.tests = [...this.tests, t];
      }
      if (u.slots != null && this.status) this.status.slots = u.slots;
      if (u.removed) this.forget(u.removed);
    });
    return () => void unlisten.then((f) => f());
  }

  /** Takes a test off the list (archived or deleted). */
  forget(id: string) {
    this.tests = this.tests.filter((t) => t.id !== id);
  }

  /** Adds a test (restored or new) if it isn't listed. */
  add(t: TestView) {
    if (!this.tests.some((x) => x.id === t.id))
      this.tests = [...this.tests, t].sort((a, b) => a.id.localeCompare(b.id));
  }
}

export const tourney = new TourneyStore();
