// The line above the move list: what a game's record says about it.
import type { GameInfoView } from './bindings/GameInfoView';

/** `2014.01.04` as "Jan 4, 2014"; a date with unknown parts as written. */
export function recordDate(date: string): string {
  const m = /^(\d{4})\.(\d{2})\.(\d{2})$/.exec(date.trim());
  if (!m) return date;
  const d = new Date(Date.UTC(+m[1], +m[2] - 1, +m[3]));
  return d.toLocaleDateString(undefined, { dateStyle: 'medium', timeZone: 'UTC' });
}

/** "Gold 2m/2m · Silver 30s/5m", or one time control when both share it. */
export function timeControlsText([gold, silver]: GameInfoView['timeControls']): string | null {
  if (gold === silver) return gold;
  return [gold && `Gold ${gold}`, silver && `Silver ${silver}`].filter((s) => s).join(' · ');
}

/** "Casual game · 2m/2m/100/0/30m · rated · Jan 4, 2014 · #671438", or null with nothing to say. */
export function gameInfoText(info: GameInfoView): string | null {
  const parts = [
    info.event,
    timeControlsText(info.timeControls),
    info.rated == null ? null : info.rated ? 'rated' : 'unrated',
    info.date && recordDate(info.date),
    info.gameId && `#${info.gameId}`,
  ].filter((s) => s);
  return parts.length ? parts.join(' · ') : null;
}
