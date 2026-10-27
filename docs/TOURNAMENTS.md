# Engine tests and tournaments

Plan for a long-running runner that plays engine-vs-engine games,
mainly for bot development: answering "is this change stronger, and by
how much?" Howdah and a command line both connect to it to queue tests,
follow them and look at their games. Written 2026-10-11; phases 1 to 3
(the runner library and command line, the daemon, and the lobby's Engine
tests) are built.

## What others do

- **cutechess-cli** (chess): one run from the command line plays a
  round robin, gauntlet or knockout at a set concurrency, with an
  opening book (`-repeat` plays each opening twice with colours
  swapped), draw and resign adjudication, SPRT, and PGN output.
  `-recover` restarts crashed engines. A run can't be watched from
  outside or resumed; the Cute Chess GUI is a separate program, with a
  game wall of running games.
- **fastchess** (chess): the same command-line style with lower
  overhead and much higher concurrency. It counts results by pentanomial
  statistics (a colour-swapped pair is one unit) and can pause, save its
  state and resume. It's now the usual tool for local SPRT tests.
- **fishtest** (Stockfish) and **OpenBench** (many engines): distributed
  testing. A test is a dev branch against a base branch; workers build
  both, take chunks of game pairs, and report back. A web page shows each
  test's log-likelihood ratio (LLR), Elo with error bars and pair counts,
  and a queue of tests with priorities sharing the workers. OpenBench
  measures each machine with a "bench" run and scales time controls.
- **gomill's ringmaster** (Go): a control file defines a competition and
  its output goes next to it. Games run in parallel; a stopped
  competition continues where it left off; `show` and `report` read its
  state from another process while it runs. Besides tournaments it runs
  parameter tuning (Monte Carlo and cross-entropy). Every game is kept
  as SGF.
- **AEI's `roundrobin.py`** (Arimaa): a config file, one game at a time,
  records out.
- **BayesElo** and **Ordo**: ratings for a pool of players from all
  their results.

Patterns worth taking:
- A queue of tests sharing the machine (fishtest, OpenBench).
- Colour-swapped pairs as the unit of work and of counting.
- State that survives a crash or a stop, and that another process can
  read while games run (ringmaster, fastchess).
- SPRT as the normal way to end a test, with Elo and its error alongside.
- A wall of running games (Cute Chess), later.
- Tuning, much later.

## How Arimaa differs from chess and Go

From Brian (2026-10-11):
- **No draws.** Under modern rules a draw can't happen, so every game is
  a win or a loss.
- **Games diverge fast.** With Arimaa's branching factor, bot games have
  historically gone their own way within a few moves, and no opening
  book or fixed setups have been used. A setup book may be wanted one
  day, but it isn't needed to start.
- **No colour advantage** has been shown: silver's second setup seems to
  cancel gold's first move.
- **Colour-paired games are still wanted**, without fixed setups.

## What Howdah has

- `howdah_aei::play_match` plays one game between two engines that have
  done the handshake, with clocks, following pyrimaa's `game.py`. Every
  move is checked by `howdah-arimaa`; an illegal move, a crash, a timeout
  or a resignation loses, and a turn limit is scored by
  `limit_score_winner`. It reports `MatchEvent`s and leaves the engines
  running, so they can be reused.
- `examples/match.rs` plays one game from the command line.
- The app plays engine-vs-engine games in a game window (`start_match`).
- Engines: the app's engine list (`engines.json`) and installed engines
  in versioned directories (`<app local data>/engines/<id>/<version>/`).
  `Profile` reads each engine's scores from its side.
- The dev bridge already serves `Backend` over local HTTP
  (`POST /invoke/<cmd>`, SSE `GET /events`), a pattern the runner's API
  can follow.

## Proposal

### Pieces

- **`crates/howdah-tourney`**: the runner as a library (tests, the
  scheduler, statistics, the results log), with no UI dependencies. It
  builds on `howdah-aei`.
- **A daemon**, one per user, running a queue of tests. It keeps running
  when Howdah closes. It can be started from a terminal or by Howdah.
- **A command line** that talks to the daemon: submit a test, list
  tests, show one (stats and recent games), pause, resume, stop, change
  concurrency. A foreground mode runs a single test without a daemon,
  using the same library, for scripts and CI.
- **Howdah** connects to the same daemon (below).

### Tests

A test is a spec plus its results. The first kind is two engines in
colour-swapped pairs, ended by SPRT or a game count. Gauntlets (one
engine against several) and round robins come later as more kinds.

The spec, in `test.toml`:
- Each engine: a name, the program and its arguments, AEI options,
  and whether to restart it for every game (default: reuse it, sending
  `newgame`).
- The time control (the AEI `M/R/P/L/G/T` format), including the turn
  limit.
