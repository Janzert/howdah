# Several games and a lobby window

Plan for playing, watching and analysing several games at once, each in
its own window, with a main window that works as a home screen and
arimaa.com lobby. Written 2026-10-09; phases 1 (game windows) and 2 (the
lobby window) are built, the rest isn't yet.

## What other clients do

- **4steps** (read from source, `mainwindow.cpp`, `game.cpp`): a main
  window with menus (new game, custom position, log in, puzzles) and a
  tab per logged-in server, each tab that server's lobby (my, invited,
  open, live and recent games, create game). Every game opens in its own
  top-level window with its own menus and docks. A game that's already
  open opens a second window on the same server session, and the seat is
  left when the last window on it closes. The game window's size and dock
  layout are saved once and used for every new window; its title is
  "Gold - Silver". A window not in front asks for attention when the
  game starts or changes state, and plays the start sound.
- **arimaa.com's web client**: the lobby page opens each game in its own
  browser window.
- **lichess** (web): one game per browser tab. The home page is the
  lobby: quick pairing by time control, the seek list, correspondence
  games, and "playing now" with a small board per ongoing game, those
  waiting on your move first. In correspondence games an "auto-switch"
  setting (`ui/round/src/moveOn.ts`) moves you to the next game waiting
  on your move as soon as you've moved; a simul host gets the same.
- **En Croissant** (Tauri, GPL-3.0; `src/utils/tabs.ts`,
  `src/state/atoms.ts`): one window with tabs. Each tab is `new`,
  `play`, `analysis` or `puzzles`; a `new` tab is a start page of
  choices, and the first real tab replaces a lone one. Tabs and the
  active tab are kept in `sessionStorage`, so they last until the app
  closes.
- **Cute Chess** (`projects/gui/src/mainwindow.cpp`, `gamewall.cpp`):
  one tab per game in a main window, and as many main windows as you
  like, listed in a Window menu. A tournament adds a tab as each game
  starts. "Active games" opens a game wall: a grid of every running game,
  each with names, clocks and a live board.

Patterns worth taking:
- A game per window for live play (4steps, arimaa.com), so two games can
  be watched side by side.
- A lobby that lists your games, those waiting on your move first, with a
  small board each (lichess, the arimaa.com lobby's thumbnails in
  `UI-SURVEY.md` section 7).
- Moving on to the next game that's waiting on you (lichess), mainly for
  postal games.
- A wall of running games (Cute Chess), later, for engine matches.
- Remembering the game window's size once for all of them (4steps).

## What Howdah had

Before phase 1 (as built, see below the Phases):

- The backend already holds several sessions (`Backend::open_session`,
  `close_session`, `list_sessions`), each with its own controller, and
  every session command and event carries a session id. A window picks
  its session from `?session=<id>` in its URL (`lib/api.ts`), and
  `lib/events.ts` drops other sessions' events. Only the main window
  existed, on `MAIN_SESSION`.
- App-wide state: the arimaa.com login and its lobby poll
  (`Gameroom::watch_lobby`, `gameroom://lobby`), invitations
  (`gameroom://invitation`), the engine list and catalog. A followed
  arimaa.com game (`Watch`) belongs to its session.
- Settings are in `localStorage`, which windows share, but a window
  didn't notice another one changing them.
- The capability file allowed only the `main` window
  (`capabilities/default.json`).
- `App.svelte` is both the toolbar (New game, arimaa.com, Engines,
  Analysis, Record, Flip, Settings, Help) and the game view.
  `WatchDialog.svelte` is the arimaa.com lobby, as a dialog.

## Proposal

### Windows

- **The main window is the lobby**, with no board of its own: a home
  page and the arimaa.com lobby (below). Closing it hides it while game
  windows are open, and each game window's Lobby button shows it again;
  with no game windows open, closing it quits Howdah. Hidden, it keeps
  running, so the lobby poll and its notifications go on. Whether
  closing it should quit instead is for later, after some use (Brian,
  2026-10-09).
- **Howdah quits** when its last window closes: the last game window
  while the lobby is hidden, or the lobby with no game windows.
- **Each game gets a window**, on its own session: a local game or match,
  a loaded record, an analysis board, or an arimaa.com game played or
  watched. Its title is the players ("Bob - Al") or the record's name,
  with "Your move" marked when it is.
