// Light or dark colors. `app.css` keys its dark palette off `data-theme` on
// the root element; with the System setting it follows the OS preference,
// live. The window frame is left to the OS: Tauri's window `setTheme` on
// Linux overrides the webview's `prefers-color-scheme` and `setTheme(null)`
// doesn't restore it, so System would stay stuck on the last forced value.
import type { Appearance } from './settings.svelte';

const systemDark = window.matchMedia('(prefers-color-scheme: dark)');
let mode: Appearance = 'system';

function apply() {
  const dark = mode === 'dark' || (mode === 'system' && systemDark.matches);
  document.documentElement.dataset.theme = dark ? 'dark' : 'light';
}

systemDark.addEventListener('change', apply);

export function setAppearance(m: Appearance) {
  mode = m;
  apply();
}