- The stopping rule: SPRT bounds (`elo0`, `elo1`, `alpha`, `beta`), a
  maximum number of pairs, or both. With both, `undecided_below` (the
  app's new tests use 0.05) ends the test undecided once the SPRT's
  chance of deciding within the limit falls below it.
- Its priority in the queue.

**Engine snapshots.** When a test is submitted, each engine's program is
copied into the test's directory and run from there, and its SHA-256 is
recorded. Rebuilding a bot while a test runs then doesn't change the
test, and the result always says exactly which binaries were tested. An
installed engine (a versioned directory that doesn't change) can be
recorded instead of copied. An engine needing files beside its program
copies its directory.

### Storage

One directory per test, under the daemon's data directory
(`<app local data>/tourney/tests/<id>/`):
- `test.toml`: the spec, as submitted. Changes later (more pairs, a new
  priority) are appended to an events file rather than rewriting it.
- `engines/`: the snapshots.
- `results.jsonl`: one line per finished game, appended when it ends:
  pair number, colours, engines, result and reason, plies, time used,
  the record's file name, and a hash of the move list.
- `games/`: a record per game (`howdah-arimaa`'s record format, with
  engine names, time control and `Event` set to the test).
- `logs/`: engine logs per game, when the test asks for them.

A test that isn't active and has no games running can be archived:
its directory is zipped into `<data dir>/archive/<id>.zip` and taken
out of the queue, or deleted outright. Archived tests can be listed
(read from their zips), restored (unzipped back into `tests/` and into
the queue as they were, and the zip removed) or deleted. Archived
tests' numbers aren't given to new tests.

The state of a test is its spec plus `results.jsonl`, so resuming after
a crash or a stop is just reading them: pairs with both games logged are
done, and anything else is played again. Nothing else holds state that
needs saving. Howdah can read a test's directory to show it even with no
daemon running.

### Scheduling

- The unit of work is a **pair**: one game each way round. Pairs are
  numbered, and game 1 of pair n has the first engine as gold.
- The daemon has a number of **game slots** (concurrency). Free slots go
  to running tests by priority, and evenly among tests with the same
  priority.
- A slot keeps its engine processes while it plays the same test's
  games, and quits them when it moves to another test or the test ends.
- An engine that crashes loses that game (as `play_match` does already)
  and is restarted. If an engine keeps failing (say three games in a
  row), the test is paused with the reason shown, rather than piling up
  losses.
- When a test's SPRT decides, it reaches its pair limit, or it ends
  undecided, games already running finish and are counted, and no more
  start.

### Statistics

Every game is a win or a loss, so the statistics are simpler than in
chess.
- **Counted by pair.** A pair scores 0, ½ or 1 for the first engine (the
  mean of its two games). With fast divergence and no colour advantage,
  the two games of a pair are close to independent, and counting by
  pair gives nearly the same answer as counting by game. It stays right
  if they aren't, e.g. two bots that set up the same way every game,
  and it costs at most half a pair.
- **Elo** is logistic Elo from the mean pair score, with a 95% interval
  from the variance of the pair scores, plus the likelihood of
  superiority (LOS).
- **SPRT** is the generalized SPRT used by fishtest and fastchess, on
  pair scores: with `s0` and `s1` the expected scores at `elo0` and
  `elo1`, `n` pairs, mean `m` and variance `v`,
  `LLR ≈ n (s1 − s0) (2m − s0 − s1) / (2v)`, stopping when it crosses
  `ln(β / (1 − α))` or `ln((1 − β) / α)`. Bounds are in plain logistic
  Elo; there are no draws to normalize away.
- **Repeated games.** Varied games depend on the engines not being
  deterministic, which time-based search usually gives. The runner
  compares the move-list hashes in `results.jsonl` and flags a test with
  repeated games, since repeats make the error bars look smaller than
  they are. It's also the evidence for whether a setup book is ever
  worth adding.
- Also shown: results by reason (goal, elimination, immobilization,
  timeout, illegal move, crash, turn limit), by colour, and average game
  length and time used, so a broken engine stands out.

### The daemon's API

Local HTTP with JSON and server-sent events, as the dev bridge does:
- Commands: submit, list, get a test (spec, state, stats), list a test's
  games, pause, resume, stop, change pairs or priority, set
  concurrency, archive or delete a test, list, restore or delete
  archived tests, shut down.
- Events: a test added or changed state, stats updated, a game started
  or finished. A running game's `MatchEvent`s (moves, clocks, engine
  info) are sent only to clients following that game.
- It listens on 127.0.0.1 and writes its port and a random token to a
  file readable only by the user, in its data directory; clients send
  the token, so other users on the machine can't drive it. Howdah and
  the command line find it through that file.

### In Howdah

- A **Tests** page in the lobby window: the queue and finished tests,
  each with its engines, games played, Elo with its interval and an LLR
  bar between the SPRT bounds. Buttons to pause, resume, stop and add a
  test (picking engines from the engine list, which the daemon then
  snapshots), and to archive every finished test at once.
- A test's dialog archives or deletes it (delete after a second click)
  once it's stopped or paused with no games running.
- An **Archived tests** dialog from the Tests page lists the archive
  (each test's results as archived, when, and the zip's size), with
  Restore and Delete (after a second click) for each.
- A **test page**: the stats above, the list of games (open one in a
  game window to replay it), and the running games.
- A **running game** opens in a game window as a read-only session that
  follows its `MatchEvent`s, as a followed arimaa.com game does. The
  game wall from `WINDOWS.md` phase 4 shows them all at once.
- If no daemon is running, Howdah offers to start one, and can still
  show finished tests from their directories.

## Phases

1. **The runner library and a foreground command.** `howdah-tourney`
   with a two-engine SPRT test: the spec, snapshots, pairs, concurrency,
   engine reuse and restart, `results.jsonl` and records, resuming, the
   statistics and the repeated-games check. A command runs one test in
   the foreground and prints progress. Tested with `aei-test-engine` and
   checked with Sharp and OpFor.
2. **The daemon.** The queue, priorities and shared slots, the HTTP and
   SSE API, the token file, and the command line as its client.
3. **Howdah.** The Tests page, the test page, and following running
   games in game windows.
4. **More kinds of test.** Gauntlets, round robins with a crosstable,
   and ratings for a pool (BayesElo or Ordo style); then the game wall.
   Pools are built (see "Pools" below); they cover round robins too.

**As built (phase 1)**, where it differs from or adds to the plan:
- `crates/howdah-tourney`, with the command `howdah-tourney`:
  `new SPEC.toml DIR` makes a test's directory (snapshotting the
  engines), `run DIR [-j N]` plays its games, resuming from its log, and
  `show DIR` prints its results. The spec's format is in `src/spec.rs`.
  Engine 1 is the one being tested; scores, Elo and the SPRT are from its
  side. A turn limit is the time control's `Nt` game field
  (`1s/10s/100/0/60t`).
