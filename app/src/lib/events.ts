// Typed event subscription. Event names follow `domain://event`.
// Planned: 'engine://info', 'gameroom://update', 'tournament://progress'
// (high-rate per-request streams such as a single analysis will use Tauri
// Channels passed to the starting command instead).
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { EngineOutput } from './bindings/EngineOutput';
import type { SessionUpdate } from './bindings/SessionUpdate';

export interface EventMap {
  'game://changed': SessionUpdate;
  'engine://output': EngineOutput;
}

export function on<K extends keyof EventMap>(
  name: K,
  handler: (payload: EventMap[K]) => void,
): Promise<UnlistenFn> {
  return listen<EventMap[K]>(name, (e) => handler(e.payload));
}