- **One window per arimaa.com game.** Opening a game that already has a
  window brings that window forward instead (4steps shares the seat
  between windows; we don't need to).
- **Closing a game window ends its session**, after the same checks as
  today: playing a live arimaa.com game it asks, as Leave game does (the
  game goes on at arimaa.com with the clock running), and a running local
  match asks too.
- **"New window"** in a game window opens an empty one; New game, Load
  and the like still act on the window they're used in.
- **Game windows remember one size** (the last one resized), as 4steps
  does, and open cascaded from the last. Reopening the windows that were
  open when Howdah quit can come later (En Croissant keeps its tabs only
  until the app closes).

### The lobby window

- **Home:** New game (against an engine or a person at the board),
  Analysis board, Open record (file or pasted text), and the Engines,
  Settings and Help dialogs, which then leave the game windows' toolbar.
- **arimaa.com:** what `WatchDialog` shows now, as a page: login, your
  games with "Your move" first, invitations, open games, live and
  recently finished games, postal games, player search, game id, and
  New game. Play, Watch and Open each open (or bring forward) a game
  window.
- **Open windows:** a list of the game windows, with a small board
  (`MiniBoard`), the players, the clocks and "Your move", and a click to
  bring one forward. This is the game wall, kept small.
- **Notifications live here:** the badges and sounds for invitations and
  postal moves due (now in `App.svelte`) move to the lobby, so they play
  once, not in every window, also while it's hidden. A game window keeps
  its own: its moves, its chat, its attention request.
- **Postal games without a window** need no polling of their own: the
  lobby's poll (`state`, once a minute, one request for all the user's
  games) already says whose turn it is in each, which is what the "your
  move" badge uses now. A postal game's moves are fetched only when its
  window opens (Brian, 2026-10-09).

### Game windows

- The board, move list, panels and the game's own controls as now, with
  a smaller toolbar: Analysis, Record, Flip, New game, and a Lobby button
  that brings the lobby forward (with the lobby's "your move" count, so
  a waiting postal game shows from any window).
