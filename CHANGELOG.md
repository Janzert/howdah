# Changelog

Notable changes to Howdah, newest first. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions
follow [semantic versioning](https://semver.org/) (0.x while Howdah is
young).

## [Unreleased]

## [0.9.0] - 2026-11-25

### Added

- Importing a newer analysis of a game you already have, by the same
  engine (records naming it as their `Annotator`), asks what to do with
  the earlier analysis's lines that the new one doesn't have: replace
  them (keeping any you've gone on from or commented on) or keep both.
  The moves' comments become the newer ones either way, unless you've
  edited them. Boards showing the game follow along.
- Removing a source in the Games panel's Sources… also removes the
  lines it added, except those you've gone on from or commented on.
- The game record can carry the engine's analysis: the Record dialog's
  "Evals" option writes each analysed move's eval in its comment as
  `[%eval]`, from gold's side, as chess's PGN tools write them, and
  "Win %" gold's chance of winning as `[%win]`, for engines that give
  one.
- The chat room lists the games being played on arimaa.com above its
  log, with the move each is at, and any one of them can be shown on a
  small board beside the log: the players, their clocks and the
  position, its moves animated as they're played, with Watch to open it
  on a full board.
- Declining an invitation can carry a message, which the inviter sees
  as the reason: Decline opens a box for it in the invitation's row
  (Enter sends, Esc or Back closes it).
- The record by opponent on a player's page sorts by any column, and
  shows each opponent's rating at their newest game. The ratings show
  only once the game database holds the whole archive.

### Changed

- A line added from the engine's PV (a click on one of its moves in the
  Analysis panel), and the engine's line beside a mark from Analyse
  game, has the engine's eval on its first move in the move list, as
  the analysed moves of the game have, and keeps it with the game's
  evals.
- arimaa.com's games name the server as their site, `arimaa.com`, in
  place of the archive's "Over the Net", which every game had. Games
  already in the database are updated when it opens.
- Analyse game looks at the setups too, so an engine that scores a
  setup (Clew) gives the setups an eval on the move list and the eval
  graph. Engines that don't (Sharp, OpFor) answer at once, so it costs
  nothing; setups still get no move marks.
- New sounds for an invitation (also a takeback request and a postal
  move due), a short two-note chime in place of a click too brief to
  notice, and for a chat line, a louder click that's easier to hear.
- Bots in the arimaa.com online lists are shaded instead of tagged,
  since their names already start with `bot_`.

### Fixed

- A finished arimaa.com game opened before it reached the archive
  lacked the event, site and times the archive's games have; it now
  gets its site at once and the rest from the game database download
  once that has the game.
- Copying (a move, the moves to it, a record, a position, a chat link
  or the version) no longer fails on Linux with "NotAllowedError".
- The board preview of an engine line's move closes when the move is
  clicked to add the line.
- In the arimaa.com game lists, a short player name beside a long one
  is no longer cut to a few letters: only a name too long for its half
  of the row is shortened.
- An invitation's message is shown whole, on a line of its own, instead
  of cut off at the end of the row.
- A game whose opponent never joined (arimaa.com ends it as abandoned,
  with no winner) no longer stops the watch with an error: the board
  shows the game as over, with a note.
