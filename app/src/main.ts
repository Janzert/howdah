import { mount } from 'svelte';
import App from './App.svelte';
import './app.css';

// In a plain browser (the Vite preview), talk to the dev bridge instead of
// Tauri. Must run before anything calls invoke or listen.
if (import.meta.env.DEV && !('__TAURI_INTERNALS__' in window)) {
  const { installDevBridge } = await import('./lib/devBridge');
  installDevBridge();
}

export default mount(App, { target: document.getElementById('app')! });
