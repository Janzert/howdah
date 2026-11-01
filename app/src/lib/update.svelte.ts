// A newer release of Howdah, if there is one. It's only told about, on
// the lobby's Home and in About, with a link to the release's page.
import { api } from './api';
import type { UpdateView } from './bindings/UpdateView';

const DISMISSED = 'update.dismissed';
const CHECK_EVERY_MS = 24 * 60 * 60 * 1000;

function readDismissed(): string | null {
  try {
    return localStorage.getItem(DISMISSED);
  } catch {
    return null;
  }
}

class Update {
  available = $state<UpdateView | null>(null);
  #dismissed = $state(readDismissed());

  /** What Home shows: dismissing hides one version, not the next. */
  get notice() {
    return this.available?.version === this.#dismissed ? null : this.available;
  }

  /** Checks now and daily while `on`; returns how to stop. A failed check
   * says nothing: the next one will do. */
  follow(on: boolean) {
    if (!on) {
      this.available = null;
      return;
    }
    const check = () =>
      api
        .checkForUpdate()
        .then((u) => (this.available = u))
        .catch(() => {});
    check();
    const timer = setInterval(check, CHECK_EVERY_MS);
    return () => clearInterval(timer);
  }

  open() {
    if (this.available) api.openUrl(this.available.url).catch(() => {});
  }

  dismiss() {
    this.#dismissed = this.available?.version ?? null;
    try {
      if (this.#dismissed) localStorage.setItem(DISMISSED, this.#dismissed);
    } catch {
      /* it'll show again next time */
    }
  }
}

export const update = new Update();
