// Typed event subscription. Event names follow `domain://event`.
// Planned: 'gameroom://update', 'tournament://progress'.
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { session } from './api';
import type { AnalysisView } from './bindings/AnalysisView';
import type { EngineOutput } from './bindings/EngineOutput';
import type { SessionUpdate } from './bindings/SessionUpdate';

export interface EventMap {
  'game://changed': SessionUpdate;
  'engine://output': EngineOutput;
  /** Analysis snapshots, at most every 100 ms while the engine reports. */
  'analysis://update': AnalysisView;
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