- The chat room's log keeps its place: at the end when lines are
  rewritten (yesterday's lines getting their date at midnight), and at
  the line it was scrolled to when the log is made narrower or wider.

## [0.8.0] - 2026-11-20

### Added

- A game database: every game on a board is saved as it changes
  (played, watched, opened from a record, or analysed; not puzzles or
  engine tests' games), in `games.db` in the app's data directory. A
  finished arimaa.com game opened from a list or a link is looked for
  there first, so it opens at once and offline, and one fetched from
  arimaa.com is kept, with any analysis added to it. The lobby's Games
  panel lists them, newest first, narrowed by player (by any name
  they've had), opponent, side, won or lost, ending, dates, where
  they're from and rated; each opens on a board, and can be deleted.
- The Games panel offers arimaa.com's whole game archive (425,129
  games through 2026-10-11) as one download of about 143 MB, merged
  into the games already there. Games added to the archive later come
  as small updates holding only the new ones. Howdah looks for updates
  at most once a day; the panel's refresh button looks at once.
- A setup explorer, the Games panel's Setups view: gold's setups,
  silver's, or silver's replies to one gold setup, over the games the
  list's filter lets through, each with its games, gold's score beside
  what the ratings predicted, the average rating of the side playing it
  and when it was last played. A setup and its mirror image count as
  one. A row's Games narrows the list to that setup or pair, and a
  silver reply goes on into the moves played after it, move by move.
- The board's Explorer, under the comment box: the moves played next
  from the shown position in the database's games, with how each went.
  A move plays as a variation, and the games reaching the position open
  on a new board at that point. It's off during an online game you're
  playing.
- Position search: the Games panel can list the games reaching a
  position, exactly or by a pattern of pieces on squares, set in the
  position editor (Find position, Find these pieces).
- Sharing games: the Games panel's Export… writes the list's games to
  a file, with variations, comments, move times, evals and (if chosen)
  chat, under an author name you give. Import… takes such a file or a
  record file, joining variations, comments and evals into games
  already there. Sources… lists where the database's contents came
  from, filters the list by one, and removes what one brought.
- Analysis is kept: what an engine finds about a position is saved in
  the game database by position, so it shows again in any game that
  reaches that position, without the engine running. Each engine
  version's analysis is kept apart.
- The engine panel shows an engine's chance of winning when it gives one
  (`win 63%`), with its eval in rabbits on hover, and a forced result as
  `win in 2` or `loss in 2`.
- A proven win says how many of the winner's turns it takes, when the
  engine gives the count (`info proven`, or Sharp's search log):
  `Gold wins in 2` in the analysis panel and comments, `W2` or `L2` in
  the move list.
- Marked moves in the move list show the engine's better line under
  them, and the analysis panel shows what the engine would have played
  from the position before the shown move, from the analysis already
  done.
- Engines are told how long their setup gets, with AEI's new `tcsetup`
  option: arimaa.com's minute in games with a person, an engine test's
  `setup_minimum`, or 0 (the plain move time). Engines that don't know
  it, Sharp and OpFor among them, log a warning and play on.
- Engine tests and matches can let an engine ponder (`ponder = true`
  on a test's engine): it searches through the opponent's turn, and the
  opponent's move ends the search.
- Engine tests can start their games from positions reached by play
  (`[positions]` in a test's spec: a file with each position's moves
  from the start, sent to the engines as moves). Each position is
  played from both sides, or with `sides = "to_move"` with the engine
  under test always the side to move. A game's record says where play
  started (a `StartMove` tag and a comment in the move list).
- An engine test's panel shows how its games ended (goal, elimination,
  immobilization, time and the rest), with each engine's wins for each,
  and each engine's wins and losses as gold and as silver.
- A player's page counts how the games loaded so far ended (goal,
  elimination, time and the rest), won and lost, with rows for rated,
  unrated and all games. Clicking a row's name or a count shows those
  games, so the table filters the list by result, ending and rated. It
  asks arimaa.com for nothing more; Older games adds to what it counts.
- The Players tab can find any arimaa.com player, not just those
  online, and open their page (an exact username opens it at once).
  Finished games' player search offers each player's page too.
- The gameroom's game lists (live, yours, open, recent, past and
  postal) show the players' ratings, and a player's name opens their
  page. Past games show the ratings from before each game where the
  game database or the player's rating history has them.
- A finished arimaa.com game shows the comments people left on it,
  below the board. The lists of a player's games and of recent games
  say how many comments a game has.
- Others' messages in the gameroom's chat room play the chat sound. A
  button in the room's bar mutes it, as does "Chat room messages" in
  Settings.
- Times of day (the chat room's and its archive's, when a game ended, a
  player's local time) follow the system's 12- or 24-hour clock: the
  desktop's clock setting on Cinnamon, GNOME, Unity and Budgie and
  `LC_TIME` on Linux, the regional time format on Windows, the 24-hour
  switch on macOS. Settings has a Time format option to choose one
  instead.

### Changed

- Analyse game takes a time for the whole game instead of a time a
  position. It gives every position a quick look, then spends the rest
  where the evals or the marks are least settled, so the marks most
  likely to change get the most search. On a game being played it
  follows the new moves. Marks that more search may still change show
  lighter, and clicking the eval graph has that move searched next.
  "Until stopped", now the default, keeps going with no time limit,
  deepening the whole game once every mark has settled.
- The board's side panel is slimmer: the eval graph sits under the
  board, Analyse game in the analysis header, and the tools and window
  buttons are one row with a menu (Record, Position editor, Pass, Lobby,
  Dock or Pop out). Next game shows only when there is one, and the
  turn's buttons sit beside the bottom player bar only while a turn or
  setup is being entered.
- The eval bar shows a hatched, empty state while the shown position
  has no eval yet, instead of an even game, and the first eval grows
  from the previous move's.
- In the gameroom's game lists, only a row's Watch or Open button opens
  its game; the player names open their pages.
- A bot's player page labels the name in its profile as Author, since
  that's the name a bot's profile gives.
- In an engine game, the side panel lists the engines in the board's
  order, the side at the top first, instead of always gold first.
- The low-time ticks wait until half of the turn is gone, so a short
  turn (under a minute) no longer ticks from its start: a 7-second turn
  starts ticking at 3.5 seconds left. Turns of a minute or more tick as
  before.
- A time control of two fields (`2/2`, `0:30/3`) adds nothing to the
  reserve, as arimaa.com reads it: its game logs show no time added in
  those games. Howdah used to add all of the unused move time. A time
  control that adds it all is now written with the `100`
  (`1m30s/5m/100`).
- Unused move time is added the way the time was measured. Where turns
  are charged in whole seconds, as arimaa.com charges them (the clocks
  worked out for a finished game's moves), the addition is rounded up
  to a whole second, as arimaa.com's game logs show, not to the nearest.
  Games played here, measured to the millisecond, add it to the
  millisecond, so a fast game no longer gains up to half a second a
  move from the rounding.
- Building Howdah needs Rust 1.100 or later. Tauri is 2.12.7.

### Fixed

- Analyse game no longer leaves positions without an eval when the
  engine gives its line only on being stopped, as OpFor often does in a
  short search: it waits for the engine's answer to `stop` (up to two
  seconds) before moving on, and searches a position left without an
  eval again.
- The chat room's lines from yesterday get their date after midnight,
  instead of looking like today's until the next message arrived.
- An SPRT test's chart draws its pass and fail curves with the test's
  own pair variance, so a test short of its bound no longer looks past
  the pass line.
- The analysis panel's engine row and Analyse game stay put at the
  panel's bottom while the engine's line changes length, instead of
  moving under the pointer.
- The menu button's dots no longer look greyed out.

## [0.7.1] - 2026-11-13

### Added

- A player page for arimaa.com players, opened from the online players:
  their profile, rating over time, games, and record by opponent with
  the games against each. Each player gets a panel of their own, and an
  opponent in the record opens theirs.
- A game with a game time limit or a turn limit shows the time and turns
  left under the turn bar, counting down as the game goes.
- The Linux packages carry AppStream metadata, so software centres such
  as GNOME Software and KDE Discover describe Howdah, and its desktop
  entry files it under Games.
- Packagers can build without the check for new versions: with
  `HOWDAH_NO_UPDATE_CHECK=1` set while building, Howdah never asks
  GitHub, and Settings doesn't offer it.

### Changed

- In a live game, the side waiting for its turn shows the time its last
  move took beside its reserve, as arimaa.com's gameroom does, rather
  than the move time its next turn will have.
- Under a maximum turn time, the side to move shows only the reserve it
  can use this turn, as arimaa.com does, with its whole reserve in the
  clock's tooltip (in a gameroom game, from the last reserve the server
  reported while the side waited, since on its turn it reports only the
  usable part); the waiting side still shows all of its reserve.

### Fixed

- A gameroom game whose result arimaa.com reports before its last move
  or two (seen in bot games) no longer loses those moves: Howdah fetches
  the finished game and plays them before ending it.
- A gameroom game whose last two moves reach Howdah together with the
  result (a bot answering at once) now shows the game-end dialog and
  plays its sound, as other endings do.
- With analysis on, a position that ends the game (goal, elimination or
  immobilization) fills the eval bar for the winner, rather than showing
  it even. A loss on time or by resignation still shows the engine's
  eval of the position.
- A message being written in the arimaa.com chat room is kept when you
  go to another panel and come back.

## [0.7.0] - 2026-11-10

### Added

- Howdah says on Home when a new version is out, with a link to the
  release. It asks GitHub at the start and once a day; Settings turns
  this off.
- A pool's crosstable can show each matchup as wins–losses, win %,
  head-to-head Elo, or the residual: how many Elo better or worse it went
  than the ratings predict. Each cell's tooltip gives all of them.
- A loaded game shows what its record says about it above the move
  list: the event, time control, whether it was rated, the date and
  the arimaa.com game id.
- Dragging an enemy piece two squares can finish a pull and start a
  push in one drop, as in 4steps: for a flip, step the stronger piece
  away, drag the enemy piece through its square and on, and step back.
- A drag of the pushing piece can finish the push and carry on to
  another square.
- An SPSA run's panel gives each parameter's move from its start in
  multiples of c: a ×c column in the parameter table (ordered furthest
  in c first by default), in a parameter's tooltip and chart header, and
  as the "Moved more than c" card's mean move.

### Fixed

- The arimaa.com panel's recently finished games show their time
  control, as the full list of recent games already did.
- A long SPSA run's panel, and a test with many finished games, no
  longer take seconds to draw (2,000 steps of 127 parameters: 4.2 s,
  now 0.17 s).

### Changed

- The arimaa.com chat room shows seconds in its times, since people
  there refer back to a message by its minutes and seconds.

## [0.6.1] - 2026-11-01

### Added

- A text size for the arimaa.com chat room, its archive and a game's
  chat, in Settings.
- The arimaa.com chat room's log scrolls back through the archive:
  earlier chat loads as you near the top, or with Load earlier.
- A button in the chat room shows how many new messages have come in
  while scrolled back, and goes to them.
- Pass (`P`) on an analysis board gives the other side the move, to see
  what it could do. It shows as `pass` in the move list and in records;
  engines analyse the position after it. Passing isn't offered in a game.

### Fixed

- The chat room and its archive keep their scroll position when you go
  to another panel and come back.
- The chat room's message preview no longer hides the log's last lines.
- A position opened from a chat link shows the same way up as its preview:
  a `boardimg.php` link's `orient` now carries over to the board. Its
  preview also says whose move it is, the side it's seen from.

## [0.6.0] - 2026-11-01

### Added

- Position links in the arimaa.com chat room and its archive
  (`planGame.cgi?movelist=...` and `boardimg.php?ranks=...`) show their
  final position in the chat, and open on a board, as do links to finished
  games and puzzles. Right-clicking a chat link copies it or opens it in
  the browser.
- The move list shows a set position from move 2 as the setups that would
  place it, in place of `start`.
- A banner over the board while its arimaa.com game has lost the
  connection, or has stopped following the game.
- Analyse game, in the analysis panel, a very early version that needs
  more work: the engine searches each position of the main line for a
  set time, and the evals show in the move list and on a graph of the
  game; clicking the graph goes to that move.
  Moves that lose ground are marked `?!`, `?` or `??`, and the panel can
  add the marks to the game with the engine's better lines as variations.
- The arimaa.com Finished games panel lists the last 100 games, not five,
  25 at a time with a More games button.

### Fixed

- A game that ended during a long network outage stayed on "Reconnecting"
  for ever, since the server doesn't answer at a table it has cleared
  away. Now its final result and move times arrive. Tries for a new seat
  that the network stopped also no longer count towards giving up.
- After taking a new seat, moves could go on going to the old one.
- Puzzles and engine test games open their board at once, with a
  spinner while the game loads, as arimaa.com games do.

## [0.5.0] - 2026-10-30

### Added

- Pools in the test daemon: a spec with `[pool]` plays two or more
  engines against each other for as long as it runs, and engines can be
  added or retired while it does. It rates them together from all their
  games (Bradley–Terry, with 95% intervals), shows a crosstable, and
  points out matchups whose results are out of line with the ratings.
  Each pair's matchup is chosen by what it would tell: every matchup
  gets a couple of pairs first (so a new engine catches up against the
  rest), then the ones whose rating difference is least known get the
  most, unless the matchup is lopsided and its order already settled.
  `howdah-tourney add-engine`, `retire` and `unretire` change a running
  pool, and `show` prints its standings. How it works is in
  `docs/TOURNAMENTS.md`.
- A pool's panel in the lobby: ratings with their intervals, the
  crosstable shaded by how far each result is from the prediction, the
  outliers, and adding and retiring engines. New test makes one from the
  engine list.
- The SPRT funnel in an SPRT test's panel: the observed Elo after each
  game, running down between the Elo at which the test would fail and
  pass, which close in as it goes. Click it for a full-size view with
  the band where the test stops undecided, where it's likely to end,
  and the cutoffs at any game count under the pointer. The LLR bar now
  sits below the results and the funnel.
- A sound button on each board: every sound, no step, move or capture
  sounds (your turn, the clock and the rest still play), or muted.
  It's the board's own, under the Settings' sound, and goes with the
  board when it's docked or popped out.
- Next game on a board showing an engine test's game goes to the next
  game the test started after it, running or finished, in the same
  board (`n` too). It turns on as soon as there is one.
- An About section in the Help panel: the version, with Copy, and links
  to the releases, changelog, source and acknowledgements.
- Clocks in a time control with fractions of a second show them under
  10 seconds: tenths while running, milliseconds when stopped. The move
  list's times show them too (`0.123`).
- An estimate of the end for tests with a pair limit and no SPRT: the
  games left, and the time at the current pace.
- A note on the Engine tests panel when the running test daemon is from
  another version of Howdah. A daemon goes on across an upgrade until
  it's shut down, and tests carry on when it starts again.

### Changed

- The board's New game and New board buttons are gone; both are in the
  lobby's Home. Its Edit position button is now Position editor, as on
  Home, since it opens the editor on a new board.
- Watching a test's game, the game-end sound is the tested engine's
  (engine 1's): the win sound when it wins, the loss sound when it
  loses. Pools and SPSA runs keep the win sound.
- A reserve already over its limit (`30s/5m/100/3m`) isn't cut to it
  any more: more time just isn't added until it falls below, as
  arimaa.com's match rules have it.
- Unused move time goes into the reserve rounded to the nearest second,
  as the match rules have it, or to the millisecond in a time control
  with fractions of a second.

### Fixed

- A test's panel could list a game twice when it finished while the
  panel was loading, which stopped the list from updating.

## [0.4.0] - 2026-10-24

### Added

- SPSA parameter tuning in the test daemon: a spec with `[spsa]` plays
  one engine against itself with its parameters (AEI options) nudged
  apart, and steps them by each iteration's results, with OpenBench's
  schedules and defaults. A run takes slots like a test, can be paused,
  resumed, stopped and archived, and picks up where it left off after a
  restart. `howdah-tourney show` and `list` print the iterations done
  and the current values, and `spsa-history.jsonl` in its directory has
  the values after every iteration, for plotting. How it works is in
  `docs/TOURNAMENTS.md`.
- An SPSA run's panel in the lobby: iterations, pace and time left, the
  two sides' score, a chart of the parameters' mean movement from their
  starts or of one parameter's path, and every parameter with where it
  stands in its range, its path so far and how far it has moved (in the
  spec's order, furthest moved first or by name, with a filter). Copy
  values copies the current values as `name = value` lines. The sidebar
  and the test cards show the iterations played.
- Time controls with fractions of a second (`0.5s/30s`, `1.5m/5m`,
  `0:02.25`), to the millisecond, for fast engine games and tests.
  Engines are sent the times as decimals then (Sharp and OpFor read
  them), and the game records keep move times to the millisecond
  (`[%emt 0:00:01.25]`). Whole-second time controls send and record
  times as before. arimaa.com games still take whole seconds only.
- Engine-only games no longer give setups arimaa.com's minute: with under
  a minute per move, a setup gets the plain move time, and time over it
  still comes out of the reserve. Games with a human player keep the
  minute. Engine tests can ask for it with `setup_minimum = "1m"` in the
  spec. Records of games without the minute say so with a
  `[SetupMinimum "0"]` tag, so their clocks replay correctly.

### Changed

- A test's panel lists its newest 100 finished games, with Show more.
- Engine matches send `stop` at most a quarter of a turn's time before
  its deadline, so a turn shorter than the usual one-second margin isn't
  stopped at once (the app's own matches already did this).

### Fixed

- The chat room and its archive work again: arimaa.com moved them from
  `srv.php` and `archsrv.php` to `srv.cgi` and `archsrv.cgi`, and the old
  addresses now refuse every request.
- A bare number after a unit in a time control takes the next smaller
  unit, as the arimaa.com gameroom reads it: `2m30` is two and a half
  minutes, not 32 minutes, and `1h5` is an hour and five minutes.
- On Windows, a test daemon started by the app no longer stops when the
  app is rebuilt and restarted under `tauri dev`: it now runs outside the
  job object that `cargo run` puts the app in.

## [0.3.0] - 2026-10-21

### Added

- Engine tests project how a running SPRT test will end: the chance it
  passes, fails or runs out its pair limit, and about how many games are
  left (with a range, and as a time at the test's recent pace). When a
  test is as likely to decide early as to run on, the test panel gives
  the games left for each. `howdah-tourney show` and `list` print the
  projection too.
- The chance an SPRT still decides within its pair limit, on the test
  panel and in the command line.
- A test with a pair limit can end undecided once its chance of
  deciding in time falls below a cutoff. New tests in the app stop
  undecided under 5% (a checkbox in the New test dialog). The cutoff
  can be changed while a test runs (`howdah-tourney undecided-below`);
  lowering or clearing it carries on a test that ended undecided.
- The sidebar's test rows show a flask whose fluid shows the SPRT's LLR
  by color and the share of the pair limit played by level, the games
  playing while a test is active, and score, Elo, LLR and games left,
  with the details in the row's tooltip.
- Shutting down the test daemon shows at once in the Engine tests
  panel, and a shutdown that lets running games finish can be called
  off (Cancel shutdown, `howdah-tourney shutdown --cancel`).

### Changed

- A test's pair limit, undecided cutoff, games at once and priority are
  changed in a Settings dialog, with Pause and Stop beside the engines.
- Engine test cards give the end estimate, show running games in their
  badge, and leave pairs out of the score line, which overflowed after
  about a thousand games.
- `howdah-tourney.exe` carries Windows version information, since
  Defender took the unsigned program without it for a trojan.

### Fixed

- Engines started by the installed app on Windows no longer each open a
  console window.
- Docked boards load on Windows; they stayed blank and couldn't be
  closed.
- A finished test game no longer shows as running, or can be followed
  as live, until its engines restart.

## [0.2.1] - 2026-10-17

### Added

- The chat room's message box previews the message as the room will
  show it (faces, piece symbols, links, `/me` and your color), and a
  button at its right end opens the faces and piece symbols to put in
  the message where the cursor is.
- A color of your own for the chat room: the color popup's last slot,
  typed in hex as on the room's page or chosen in the system's color
  dialog.

### Fixed

- The chat archive keeps days with nothing said between restarts,
  rather than asking arimaa.com for them again.

## [0.2.0] - 2026-10-17

### Added

- The chat room's archive, as a panel that needs no login: a calendar
  of the days kept here (messages and people each day, with part-kept
  days marked), and any day to read. Opening a day gets only what isn't
  kept yet from arimaa.com, and what you see in the chat room is kept
  as you go.
- The gameroom's chat room, as a panel in the arimaa.com lobby: enter
  and leave, who's there, the room's log with its faces, piece symbols,
  links, `/me` and colors, and a color of your own. Messages in white,
  the room's spoiler tag, are hidden until clicked, and the color
  popup's Spoiler sends the next message as one. Howdah leaves the room
  when you play a rated game against a person that isn't postal, or an
  event game (a setting turns this off).
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
- Home offers to download Sharp and OpFor, in one click, while no engine
  is installed.
- Puzzles is a lobby panel, with a filter and how many of each group are
  solved.
- In the lists of games and puzzles, a click anywhere on a row opens it.
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

[Unreleased]: https://github.com/Janzert/howdah/compare/v0.9.0...HEAD
[0.9.0]: https://github.com/Janzert/howdah/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/Janzert/howdah/compare/v0.7.1...v0.8.0
[0.7.1]: https://github.com/Janzert/howdah/compare/v0.7.0...v0.7.1
[0.7.0]: https://github.com/Janzert/howdah/compare/v0.6.1...v0.7.0
[0.6.1]: https://github.com/Janzert/howdah/compare/v0.6.0...v0.6.1
[0.6.0]: https://github.com/Janzert/howdah/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/Janzert/howdah/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/Janzert/howdah/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/Janzert/howdah/compare/v0.2.1...v0.3.0
[0.2.1]: https://github.com/Janzert/howdah/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/Janzert/howdah/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/Janzert/howdah/releases/tag/v0.1.0
