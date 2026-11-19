// Choosing files on this computer for the game database's export and
// import: the system's Save and Open dialogs in the app
// (tauri-plugin-dialog). The browser preview has none, so there
// `nativeFiles` is false and the panel asks for a path instead.
/** Whether the system's file dialogs are there (the app, not the
 * browser preview). The lobby, which uses them, is never docked. */
export const nativeFiles = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export interface FileFilter {
  name: string;
  extensions: string[];
}

/** The game database files and record files the import takes. */
export const IMPORT_FILTERS: FileFilter[] = [
  { name: 'Game databases and records', extensions: ['db', 'sqlite', 'txt', 'pgn', 'aei'] },
  { name: 'Game databases', extensions: ['db', 'sqlite'] },
  { name: 'Game records', extensions: ['txt', 'pgn', 'aei'] },
  { name: 'All files', extensions: ['*'] },
];

export const EXPORT_FILTERS: FileFilter[] = [{ name: 'Game database', extensions: ['db'] }];

/** A path to save to, from the Save dialog; null when cancelled. */
export async function chooseSavePath(defaultPath: string, filters: FileFilter[]): Promise<string | null> {
  const { save } = await import('@tauri-apps/plugin-dialog');
  return await save({ defaultPath, filters });
}

/** A file to open, from the Open dialog; null when cancelled. */
export async function chooseOpenPath(filters: FileFilter[]): Promise<string | null> {
  const { open } = await import('@tauri-apps/plugin-dialog');
  const picked = await open({ multiple: false, directory: false, filters });
  return typeof picked === 'string' ? picked : null;
}

/** A file name for an export: `games-<what>-<YYYY-MM-DD>.db`. */
export function exportName(what: string, now = new Date()): string {
  const slug = what
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '');
  const day = now.toISOString().slice(0, 10);
  return `games-${slug || 'export'}-${day}.db`;
}
