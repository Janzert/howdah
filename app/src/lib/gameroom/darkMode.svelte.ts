// Whether the page shows the dark appearance (`data-theme` on the root,
// set by `lib/appearance.ts`), as reactive state for colors worked out in
// code, such as the chat's message colors.
const isDark = () => document.documentElement.dataset.theme === 'dark';

class DarkMode {
  dark = $state(isDark());

  constructor() {
    new MutationObserver(() => (this.dark = isDark())).observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['data-theme'],
    });
  }
}

export const darkMode = new DarkMode();