- Games, not pairs, are handed out, so two slots may play both games of
  a pair at once. Only complete pairs count in the statistics.
- Every engine is copied, installed or not.
- An engine that forfeits (crashes, stops answering) or loses on time
  (it may still be searching) is restarted. A failure before the first
  move restarts both engines and replays the game, since neither can be
  blamed. Three failures in a row by one engine (forfeits or failed
  starts) halt the test.
- **SPRT details.** The variance in the LLR has one pair of each outcome
  added; the mean is the real one. In a real run, one won pair made the
  sample variance zero and the LLR 11, a pass after one pair. The test
  decides at the first pair that takes the LLR past a bound, in log
  order, so games still running when it decides are counted in the
  results but can't undo it, and a resumed test stays decided.
- **Ending undecided.** `decide_chance` projects the rest of the test:
  the LLR as a Brownian motion from where it is, with the drift and
  variance of a true score drawn from a normal posterior (the observed
  score, variance over `n`), and the first-passage chance of each bound
  within the pairs left, added. Checked after every game against
  `undecided_below`: since that chance is a martingale, stopping when it
  falls below 5% means at most about 5% of tests stopped this way would
  have gone on to decide, however often it's checked (averaged over the
  flat prior; simulating [0, 10] over 6000 games, at any given true
  Elo it changed at most about 1.6% of results).
  The parent repo's SPRT cutoff chart simulates it. The model overstates
  the chance over the first few pairs, which only errs towards playing
  on. The cutoff is a setting beside the pair limit, changeable while
  the test runs (off: play on to the limit). A test ended undecided
  can't be resumed until a raised pair limit or a lowered cutoff gives
  it room to decide, and that change carries it on; one that raises the
  cutoff past a running test's chance ends it.
- **When a test will end.** `project` (`TestInfo.projection`) simulates
  the rest of an unfinished SPRT test 500 times, pair by pair, with the
  runner's own endings: the first LLR crossing, the pair limit, and the
  undecided cutoff (checked about 40 times a path while the chance is
  well above the cutoff, every pair once it's within three times it,
  with bisection back to the first pair under it). It gives the chance
  of each ending and the games left at the 10th, 50th and 90th
  percentiles, which the test page and the test cards show as "Ends in
  about 1800 games (930–3600)", and the sidebar's test rows as the
  median alone. Each path draws its true Elo from the posterior given the
  pairs so far, with a prior of 80% N(0, 15) and 20% N(0, 80) Elo:
  most changes are small, a few are far out. With a flat prior the early
  projections expected a quick pass or fail far too often (after 50
  pairs, 44% predicted passes against 19% real ones in simulated tests
  near [0, 10]). Over tests drawn from the prior, projected after 50,
  250 and 700 pairs, the 80% range held 77% of 769 real endings, 48%
  ended before the median, and pass, fail and undecided were predicted
  21%, 36% and 43% against 22%, 34% and 44%
  (`project_is_calibrated_at_length`, ignored by default). How good the
  early projections are depends on how well the prior fits the changes
  being tested; after a few hundred pairs the results outweigh it.
  Test lengths fall into two groups, a decision along the way or running
  on to near the end, so the overall median often sits between them.
  `project` also gives the percentiles among the paths that pass or fail
  and among those that don't, and the test page shows the two as rows
  ("if it decides: …", "if it doesn't: …") when each is at least 15%
  likely under a pair limit. Replayed on eleven real tests at even
  spacing, each row held the real ending in its 80% range 79% (deciding)
  and 100% (not deciding, mostly runs to the limit) of the times it would
  have shown; over 150 tests drawn from the prior with the 5% cutoff,
  83% and 79%, each with its median at the real length (×1.01, ×0.98).
  (Checkpoints for such checks go at even spacing: spacing them by each
  test's real length packs short tests' checkpoints closer, and makes a
  calibrated projection look long.) A
  projection takes tens of milliseconds, so the daemon keeps each
  test's last one and works it out again only when the pair counts,
  limit or cutoff change, outside the queue's lock.
