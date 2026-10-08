import { mount } from 'svelte';
import App from './App.svelte';
import './app.css';
import { setAppearance } from './lib/appearance';
import { settings } from './lib/settings.svelte';

// Before the first paint, so a dark window doesn't flash light.
setAppearance(settings.appearance);

// In a plain browser (the Vite preview), talk to the dev bridge instead of
// Tauri. Must run before anything calls invoke or listen.
if (import.meta.env.DEV && !('__TAURI_INTERNALS__' in window)) {
  const { installDevBridge } = await import('./lib/devBridge');
  installDevBridge();
}

export default mount(App, { target: document.getElementById('app')! });
