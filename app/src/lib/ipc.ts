// The app's `invoke` and `listen`: Tauri's, or a docked board's relay
// through the lobby (lib/frames.ts). The relay can't take over Tauri's
// own functions with `mockIPC`: on Windows WebView2 runs Tauri's scripts
// in iframes too, and they define `__TAURI_INTERNALS__` read-only, so
// `mockIPC` throws there.
import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { type EventCallback, listen as tauriListen, type UnlistenFn } from '@tauri-apps/api/event';

export type { UnlistenFn };

/** What stands in for Tauri's IPC. */
export interface Ipc {
  invoke: (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;
  listen: (event: string, handler: EventCallback<unknown>) => Promise<UnlistenFn>;
}

let relay: Ipc | null = null;

/** Sends every later `invoke` and `listen` to `ipc`. */
export function useIpc(ipc: Ipc) {
  relay = ipc;
}

export function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return relay ? (relay.invoke(cmd, args) as Promise<T>) : tauriInvoke<T>(cmd, args);
}

export function listen<T>(event: string, handler: EventCallback<T>): Promise<UnlistenFn> {
  return relay ? relay.listen(event, handler as EventCallback<unknown>) : tauriListen<T>(event, handler);
}