- **Ctrl-C.** The first stops new games and waits for the running ones;
  the second quits, and those games are played again on the next run.
  Engines run in their own process group (`EngineConfig::own_process_group`),
  since a terminal sends Ctrl-C to the whole foreground group: before
  that, the engines died with the first one and their games were scored
  as forfeits.
- Records carry `Event` (the test's name), `Round` (`pair.game`),
  `Gold`, `Silver`, `TimeControl` and, when the game didn't end on the
  board, `Termination`; move times are `%emt`, in whole seconds.
- A last line of `results.jsonl` cut short by a kill is dropped when the
  log is next opened.
- Tested with `aei-test-engine` (pairs, records, resuming, an engine
  crashing until the test halts, the SPRT ending a one-sided test).
  Checked with Sharp against OpFor (4 pairs, 4 games at once, with
  engine logs) and Sharp against itself, stopped with a group-wide
  SIGINT and resumed.

**As built (phase 2):**
- The queue is `Daemon` in the library, with no HTTP in it; `server`
  (axum) and `client` (reqwest) are modules behind features of the same
  names, both on by default, so the app can take the client alone.
- The data directory is `$HOWDAH_TOURNEY_DIR`, else `tourney/` in the
  app's local data directory (beside `engines/`), or `--data-dir`.
  Tests are `tests/<NNNN>-<name>/`. Instead of an events file, each
  test's changes (status, priority, pair limit, games at once and the
  undecided cutoff when changed from the spec's, end) go in `state.json`
  beside `test.toml`, rewritten whole (written to a temporary file and
  renamed).
- Statuses are active, paused and finished, a finished test saying why:
  passed or failed (the SPRT), done (all pairs played), undecided
  (unlikely to decide within the pair limit), stopped or halted. Resuming clears a halt and its failure counts; a decided test
  can't be resumed, and one that has played all its pairs is resumed by
  raising its pair limit.
- A test's `concurrency` caps its games at once in the daemon too. A
  free slot takes the active test with the highest priority, then the
  one it holds engines for, then the one with the fewest games running,
  then the oldest; it quits its engines when it moves to another test.
  The daemon starts with half the cores as slots unless told
  (`--slots`).
- Submitting needs absolute program paths; the command line resolves a
  spec's paths from the spec file's directory before sending it.
- The API is in `src/server.rs`'s doc comment. The port is random
  unless `--port` sets one. `daemon.json` (port, token, pid) is mode 600
  on Unix; on Windows it relies on the local app data directory being
  the user's own. A second daemon on the same directory is refused while
  the first answers. Clients that fall more than 1024 events behind miss
  events (they can list the tests again).
- Shutting down waits for running games; `--now` (or a second Ctrl-C)
  abandons them, killing their engines, and they're played again when a
  daemon next starts. Event streams close when the daemon is done.
- Command line: `daemon`, `submit`, `list`, `status`, `watch [ID]`,
  `pause`, `resume`, `stop`, `priority`, `max-pairs`, `slots`,
  `shutdown [--now]`, and `show` takes a test's id as well as a
  directory.
- Not built in phase 2: a running game's moves over the API and Howdah
  starting a daemon (both came with phase 3), and removing a test.
- Tested with `aei-test-engine` (priorities, pausing across a restart,
  resuming a halted test, extending a finished one, refusals, and the
  HTTP API with its token). Checked on Linux with Sharp and OpFor: two
  tests queued with priorities, pause and resume, `shutdown --now`
  leaving no engines running, a restart keeping a paused test paused
  and carrying on the active one, a raised pair limit, and a graceful
  shutdown.

**As built (phase 3):**
- The daemon reports a running game's moves (`movePlayed` events, with
  the time each took and both reserves after it) and serves a snapshot
  for a viewer joining mid-game (`GET /tests/{id}/live/{pair}/{game}`:
  the moves so far, the reserves and the time spent on the turn being
  played).
