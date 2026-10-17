# Changelog

Notable changes to Howdah, newest first. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions
follow [semantic versioning](https://semver.org/) (0.x while Howdah is
young).

## [Unreleased]

### Added

- The arimaa.com lobby shows the gameroom's announcements edited in the
  last 90 days, each with a Hide button; older and hidden ones are under
  "Older announcements". Their links open in the system's browser.
- The arimaa.com lobby lists who's logged in to the gameroom, with an
  Invite button for players who take invitations; it fills in the New
  game form's opponent.
- `howdah-tourney`, a command line for testing engine changes: it plays
  two engines in colour-swapped pairs, several games at once, keeps every
  game and the results in the test's directory (with copies of the
  engines, so rebuilding one doesn't change a running test), resumes
  where it stopped, and reports score, Elo, LOS and an SPRT that ends the
  test once it decides.
- Engine tests in the lobby: start the test daemon, start a test of two
  engines from the engine list, and follow its results (score, Elo and
  the SPRT) as games finish. Each test's games open in a game window: a
  running one follows the game live with its clocks, a finished one
  opens as a record. Tests can be paused, resumed, stopped and
  reprioritized, their pair limit and games at once changed, and the
  daemon given more or fewer games at once. A stopped or paused test can
  be archived (zipped into the daemon's `archive/` folder) or deleted
  from its dialog, and Archive finished archives every finished test.
  The Archived tests dialog lists archived tests to restore or delete.
  `howdah-tourney archive|delete ID`, `archived`, `restore FILE` and
  `delete-archived FILE` do the same from the command line.
  A test game's window title names the test and the game (pair.game),
  and Follow opens a window that moves on, 15 s after each game ends,
  to the test's newest game not open in another window. A running test
  game shows both engines' panels (depth, eval, PV and log) as a local
  engine game does, and its earlier moves show their times.
- `howdah-tourney daemon`: a long-running queue of engine tests sharing
  a number of game slots, highest priority first, that carries on after
  a restart. The same command submits, lists, watches, pauses, resumes,
  stops and reprioritizes tests, over a local HTTP API with a token.
- The engine test daemon keeps engines' memory in check: an engine that
  has grown too much (three times its size after its first game, by
  default) is restarted between games, one past its limit (a quarter of
  the machine's memory, by default) forfeits its game, recorded as a
  memory-limit forfeit, and the engines together stay within a budget
  (half the machine's memory, or `--memory-budget`). On Linux each
  engine also runs under a kernel memory cap (a systemd user scope) when
  it can. Limits can be set per test (`[memory]` in `test.toml`), and
  each game's results and the daemon's log show the engines' memory.
- A position editor, from the lobby or from a game window ("Edit
  position", `e`, which opens the shown position in a new window): place
  pieces from a palette per side (it counts what's left), drag them
  around or off the board, set the side to move and the move number, and
  start from both default setups, clear, flip, mirror or swap colors.
  Positions can be typed as 4steps takes them (`Ed4 Ra2x Hb2n s`), pasted
  in the short format or as a board diagram, and copied in either. It
  says what keeps a position from starting a game (too many of a piece,
  a piece alone on a trap, a rabbit on its goal row, no rabbits, no legal
  move) and marks the squares. Analyse starts free play from the
  position, with analysis on; Play… starts a game from it against an
  engine or with clocks.
- arimaa.com's puzzles: the lobby's Puzzles lists them, and each opens in
  a window to solve. Your moves are checked against the answer, the
  other side's replies are played, and it says when you've solved it;
  there's a hint, Try again after a wrong move, Show answer and Next
  puzzle, and solved puzzles are ticked in the list. Puzzle files in
  arimaa.com's format (the variables its old Flash viewer loaded) open
  from Open record too.
- Games from a set position: records keep it in a `Position` tag, and
  such records now load (they were refused). Engines are given the
  position with `setposition`.

- Playing arimaa.com's server bots, as 4steps's bot launcher does: the
  lobby lists the official bot ladder (with your place on it) or every
  bot, with your record against each, shows a chosen
  bot's time control and whether it's running, and starts it. With "Join
  on creation" the game it opens comes up in a new window at once; with
  "Keep rated" off you join in the gameroom's unrated mode, so a rated
  bot game isn't rated.
- Each game has a window of its own, so several games can be played,
  watched or analysed at once. The main window is now a lobby: new games,
  an analysis board, opening a record, the open game windows (with a
  small board, the clocks and "Your move"), the arimaa.com lobby that was
  a dialog, and the Engines, Settings and Help dialogs. Invitations and
  postal moves due are announced there, also while it's hidden (closing
  it hides it while games are open); a game window's Lobby button brings
  it back. Closing a game window ends its game; while it plays an
  arimaa.com game or a match is on, it asks first. Settings changed in
  one window apply in all of them.
- Moving between games: an arimaa.com game opens in one window only (its
  window comes forward when opened again), "Next game" (`n`) goes to the
  next game waiting on your move, a game window's title names the
  players and says "Your move" when it's yours, and new game windows
  open at the size you last gave one, cascaded.
- Clocks at each move: going through a finished game (an arimaa.com
  game, or a record with a time control and move times), the player
  bars show the clocks as they stood after the shown move, with the time
  that move took and the reserve after it, as arimaa.com's game viewer
  does. A timed game's record now keeps its time control, one per side
  (`GoldTimeControl`, `SilverTimeControl`) when they differ.
- A setting to play the low-time ticks for both clocks in a game with no
  human player at this machine: one watched on arimaa.com, engine against
  engine, or an engine test game (off by default; otherwise only a human's
  own clock ticks).

### Changed

- The lobby is laid out anew: a sidebar lists Home, arimaa.com and
  Engine tests, the open boards with a small board each, and Engines,
  Settings and Help, each a panel of the main window; it folds to icons
  and boards. Games now open as boards in the main window by default:
  each can pop out to a window of its own and dock back, and the sidebar
  goes to its panel or its window (Settings has "New boards: in their own
  windows" for the old way). The arimaa.com panel opens on an overview
  (invitations and your games, open games, and the first of the live,
  finished and online lists), with buttons that open its other panels
  (the server's bots, games to watch, finished games, players online),
  listed under arimaa.com in the sidebar until closed. New games and
  invitations go through a New game dialog, which Invite opens with the
  player filled in. It offers the same time controls as a local
  New game: now all of arimaa.com's presets, postal ones included, by
  the site's names. Engine tests show as cards; each test opens in its
  own panel, listed under Engine tests with its score. Notifications about
  arimaa.com have a Show button.
- Fast-forward (`p`, between Forward and End) steps through the moves to
  the end of the shown line, each animated; press again to stop.
- Home offers to download Sharp or OpFor while no engine is installed.
- Puzzles is a lobby panel, with a filter and how many of each group are
  solved.
- The lobby's keys: Alt+↑/↓ through the sidebar, Alt+1–9 to an entry by
  number, also from a docked board.
- An arimaa.com game opens its board at once, with a spinner until the
  game has loaded (or what went wrong, and Close).
- Boards without players are titled for what they hold (Analysis board,
  Game record, New board), and a loading one says so in the sidebar.
- Every dialog has the same frame; arimaa.com's Bots panel picks your
  side with the same toggle as its New game.
- A board's move list is one tab stop, and its engine panels take a line
  each, leaving more room for the moves; the move buttons are icons.
- A board's buttons are in tidy rows, with Lobby, Dock or Pop out and Next
  game apart from the game's tools; "New window" is "New board".
- A refused third repetition names the moves after which the position
  stood before ("as after 12g and 14g"), as arimaa.com does.
- Installing an engine update adds the new version as its own engine,
  with the earlier one's options, and offers to remove the earlier one;
  until then both can be used. A new engine whose name another engine
  already has gets its version added to the name (which can be changed in
  the Engines dialog).
- Deleting an engine installed from a manifest deletes its downloaded
  files too, after a second click.
- The arimaa.com game panel says whether the game is rated, with its
  time control, instead of the server's event name ("Casual game" for
  nearly every game), and the disabled "Ask for takeback" button says
  why it's disabled.

### Fixed

- The engine test daemon's idle game slots kept their engines running,
  so a daemon with more slots than a test's games at once held extra
  engines (and their memory) for as long as it ran. A slot now lets its
  engines go after 10 seconds with nothing to play. Ending an engine
  also ends whatever it started, and on Windows an app's or daemon's
  engines no longer outlive it.
- Logged in to arimaa.com with your email address, the lobby showed the
  email as your name and didn't find your side in your own games (Play,
  "Your move", postal moves due); it now uses your gameroom username.
- Games ended by the game or turn limit are scored by arimaa.com's rule:
  with equal piece counts, the side ahead after the most recent turn
  where the counts differed wins (silver only if they never differed),
  not silver on every tie.
- "Move now" right after Sharp started thinking no longer makes it lose
  on time, and analysis no longer stalls when Sharp is stopped that
  early. (Sharp's next release also answers such a stop with a move.)
- On Linux, drop-down lists and tooltips match the light or dark setting
  instead of the desktop's theme.
- The analysis line holds still while the pointer is over it, so a click
  adds the line that was shown.
- Long player names shorten instead of wrapping in a narrow window, and
  the setup header no longer wraps.

## [0.1.0] - 2026-10-08

The first release, with installers for Linux (AppImage, deb, rpm),
Windows (MSI and NSIS setup) and macOS (dmg, signed ad hoc). None are
code-signed; the release notes say how to get past SmartScreen and
Gatekeeper.

### Playing and reviewing

- The complete rules, including the repetition rule and the one-minute
  setup allowance in timed games.
- A board with drag, hover-arrow and step-mode input, animated moves,
  frozen pieces marked, and themes.
- A move tree with variations, comments and move glyphs, and a move list
  that can show each move's time or the time into the game.
- Game records in the PGN-style Arimaa format, read and written,
  including variations, results inside variations and move times.
  Records from the arimaa.com archive read as is, `takeback` lines
  included.
- Clocks colored by the time left for the turn, as on arimaa.com.
- Sounds for game events, including per-theme board sounds.
- Light and dark colors, following the system or chosen in Settings.

### Engines

- Play against an AEI engine or watch two engines play, with clocks and
  time controls.
- Download and update engines from their manifests: Sharp and OpFor are
  offered from the first run, and other engines can be added by manifest
  URL, manifest file, or program path.
- Engine options as fields in the engine's settings, for one game, or
  for analysis.

### Analysis

- An engine searches the shown position, with its evaluation, principal
  variation (added to the tree with a click) and search output.

### arimaa.com

- Log in to the gameroom to watch live games and open finished ones.
- Play: create and join games (rated or unrated, live or postal),
  invitations, chat, takebacks, timeouts, and opponent presence.
- Keeps playing through network drops and sleep, and resumes games in
  progress.

[Unreleased]: https://github.com/Janzert/howdah/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/Janzert/howdah/releases/tag/v0.1.0
