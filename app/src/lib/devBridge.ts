// Dev only, outside Tauri: route `invoke` and events to the dev bridge
// (src-tauri/src/bin/dev-bridge.rs), which runs the real backend natively.
// Vite proxies /bridge to it. Start it with `npm run bridge`.
import { emit } from '@tauri-apps/api/event';
import { mockIPC } from '@tauri-apps/api/mocks';
import { isApiError, session } from './api';
import type { ApiError } from './bindings/ApiError';

const BASE = '/bridge';

function bridgeError(detail: string): ApiError {
  return {
    kind: 'state',
    message: `Dev bridge request failed (${detail}). If the bridge isn't running, start it with \`npm run bridge\`.`,
    line: null,
  };
}

async function invoke(cmd: string, args: unknown): Promise<unknown> {
  let res: Response;
  try {
    res = await fetch(`${BASE}/invoke/${cmd}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(args ?? {}),
    });
  } catch (e) {
    throw bridgeError(String(e));
  }
  const text = await res.text();
  if (res.ok) return text ? JSON.parse(text) : null;
  // Command errors come back as an ApiError; anything else (the Vite proxy's
  // 502 when the bridge is down, bad JSON) becomes one.
  let err: unknown = null;
  try {
    err = JSON.parse(text);
  } catch {
    // Not JSON.
  }
  if (isApiError(err)) throw err;
  throw bridgeError(`HTTP ${res.status}${text ? `: ${text}` : ''}`);
}

export function installDevBridge(): void {
  mockIPC((cmd, args) => invoke(cmd, args), { shouldMockEvents: true });
  const source = new EventSource(`${BASE}/events`);
  for (const name of ['game://changed', 'engine://output', 'analysis://update', 'gameroom://watch', 'gameroom://lobby', 'gameroom://invitation']) {
    source.addEventListener(name, (e) => {
      void emit(name, JSON.parse((e as MessageEvent<string>).data));
    });
  }
  // EventSource reconnects by itself; after a bridge restart the session is
  // new, so ask for the state again.
  source.addEventListener('open', () => {
    void invoke('get_state', { session }).then((view) =>
      emit('game://changed', { session, view, animation: [], animationBudgetMs: null }),
    );
  });
  source.addEventListener('error', () => console.warn('dev bridge: event stream lost, retrying'));
  console.info('dev bridge: frontend is using the native backend at', BASE);
}

/** Closes a session as the page goes away. A beacon, since a request
 * started from `pagehide` may be cut off with the page. */
export function closeSessionOnUnload(id: number): void {
  navigator.sendBeacon(`${BASE}/invoke/close_session`, JSON.stringify({ session: id }));
}