- The app is a client (`app/src-tauri/src/tourney.rs`, using
  `howdah-tourney` with only its `client` feature). The lobby's
  **Engine tests** section, below Open windows rather than on a page of
  its own, shows the daemon's slots (with − and +), its tests (name,
  engines, score, Elo, and the LLR as a bar between its bounds, with
  the state) and Shut down. A click on a test opens its dialog: results,
  Pause, Resume, Stop, priority and pair limit, the games playing now
  (Watch) and the finished ones, newest first (Open). New test picks two
  engines from the engine list (one tested against itself gets "(1)"
  and "(2)" after its name), a time control with a turn limit, the SPRT
  bounds and/or a pair limit, games at once and priority. Installed
  engines are copied with their directory.
- The daemon's events reach the lobby as `tourney://changed`. The first
  `tourney_status` starts a task that connects to the daemon, passes
  its events on, and looks for a daemon again every 3 seconds once it's
  gone.
- A test's game opens in a game window. A finished one loads as its
  record. A running one is followed as a watched arimaa.com game is: a
  match between two remote players (the engines' names), with the moves
  and clocks from the daemon (its own event stream, opened before the
  snapshot is read so no move falls between), and the result from the
  daemon when the game ends. It stops following when the window closes
  or shows another game. The session remembers the test game it shows
  (`SessionView.testGame`), and the window's title names it after the
  players (`SlowA - SlowB · <test> 12.1`).
- A running test game shows both engines' panels, as a local engine
  game does (Brian, 2026-10-13). The runner passes `play_match`'s `Info`
  and `Log` events on as `RunEvent::EngineOutput` and keeps each side's
  last 150 lines in the game's live state; the daemon sends them on a
  broadcast of their own (`GameOutput`), served per game at `GET
  /tests/{id}/output/{pair}/{game}` (`Client::output`), so the general
  event stream doesn't carry every running game's output. `LiveGame`
  has the kept lines and the engines' `id name`s, from which the app
  picks each one's `Profile` to read its output. The game files don't
  keep engine output, so a finished game opened from the list has no
  panels. The snapshot's move times (`usedMs`, all of them) now go on
  the moves already played, as a finished game's record has them.
  Checked in the browser preview with Sharp against OpFor: a window
  opened mid-game showed both panels at once (from the session's kept
  output), then live, and moving the session on to the pair's second
  game showed only that game's output, sides swapped.
- The test dialog's Follow (by Playing now) opens a window that follows
  the test (Brian, 2026-10-13): when its game ends, the game-end dialog
  counts down 15 s and then opens the test's most recently started game
  that no other window shows, in the same window, or waits for one to
  start. Choosing anything in the dialog (Review game, Analyse) stops
  following; Next game skips the countdown. The backend picks the game
  (`follow_tourney_test`) under a lock, so several windows following one
  test end up on different games even when they look at once. Checked in
  the browser preview with two following windows on a test of slowed
  random movers (`aei-test-engine --think-ms 700`): they took different
  games, moved on after the countdown, Review game stayed put, and a
  window waiting while the test was paused took the newest game when it
  resumed.
- Without a daemon, the section offers to start one when
  `howdah-tourney` sits beside the app's program, with its output in
  `daemon.log` in the data directory; otherwise it gives the command.
  Installers bundle it there as a Tauri sidecar (`npm run sidecar`, then
  `tauri build --config src-tauri/tauri.sidecar.conf.json`, as the
  release workflow does); in development `cargo build -p howdah-tourney`
  (or `npm run bridge`) puts it there. An AppImage's files go away when
  the app exits, and on Windows a running daemon would block replacing
  its program in an update, so there the app runs a copy kept in the
  data directory's `bin/`, refreshed when the app's is newer. The dev bridge uses its own
  data directory (`dev-bridge-config/tourney`), so a daemon started from
  the browser preview is separate from the user's.
- Not built yet: showing tests from their directories with no daemon,
  and the game wall (phase 4).
- Checked in the browser preview against the dev bridge: starting the
  daemon, a Sharp vs OpFor test from New test, its dialog with the games
  playing, following a running game to its end (moves, clocks and the
  result matching the daemon's record), opening a finished game, and the
  list updating as a test is added and finishes. The browser preview
  opens a game window in the lobby's own tab with a new session, so the
  followed game was opened on a session made through the bridge.

**As built (engine processes and memory, 2026-10-11):** after test 0003
(OpFor against OpFor, 2 games at a time on a daemon with 4 slots) had 8
OpFor processes holding about 25 GB after 10 hours, and the machine ran
out of memory. 4 of the 8 were idle: a slot kept its engines until it
moved to another test, so every slot that had played a game still held
two engines while only two games ran. The rest is protection that works
for any engine.
- **Idle slots let their engines go.** A slot that has had nothing to
  play for 10 s (`IDLE_KEEP`) quits its engines, so the engines running
  are those of the games being played, plus a slot's for a few seconds
  between games. A slot that finishes a game usually takes the next one
  itself and keeps its engines.
- **Quitting kills the whole engine** (`howdah-aei`, `Engine::quit`):
  `quit`, up to the grace period, then SIGKILL to its process group
  (the tourney starts every engine in its own group), and the group is
  killed after a clean exit too, so nothing the engine started outlives
  it. Dropping an `Engine` kills its group as well, and `Engine::kill`
  does it at once. On Windows every engine runs in a job object that
  kills it and whatever it started when the job is closed or terminated
  (also when our process ends); the engine joins it right after it
  starts, so a child started in that instant could escape it.
- **Memory is read** from the OS (`howdah_aei::process`: `/proc/<pid>/statm`
  on Linux, the working set on Windows, `proc_pidinfo` on macOS): only the
  engine's own process, not its children. Each running game's engines
  are read every 2 s, and each engine after each game. Nothing reads
  idle engines; they keep their last reading.
- **Restarts between games.** Never mid-game: after a game, an engine is
  restarted if its resident memory is more than `growth` times what it
  was after its first game (default 3, and only once it's past 256 MB),
  or past `restart_mb` (none by default). These are in the test's
  `[memory]` table in `test.toml`; with none, the defaults apply, so
  older tests get them on resume.
- **The hard limit.** An engine past `limit_mb` (default a quarter of
  the machine's memory) in a game forfeits it at once: the game is
  recorded with reason `resourceLimit` (and the record's `Termination`,
  and the log's detail, saying which engine used how much against what
  limit), and both engines are restarted, the other one since it may be
  mid-search. It counts in the statistics as the loss it is, and as a
  failure towards halting the test (three in a row halt it), but it's
  counted apart under endings, so a leak shows instead of passing for
  weak play.
- **The budget.** All the engines a daemon (or a `run`) holds share a
  budget, half the machine's memory unless `daemon --memory-budget MB`
  says otherwise. A slot starts no game while the engines hold 90% of
  it, quitting its own idle engines first and checking again every 2 s.
  When the engines together pass the budget mid-game, the largest one
  forfeits its game as above (the reason names the budget). `status`
  shows the engines' memory against the budget.
- **The kernel's cap**, as a backstop for the watchdog, which only reads
  every 2 s: each engine starts with a cap a quarter (and at least
  256 MB) above its test's limit, so the watchdog normally ends the game
  first and says why. On Linux the engine is started through
  `systemd-run --user --scope -p MemoryMax=… -p MemorySwapMax=0`, when
  that works (tried once per process); `systemd-run` execs the engine, so
  its pid, process group and parent-death signal stay as they were. On
  Windows it's the job object's per-process memory limit, which counts
  committed memory rather than the working set. macOS has none. An
  engine the kernel kills shows up as a crash (a forfeit), not as a
  `resourceLimit`.
  - Why per engine, and not the daemon in one scope or `setrlimit`:
    `RLIMIT_AS` limits address space, not memory, and engines (the D
    runtime, big hash tables) reserve far more than they touch, while
    Linux ignores `RLIMIT_RSS`. A single scope for the daemon would need
    the app to start it through `systemd-run` (or the daemon to re-exec
    itself) and would let the kernel pick the victim, possibly the
    daemon. A scope per engine kills only the engine that grew, and the
    budget above caps the total.
- **Results.** Each game's `results.jsonl` entry has `memoryMb` (both
  engines' resident memory as it ended) and `restarts` (engine and
  reason, numbers included); `daemon.log` has the memory at the end of
  each game's line (`(one/two MB)`, by engine) and a "restarting"
  line with the reason for each restart, and at startup the budget and
  how engines are capped.
- Tested with `aei-test-engine`'s `--grow-mb` (and `--think-ms`, so a
  game lasts past a reading): a restart after the second game and not
  the first, a forfeit at the limit, the largest engine forfeiting past
  the budget, and the kernel's cap killing an engine; with
  `--ignore-quit` and `--child-pid-file`: `quit`, `kill` and dropping
  each killing the engine and its child; and idle slots ending with no
  engines. The Windows and macOS code is only compiled (in CI, and
  checked with `cargo check --target`), not run.
- **The app's own engines** (matches and analysis, `controller.rs`)
  don't use the watchdog: they're one or two engines the user is
  watching, and they aren't started in a group of their own (so
  `quit` kills only the process there). Their Windows job (kill on
  close) fixes engines outliving the app there. If a leaking engine
  turns up in long analysis sessions, the same pieces apply: start them
  with `own_process_group` and a `memory_limit`, and read
  `Engine::resident_memory` between searches.

Later, if wanted: eval-based adjudication (resigning for an engine past
a score threshold for some turns, using `Profile`'s scores), a setup
book, tests naming git commits that the daemon builds, and remote workers
taking pairs from the daemon.

### SPSA tuning

A spec with `[spsa]` is a tuning run instead of a test (format in
`spec.rs`, the method in `spsa.rs`). It has one engine, which plays both
sides of every pair: engine 1 (`<name>+`) at θ + c·Δ and engine 2
(`<name>-`) at θ − c·Δ, where θ is the parameters (AEI options, each with
a start, bounds, `c_end` and optionally `r_end` and `int`) and Δ a random
±1 per parameter, drawn per iteration. After an iteration's
`pairs_per_iteration` pairs (default 8), θ moves by r·c·(W − L)·Δ per
parameter, W and L engine 1's wins and losses there, and is clamped to
the bounds. The schedules and defaults follow OpenBench's code
(`spsa_utils.py`; its wiki page, "SPSA Tuning Workloads", gives only the
names): with N iterations, A = `a_ratio`·N (0.1), α 0.602 and γ 0.101,
iteration k (from 1) uses c_k = c_end·(N/k)^γ (at least 0.5 for an
integer) and r_k = a_k/c_k², a_k = r_end·c_end²·((A + N)/(A + k))^α, so
the last iteration steps by `c_end` and `r_end` (default 0.002). As in
OpenBench, θ stays real and an integer is rounded only for the engine,
at random (`floor(x + U)`), so a step under 1 still moves it on
average; its c is at least 0.5 so the two sides differ.

- **In the queue** it's a test like any other: it takes slots by
  priority, its `concurrency` caps it, and it can be paused, resumed,
  stopped and archived. Its pair limit is `iterations ×
  pairs_per_iteration` (`max-pairs` is refused: N is in the schedule),
  and it ends as `maxPairs` once every iteration is played. `TestInfo.spsa`
  (`SpsaInfo`) has the iterations done and θ, which `list` and `show`
  print.
- **Options per game.** Slots keep engines between games as usual; the
  game's parameter values are sent with `setoption` before each game,
  after the spec's options. An engine has to take them between games
  (OpFor's `tune_*` and `eval_*` do). Its hash and anything else it
  keeps between games carry over from earlier values; `restart_each_game`
  avoids that at the cost of a start per game.
- **State.** An iteration's point (θ as it opened, Δ, c, r and the
  values each side gets) is drawn when its first game is handed out and
  appended to `spsa-points.jsonl`; when its last game is logged, its step
  (W, L, θ after it by name, the time) goes to `spsa-history.jsonl`, the
  file to plot from. A restart reads θ from the history's last line and
  steps any iteration whose games were all logged before its step was
  written. Points come from a seeded generator (`seed`, picked when the
  test is made if the spec has none), so a point drawn again is the same.
- **Overlap.** The next iteration opens as soon as every game of the
  open ones is handed out, from θ as it stands, rather than after their
  last game ends, so slots don't sit idle at each iteration's end. With
  `concurrency` up to `2 × pairs_per_iteration` that's at most one
  iteration behind, which OpenBench's workers (each taking a point from
  the θ of the moment) accept too.
- **In the app**, a run's test panel shows `SpsaPanel` in place of the
  SPRT's results (`TestView.spsa`, with each parameter's `c` for the
  next iteration): progress and pace, the mean movement of θ from its
  start as a share of each range (or one parameter's path, with its
  start), and the parameters with a range bar (start, θ, θ ± c), a
  sparkline and the move. The history comes from `GET
  /tests/{id}/spsa-history?skip=N`, fetched whole once and then only
  the steps after those it has, as `TestInfo.spsa.done` rises. "Moved
  more than c" counts parameters further from their start than the
  next perturbation, a rough line between a trend and noise.

