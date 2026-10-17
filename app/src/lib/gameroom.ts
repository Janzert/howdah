// Small checks on the gameroom's lists and a followed game's state, shared
// by the lobby's alerts, the arimaa.com lobby and the game windows.
import type { AnnouncementView } from './bindings/AnnouncementView';
import type { Color } from './bindings/Color';
import type { LiveGameView } from './bindings/LiveGameView';
import type { WatchView } from './bindings/WatchView';
import type { SoundName } from './sound';

const same = (a: string | null, b: string | null | undefined) =>
  a != null && b != null && a.toLowerCase() === b.toLowerCase();

/** The user's side in one of their games. */
export function mySide(g: LiveGameView, user: string | null | undefined): Color | null {
  if (same(g.gold, user)) return 'gold';
  if (same(g.silver, user)) return 'silver';
  return null;
}

/** Whether one of the user's games waits on their move. The list gives no
 * turn before the first move, which is gold's setup. */
export function myTurn(g: LiveGameView, user: string | null | undefined): boolean {
  const side = mySide(g, user);
  return side != null && (g.turn ?? 'gold') === side && g.gold != null && g.silver != null;
}

/** The opponent's chat lines that `next` adds to `prev` (the same game,
 * at the user's seat). */
export function newOpponentChat(prev: WatchView | null, next: WatchView): WatchView['chat'] {
  if (next.side == null || prev?.gid !== next.gid) return [];
  return next.chat.slice(prev.chat.length).filter((l) => l.side != null && l.side !== next.side);
}

/** The sounds for a change at the user's seat from `prev` to `next` (the
 * same game): the opponent sitting down, leaving or coming back (also at
 * the table after the game, while its chat is open), their chat, and the
 * server refusing the user's move. */
export function watchSounds(prev: WatchView | null, next: WatchView): SoundName[] {
  const side = next.side;
  if (side == null || prev?.gid !== next.gid) return [];
  const sounds: SoundName[] = [];
  const opponent = side === 'gold' ? 1 : 0;
  const afterGame = next.state === 'ended' && prev.state === 'ended' && next.chatOpen;
  if (next.state === 'following' || afterGame) {
    if (prev.waiting && !next.waiting) sounds.push('join');
    else if (!next.waiting && prev.away[opponent] !== next.away[opponent]) {
      sounds.push(next.away[opponent] ? 'leave' : 'join');
    }
  }
  if (next.refused && next.refused !== prev.refused) sounds.push('illegal');
  if (newOpponentChat(prev, next).length > 0) sounds.push('chat');
  return sounds;
}

/** How recently an announcement must have been edited to be shown
 * unasked. The gameroom keeps announcements for years (seen in 2026: one
 * from 2018), and new ones are rare. */
export const FRESH_ANNOUNCEMENT_S = 90 * 24 * 3600;

/** Identifies an announcement for hiding it: an edit makes it new again. */
export function announcementKey(a: AnnouncementView): string {
  return `${a.modified ?? ''}:${a.blocks.map((b) => b.spans.map((s) => s.text).join('')).join('\n')}`;
}

/** The announcements to show unasked (edited recently, by the server's
 * clock, and not hidden), and the rest. */
export function splitAnnouncements(
  all: AnnouncementView[],
  hidden: ReadonlySet<string>,
): { fresh: AnnouncementView[]; older: AnnouncementView[] } {
  const isFresh = (a: AnnouncementView) =>
    a.age != null && a.age <= FRESH_ANNOUNCEMENT_S && !hidden.has(announcementKey(a));
  return { fresh: all.filter(isFresh), older: all.filter((a) => !isFresh(a)) };
}