- **Next game** (lichess's auto-switch, as a button and a key): after
  your move, bring forward the next game window waiting on your move, or
  open the next postal game from the lobby's list.

### Engines while playing online

As now, only the window where the user plays an arimaa.com game refuses
analysis. Another window could still analyse the position; at some point
the client has to trust the user and community norms (Brian,
2026-10-09).

### Shared state

- Settings: each window listens for `storage` events and reloads
  `settings` when another window saves them, so a theme or sound change
  applies everywhere.
- Things that are per window stay per session: the board's flip, the
  analysis engine and whether analysis is on, the folded panels.
- Engines: the "shares the CPU" note counts engines running in every
  session, not just this one.

### Building it

- **Opening a window:** the frontend creates it (`WebviewWindow` from
  `@tauri-apps/api/webviewWindow`, label `game-<session>`, URL
  `index.html?session=<id>`), after `open_session`. In the browser
  preview the same call is `window.open`, so the dev bridge and the e2e
  tests (Playwright's new page) work the same way. The capability file
  allows `game-*` windows and creating windows.
- **One entry page, two roots:** `main.ts` mounts the lobby without
  `?session=`, and the game view (today's `App.svelte`, slimmed) with it.
- **Closing:** the backend closes a session when its window is destroyed
  (Tauri's window events, by label), whatever the frontend managed to do
  first; in the browser preview, `pagehide` calls `close_session`.
- **Hiding the lobby:** the backend catches the main window's close
  request (Tauri's `CloseRequested`), hides it while a `game-*` window
  exists, and lets it close otherwise; a game window's Lobby button shows
  and focuses it. The browser preview has no hiding: its lobby is just a
  tab.
- **Finding a game's window:** the backend keeps which session follows
  which arimaa.com game (it does through `SessionHandle`'s `Watch`), so
  "open game 539546" can answer with the session that already has it.
- **`MAIN_SESSION`** goes away once the main window has no board.

## Phases

1. **Game windows** (done 2026-10-09): the two roots, opening and
   closing windows (Tauri and the browser preview), sessions ending with
   their windows, the capability, settings kept in step, and an e2e test
   with two windows. The main window keeps today's UI, with a "New
   window" button.
2. **The lobby window** (done 2026-10-09): the main window becomes home
   plus the arimaa.com lobby (moved out of `WatchDialog`), the
   open-windows list, and the notifications moved there. Game windows
   get the smaller toolbar.
3. **Moving between games:** one window per arimaa.com game, Next game,
   "your move" in titles, and remembering the window size.
4. **Later:** reopening windows after a restart, a full game wall for
   engine matches, tabs as an alternative to windows if they're wanted.

**As built (phase 1)**, where it differs from or adds to the plan:
- The main window kept `App.svelte` until phase 2; game windows have the
  "New window" button, as the Proposal has it.
- Closing a game window asks
  while it plays an arimaa.com game (not one only watched) or a match
  is on (`closeWarning` in `App.svelte`), in an in-app dialog
  (`CloseWindowDialog.svelte`); confirming closes it, and the backend
  ends the session (leaving the arimaa.com game) when the window is
  destroyed. The browser preview can only offer `beforeunload`'s
  generic prompt, and only in game windows.
- Browser preview: a game window's tab closes its session on `pagehide`
  with a beacon, so reloading it starts a new session (an empty game);
  Tauri windows keep their session across a reload.
- Settings follow `storage` events; checked in the real app too
  (WebKitGTK delivers them between Tauri windows).

**As built (phase 2):**
- `Lobby.svelte` is the main window's root: "Play and analyse" (New game,
  Analysis board, Open record), "Open windows", and the arimaa.com
  section (`GameroomLobby.svelte`, the old dialog as a page), with
  Engines, Settings and Help in its header. `MAIN_SESSION` is gone: the
  backend starts with no sessions, and every session is a game window's.
- Everything the lobby starts opens a new game window, set up before
  the window opens (`openGameWindow(prepare)` with `apiFor(id)`), so a
  failed start (a refused seat, a bad record) shows its error in the
  lobby and leaves no window. A match's spec goes to the window through
  localStorage (`takeHandOff`), so Rematch and Swap sides still work
  there. The Analysis board opens with the last analysis engine on.
- Open windows lists each with a `MiniBoard`, the players, the clocks
  (move time and reserve, as the player bars) and "Your move", those
  first; a click brings the window forward. The backend sends
  `sessions://changed` when a session opens or closes; the lobby follows
  every session's `game://changed` and `gameroom://watch` (`onEvery`).
- Game windows' toolbar: New game (in that window), New window,
  Analysis, Record, Flip and Lobby. Help stays on `?`, and the Engines
  dialog on the New game dialog's Engines button. The Lobby button shows
  how many invitations and postal moves due wait there; a window opened
  between lobby polls gets them from the backend's last lists
  (`gameroom_last_games`, no server request).
- Notifications (flash, sound, attention request) come from the lobby
  only, hidden or not; postal games open in a window don't count.
  Accepting an invitation, or one of the user's being accepted, opens a
  game window playing it.
- Closing the lobby hides it while a game window is open (`lib.rs`,
  `CloseRequested`); closing the last game window while it's hidden
  quits, and so does closing the lobby with no game windows. On macOS
  the dock icon shows a hidden lobby again (`RunEvent::Reopen`, not run
  by hand). Checked in the real app in Xephyr on Linux.
- `cancel_gameroom_game` no longer takes a session: a session playing
  the cancelled game starts a new one, whichever it is.
- In the browser preview the lobby is the tab at `/`, and tabs are
  named, so the Lobby button and the windows list find the tab instead of
  opening another (a browser may not switch to it).
- Not done: the "shares the CPU" note in a game window still counts only
  its own session's engines (Shared state, above).

Windows come first; nothing in the design should rule out tabs (a game
view that doesn't assume it owns the whole window, and a session per
game either way). Brian, 2026-10-09.

## Decided

Brian's answers to the first round of questions (2026-10-09), recorded
above: analysis is refused only in the window playing the game; closing
the lobby hides it while game windows are open; postal games without a
window rely on the lobby's poll; windows now, keeping tabs possible.

## Open questions

- Whether closing the lobby should quit, once it's been used for a
  while.
- Whether "Next game" should also switch automatically after a move, as
  lichess's setting does, or stay a button and a key.
