import { mount } from 'svelte';
import App from './App.svelte';
import './app.css';
import { api, isGameWindow, session, useSession } from './lib/api';
import { setAppearance } from './lib/appearance';
import { settings } from './lib/settings.svelte';
import { closeSessionWithPage, inTauri } from './lib/windows';

// Before the first paint, so a dark window doesn't flash light.
setAppearance(settings.appearance);

// In a plain browser (the Vite preview), talk to the dev bridge instead of
// Tauri. Must run before anything calls invoke or listen.
if (import.meta.env.DEV && !inTauri) {
  const { installDevBridge } = await import('./lib/devBridge');
  installDevBridge();
  // A game window's tab closes its session as it goes, so a reload finds
  // it gone (or the bridge restarted): carry on with a new one.
  if (isGameWindow && !(await api.listSessions()).includes(session)) useSession(await api.openSession());
  await closeSessionWithPage();
}

// One entry page, two roots (docs/WINDOWS.md): a game window
// (`?session=<id>`) mounts the game view, and the main window its own UI,
// which for now is the same game view on the main session; the lobby
// replaces it later.
export default mount(App, { target: document.getElementById('app')! });
