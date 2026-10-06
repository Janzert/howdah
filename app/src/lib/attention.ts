// Drawing the user's attention when it's their move (or something else
// waits on them) and the window isn't focused: the OS's attention request
// (a flashing taskbar entry or a bouncing dock icon) in the app, a title
// prefix in a plain browser.
import { isTauri } from '@tauri-apps/api/core';

/** The title as it was before a prefix was added. */
let plainTitle: string | null = null;

function clearTitle() {
  if (plainTitle != null) document.title = plainTitle;
  plainTitle = null;
}

/** `what` says why, in the browser's title ("Your move" by default). */
export async function requestAttention(what = 'Your move') {
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
  } else {
    if (plainTitle == null) {
      plainTitle = document.title;
      window.addEventListener('focus', clearTitle, { once: true });
    }
    document.title = `● ${what} · ${plainTitle}`;
  }
}