### Pools

A spec with `[pool]` is a standing tournament instead of a test (format
in `spec.rs`, the method in `pool.rs`): two or more engines play each
other in colour-swapped pairs for as long as it runs, engines can be
added or retired meanwhile, and it shows ratings, a crosstable and the
matchups whose results are out of line with the ratings. It's for
continual testing across versions: add each new build, and see where it
lands against every earlier one. It has no SPRT; `max_pairs` is
optional, and without one it plays until stopped.

- **Ratings.** The Bradley–Terry model (logistic Elo; no draws to
  model) fitted to every game at once, as BayesElo and Ordo do: the most
  likely ratings under a normal prior of SD 600 Elo on each, found by
  Newton's method with step halving, with their covariance from the
  inverse Hessian. The prior keeps an engine with only wins or only
  losses finite and costs almost nothing once there are games. Ratings
  are relative to the anchor (`anchor`, else the first engine with
  games), and each one's 95% interval is that of its difference from the
  anchor, from the covariance (the anchor's is 0). Games count one by
  one; the pairs' near independence (see Statistics) makes that fine.
- **Scheduling** (`choose`), when a pair's first game is handed out:
  coverage first, then information. While some matchup of active
  engines has fewer than `min_pairs` pairs (default 2, pairs in play
  counting), one of the least played is drawn at random; a new engine
  starts at none against everyone, so it catches up before anything
  else. After that each matchup is weighted by how much one more pair
  would shrink the variance of its rating difference, `v² I / (1 + v I)`
  (`v` that variance, `I = 2 p (1 − p)` a pair's Fisher information),
  with the games in play added to the Hessian as if played, so several
  slots picking at once don't pile onto one matchup, times the
  matchup's relevance: the larger of how contested it is, `4 p (1 − p)`,
  and the chance its order is still open, `2 Φ(−|d| / √v)`. Without that,
  the first pool on the Windows machine (three OpFor releases and Sharp,
  2547 games) gave Sharp 57% of its games though it won 98% of them: a
  lopsided matchup's difference stays the least known because each game
  tells little, so pure information kept choosing it, to measure a gap
  whose order was never in doubt. With it, the same tally gives Sharp's
  matchups 15% of the weight, mostly the floor, while close engines stay
  at full weight however well measured and a new engine far from the
  rest gets full weight until its place is clear. Every weight gets
  10% of the mean added (so distant matchups still get the odd pair and
  the crosstable fills in), and a matchup whose result is 2 or more
  standard deviations out of line but not yet an outlier gets the mean
  again. The matchup is drawn in proportion to its weight, from a
  generator seeded by the spec's `seed` and the pair number, and which
  engine is engine 1 is drawn too. "The least confident outcome" (a
  predicted score nearest 50%) was the first idea; it isn't the same
  thing, since two engines rated alike after 500 games have nothing left
  to tell.
- **Outliers.** A matchup with at least 6 games is out of line when
  `z = (wins − n p) / √(n p (1 − p))` is `outlier_z` (default 3) or
  more either way, `p` from the ratings. 3 rather than 2 because a pool
  of 20 engines has 190 matchups, and at 2 about ten would show by
  chance.
- **State.** `results.jsonl` entries carry `engines`, the pair's
  engines 1 and 2 as indexes into the spec's engines (`winner`, `used`
  and the rest stay by side), so a restart knows every pair's matchup
  from its log; a pair with no game logged is drawn again. Engines are
  only ever appended, so the indexes hold. Adding or retiring one
  rewrites `test.toml` whole (a temporary file, renamed), with the new
  engine's snapshot in `engines/` as at submission and `retired = true`
  on a retired one.
- **Retiring.** A retired engine plays no new pairs; its games still
  count in the ratings. A pair drawn before its engine was retired is
  drawn again if neither game has been played, and its other game is
  dropped if one has. An engine failing three games in a row is retired
  (`engineRetired`) rather than halting the pool, while at least two
  others can play; otherwise the pool halts as a test does.
- **Slots.** A slot keeps its engine processes for the next pair when it
  has the same engines (either way round) and swaps the rest, so with
  many engines most pairs start fresh processes.
- **API and command line.** `POST /tests/{id}/engines` `{"engine":
  EngineSpec}` and `POST /tests/{id}/retire` `{"engine": name,
  "retired": bool}`; `howdah-tourney add-engine ID NAME PROGRAM [ARG...]`
  (or `ENGINE.toml`, one `[[engines]]` table), `retire` and `unretire`.
  `show`, `run` and `list` print the ratings, the crosstable (rows
  against columns, both in rank order, `*` marking outliers) and the
  outliers in words. `TestInfo.pool` is `PoolInfo`, and
  `TestInfo.running_engines` names each running pair's engines.
- **In the app**, New test has Two engines or Pool (two or more engines
  from the engine list, with no SPRT), and a pool's panel shows
  `PoolPanel` in place of the SPRT's results: the ratings in rank order
  with their intervals drawn as a dot and whisker on a shared scale,
  Retire or Bring back for each, Add an engine from the engine list
  (named apart from the pool's, `Sharp (2)`, if it has to be), the
  crosstable with each cell shaded by `z` (the accent color better than
  predicted, warn worse, up to 55% at `outlier_z`, nothing within half a
  standard deviation, the numbers always shown and the tooltip giving
  the prediction), and the outliers.

## Decided

Brian, 2026-10-11:
- One daemon running a queue of tests, rather than a process per test.
- Engine processes are reused between games, with an option to restart
  them for every game.
- Two-engine SPRT tests come first.
- No setup book to start with, and maybe never.

Brian, 2026-10-26:
- A pool (standing tournament) for continual testing between versions,
  with ratings, a crosstable and outliers, taking new engines at any
  time, scheduled by coverage then information (above).

## Open questions

- How a test names its engines: a program path that's snapshotted (the
  proposal above), or a git commit the daemon builds, as fishtest and
  OpenBench do. Commits are nicer for development but need a build
  recipe per bot. Paths first; commits could come later.
- Default SPRT bounds and time controls, once real tests show how many
  games a test takes.
- Whether the command line is its own binary or a subcommand of a
  future `howdah` CLI.
- Whether the daemon should exit when its queue is empty, or stay until
  told to stop.
