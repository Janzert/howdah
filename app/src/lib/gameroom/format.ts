// Words for the arimaa.com lobby's lists.
import type { AnnouncementView } from '../bindings/AnnouncementView';
import type { GameResult } from '../bindings/GameResult';
import type { InvitationView } from '../bindings/InvitationView';
import type { LiveGameView } from '../bindings/LiveGameView';
import type { PastGameView } from '../bindings/PastGameView';
import type { RecentGameView } from '../bindings/RecentGameView';
import type { ServerBotView } from '../bindings/ServerBotView';
import type { WinReason } from '../bindings/WinReason';

const join = (parts: (string | null | undefined | false)[]) => parts.filter((s) => s).join(' · ');

const reasons: Record<WinReason, string> = {
  goal: 'goal',
  elimination: 'elimination',
  immobilization: 'immobilization',
  timeout: 'time',
  resignation: 'resignation',
  illegalMove: 'illegal move',
  score: 'score',
  forfeit: 'forfeit',
};

export function resultText(result: GameResult | null): string | null {
  return result ? `${result.winner === 'gold' ? 'Gold' : 'Silver'} won by ${reasons[result.reason]}` : null;
}

/** "2m/5m/100/0/30m · rated · postal" */
export function gameMeta(g: LiveGameView): string {
  return join([g.timeControl, g.rated && 'rated', g.postal && 'postal']);
}

/** "as Gold · 2m/5m/100/0/30m · unrated" */
export function invitationMeta(i: InvitationView): string {
  return join([`as ${i.side === 'gold' ? 'Gold' : 'Silver'}`, i.timeControl, i.rated ? 'rated' : 'unrated']);
}

/** "Gold won by goal · 33 moves · Jan 4, 2014 10:38 am · 2m/2m · rated" */
export function pastMeta(g: PastGameView): string {
  return join([
    resultText(g.result),
    g.moves != null && `${g.moves} moves`,
    g.finished,
    g.timeControl,
    g.rated && 'rated',
  ]);
}

/** "Gold won by goal · 33 moves · Oct 4, 10:23 · rated" */
export function recentMeta(g: RecentGameView): string {
  const ended =
    g.endedMs != null &&
    new Date(g.endedMs).toLocaleString(undefined, {
      month: 'short',
      day: 'numeric',
      hour: 'numeric',
      minute: '2-digit',
    });
  return join([resultText(g.result), g.moves != null && `${g.moves} moves`, ended, g.rated && 'rated']);
}

/** "You: 6 games, won 1, lost 5 · not beaten yet" */
export function botRecord(b: ServerBotView): string {
  return join([
    b.games ? `You: ${b.games} ${b.games === 1 ? 'game' : 'games'}, won ${b.won ?? 0}, lost ${b.lost ?? 0}` : null,
    b.toBeWon === true && 'not beaten yet',
  ]);
}

export function announcementDate(a: AnnouncementView): string {
  return a.modified == null
    ? 'Undated'
    : new Date(a.modified * 1000).toLocaleDateString(undefined, { dateStyle: 'medium' });
}
