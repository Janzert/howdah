// New games from arimaa.com's archive, added by themselves (the
// `autoUpdateGames` setting). The lobby follows it. The backend fetches
// the manifest at most once a day whatever we ask (`base.rs`), so asking
// every hour costs nothing and keeps a check from just missing the day;
// it installs only small deltas onto an archive already in. A failed
// check says nothing: the next one will do. The Games panel shows what
// was added from the `library://base` event.
import { api } from './api';

/** After the lobby opens, so a check doesn't slow the start. */
const FIRST_AFTER_MS = 60 * 1000;
const ASK_EVERY_MS = 60 * 60 * 1000;

/** Checks soon and then hourly while `on`; returns how to stop. */
export function followBaseUpdates(on: boolean) {
  if (!on) return;
  const check = () => void api.autoUpdateBase().catch(() => {});
  const first = setTimeout(check, FIRST_AFTER_MS);
  const timer = setInterval(check, ASK_EVERY_MS);
  return () => {
    clearTimeout(first);
    clearInterval(timer);
  };
}
