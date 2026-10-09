// Drawing the user's attention when it's their move (or something else
// waits on them) and the window isn't focused: the OS's attention request
// (a flashing taskbar entry or a bouncing dock icon) in the app, a title
// prefix in a plain browser. It also keeps the window's title
// (`setTitle`), which that prefix goes in front of.
import { isTauri } from '@tauri-apps/api/core';

/** The window's title without the attention prefix. */
let baseTitle = document.title;
/** The browser's attention prefix, while the window waits for focus. */
let prefix: string | null = null;

function showTitle() {
  document.title = prefix ? `● ${prefix} · ${baseTitle}` : baseTitle;
}

function clearTitle() {
  prefix = null;
  showTitle();
}

/** Sets the window's title (the window frame's too, in the app). */
export async function setTitle(title: string) {
  if (title === baseTitle) return;
  baseTitle = title;
  showTitle();
  if (isTauri()) {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    await getCurrentWindow()
      .setTitle(title)
      .catch((e) => console.warn('setting the title failed:', e));
  }
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
    if (prefix == null) window.addEventListener('focus', clearTitle, { once: true });
    prefix = what;
    showTitle();
  }
}
