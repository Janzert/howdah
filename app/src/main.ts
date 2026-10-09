import { mount } from 'svelte';
import App from './App.svelte';
import './app.css';
import { api, isGameWindow, session, useSession } from './lib/api';
import { setAppearance } from './lib/appearance';
import { settings } from './lib/settings.svelte';
import Lobby from './Lobby.svelte';
import { closeSessionWithPage, inTauri, nameTab } from './lib/windows';

// Before the first paint, so a dark window doesn't flash light.
setAppearance(settings.appearance);

// In a plain browser (the Vite preview), talk to the dev bridge instead of
// Tauri. Must run before anything calls invoke or listen.
if (import.meta.env.DEV && !inTauri) {
  const { installDevBridge } = await import('./lib/devBridge');
  installDevBridge();
  // A game window's tab closes its session as it goes, so a reload (whose
  // close may still be on its way) or a restarted bridge carries on with a
  // new one.
  const reloaded = (performance.getEntriesByType('navigation')[0] as PerformanceNavigationTiming | undefined)?.type === 'reload';
  if (session != null && (reloaded || !(await api.listSessions()).includes(session))) {
    useSession(await api.openSession());
  }
  nameTab();
  await closeSessionWithPage();
}

// One entry page, two roots (docs/WINDOWS.md): a game window
// (`?session=<id>`) shows its game, the main window the lobby.
export default mount(isGameWindow ? App : Lobby, { target: document.getElementById('app')! });
