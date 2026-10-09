// Typed event subscription. Event names follow `domain://event`.
// Planned: 'tournament://progress'.
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { session } from './api';
import type { AnalysisView } from './bindings/AnalysisView';
import type { EngineOutput } from './bindings/EngineOutput';
import type { GameroomGames } from './bindings/GameroomGames';
import type { InvitationAnswer } from './bindings/InvitationAnswer';
import type { SessionUpdate } from './bindings/SessionUpdate';
import type { SessionsChanged } from './bindings/SessionsChanged';
import type { WatchView } from './bindings/WatchView';

export interface EventMap {
  'game://changed': SessionUpdate;
  'engine://output': EngineOutput;
  /** Analysis snapshots, at most every 100 ms while the engine reports. */
  'analysis://update': AnalysisView;
  /** The followed arimaa.com game's state, when it changes. */
  'gameroom://watch': WatchView;
  /** The lobby's lists, every minute while logged in to arimaa.com. */
  'gameroom://lobby': GameroomGames;
  /** How an invitation the user sent was answered. */
  'gameroom://invitation': InvitationAnswer;
  /** The sessions open now (one per game window), when one opens or closes. */
  'sessions://changed': SessionsChanged;
}

/** Subscribes to an event. Events from a session carry a `session` field
 * (added by the backend); only this window's session's reach `handler`. */
export function on<K extends keyof EventMap>(
  name: K,
  handler: (payload: EventMap[K]) => void,
): Promise<UnlistenFn> {
  return listen<EventMap[K] & { session?: number }>(name, (e) => {
    if (e.payload.session === undefined || e.payload.session === session) handler(e.payload);
  });
}

/** Subscribes to an event from every session, with the session it came
 * from (null for app-wide events): for the lobby, which shows them all. */
export function onEvery<K extends keyof EventMap>(
  name: K,
  handler: (payload: EventMap[K], session: number | null) => void,
): Promise<UnlistenFn> {
  return listen<EventMap[K] & { session?: number }>(name, (e) => handler(e.payload, e.payload.session ?? null));
}
