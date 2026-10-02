// Drawing the user's attention when it's their move and the window isn't
// focused: the OS's attention request (a flashing taskbar entry or a
// bouncing dock icon) in the app, a title prefix in a plain browser.
import { isTauri } from '@tauri-apps/api/core';

const PREFIX = '● Your move · ';

function clearTitle() {
  if (document.title.startsWith(PREFIX)) document.title = document.title.slice(PREFIX.length);
}

export async function requestAttention() {
  if (document.hasFocus()) return;
  if (isTauri()) {
    try {
      const { getCurrentWindow, UserAttentionType } = await import('@tauri-apps/api/window');
      const win = getCurrentWindow();
      await win.requestUserAttention(UserAttentionType.Informational);
      // GTK leaves the urgency hint set until it's cleared.
      window.addEventListener('focus', () => win.requestUserAttention(null).catch(() => {}), { once: true });
    } catch (e) {
      console.warn('attention request failed:', e);
    }
  } else if (!document.title.startsWith(PREFIX)) {
    document.title = PREFIX + document.title;
    window.addEventListener('focus', clearTitle, { once: true });
  }
}
