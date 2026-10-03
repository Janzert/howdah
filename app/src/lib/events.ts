// Typed event subscription. Event names follow `domain://event`.
// Planned: 'gameroom://update', 'tournament://progress'.
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { AnalysisView } from './bindings/AnalysisView';
import type { EngineOutput } from './bindings/EngineOutput';
import type { SessionUpdate } from './bindings/SessionUpdate';

export interface EventMap {
  'game://changed': SessionUpdate;
  'engine://output': EngineOutput;
  /** Analysis snapshots, at most every 100 ms while the engine reports. */
  'analysis://update': AnalysisView;
}

export function on<K extends keyof EventMap>(
  name: K,
  handler: (payload: EventMap[K]) => void,
): Promise<UnlistenFn> {
  return listen<EventMap[K]>(name, (e) => handler(e.payload));
}
