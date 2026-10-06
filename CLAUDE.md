# Howdah: notes for Claude

Cross-platform Arimaa client. It's a Tauri 2 spike, but it's structured the
way the real project will be. Long term it covers gameroom play, AEI bots,
analysis, tournaments, postal games and bot tooling.

Design notes live in `docs/`. `docs/UI-SURVEY.md` collects UI and feature
ideas from other Arimaa and chess clients, and `docs/ENGINES.md` records how
Sharp, OpFor and other AEI engines behave.

## Architecture

- `crates/howdah-arimaa`: pure Rust, no UI or Tauri deps (`thiserror` only).
  It will also back a headless CLI and PyO3 bindings, so keep the API clean.
  - `types`: Color, PieceKind (Ord = strength), Piece, Square (0 = a1,
    63 = h8), Dir, TRAPS.
  - `position`: `Position` with a zobrist hash. Pieces are stored as
    cumulative strength bitboards: `at_least[color][k]` is every piece of
    strength k or greater (index 0 = occupancy, index 6 = empty sentinel), so
    `stronger_than`/`weaker_than` are single lookups and one piece type is
    `at_least[k] & !at_least[k+1]`. `apply_step` and `undo_step` are raw
    mechanics: they move a piece and resolve traps (only traps next to the
    vacated square, as in pyrimaa), with no legality checks.
  - `turn`: `TurnBuilder` is the only place step legality lives: freezing,
    push/pull, rabbit direction, 4 steps, no unfinished push, no net-null turn.
    An ambiguous enemy step is a pull, and push-finish/pull-finish steps can't
    start a pull. Pusher eligibility (stronger, unfrozen) is the same at push
    start and finish, since the pushed piece's step can't affect friendly pieces.
    `shortest_routes`/`reachable` find a friendly piece's walks on its own
    (simple steps only, the piece must survive), and `Route::best_along`
    picks the one entering the most squares of a dragged path, then the
    one losing the fewest friends on traps.
  - `setup`, `outcome` (goal, elimination, immobilization; `WinReason` also
    covers timeout/resignation/illegal move/score/forfeit, with arimaa.com
    letters), `timecontrol` (`M/R/P/L/G/T` format and reserve arithmetic,
    ported from pyrimaa `util.py`), `notation` (syntax only), and
    `game::Game`: moves plus cached positions per ply. It rejects third
    repetitions and plays moves from notation (`play_notation`, used for
    engine and server moves). `end_game` records external results; `parse`
    validates capture tokens, and `to_record` round-trips.
  - `tree::GameTree`: moves with variations (design in
    `docs/VARIATIONS.md`). An arena of `Node`s with stable `NodeId`s; the
    first child continues the main line. Adding a move that's already a
    child reuses it, repetition counts only the path to the node, and
    `from_game`/`to_game`/`main_game` convert to and from `Game`. Nodes
    carry an `Annotation` (comment, variation intro and PGN-numbered
    `Glyph`s). The session keeps its game as one. Comments can hold PGN
    commands (`[%emt 0:00:12]`): `comment_text` leaves them out, and
    `set_comment_text`/`set_command` change one part and keep the rest.
  - `record::GameRecord`: tags plus a `GameTree`, read from and written to
    the PGN-style record format (`parse`, `parse_all`, `to_record`). The
    reader is lenient (arimaa.com `Name: value` tags, `White`/`Black`
    names, `w`/`b` labels, `takeback` lines kept as variations);
    `Game::parse` goes through it and keeps the main line.
- `crates/howdah-aei`: async AEI controller (tokio; no UI deps).
  - `Engine`: spawns the process (no shell), runs the handshake, and sends
    typed commands. Messages are parsed with deadlines (`recv_until`), and
    `info pv` is split into turns. `SearchLog` reads bot_Sharp's search
    logs: the summary at the end (`log Depth 12.0233+ Eval 71 Time …`)
    and, with its `verbose` option, `ID`/`FS` progress lines with a PV.
    `Profile` (picked from `id name`: Sharp, OpFor, Generic) holds engine
    quirks: the options analysis needs and how to read scores (`Score`,
    from the mover's side). Our fork of Sharp
    (github.com/Janzert/arimaasharp) allows `verbose` outside dev
    builds.
  - `play_match`: engine vs engine with clocks, following pyrimaa's
    `game.py` (option names, timeouts, `stop` before the deadline, and
    losses by illegal move, crash, timeout, resignation or turn limit).
  - `src/bin/aei-test-engine.rs` is a random-move engine with misbehaviour
    modes, used by the tests. `--until-stop` makes it search until `stop`
    like an analysing engine, logging anything else sent mid-search as an
    error; the app's controller tests use it. `examples/match.rs` plays one game from the
    command line.
- `crates/howdah-gameroom`: client for the arimaa.com gameroom over ASIP
  (reqwest; depends only on `howdah-arimaa`). For a game on the server,
  the server is the authority on moves, clocks and results. Where paths
  differ, use the one that likely costs the server least, as the browser
  client does.
  - `wire`: `Record` decodes either reply format (`key=value` for ASIP
    1.0, JSON for 2.0); `encode_request` encodes requests.
  - `client`: `Lobby` (`login` the browser's way through `login.cgi`, or
    `login_asip`; `games` (live, recently finished, the user's and open
    games) from ASIP 2.0 `state`, read by `LobbyGames::from_state`;
    `reserve_seat`;
    `find_game_id`, a finished game's permanent id over ASIP 1.0;
    `watch`, which gets a viewer seat the browser client's way and follows
    it on `client3gs.cgi`, since ASIP viewer seats get moves only in ~10 s
    steps; `open`, the same by id for a live or a finished game; for playing,
    `new_game`, `cancel_open_game`, `my_games` and `play`, a player's
    seat the browser's way) and
    `GameServer` (`sit`, `game_state`, the `update`
    long poll, which adds each reply's new moves and chat to what came
    before and refetches the full state if the lengths don't match;
    `actions()` gives an `Actions` handle for `move`, `takeback`, `chat`
    and the like while another task polls). `Http`
    sends a Referer, spaces requests a second apart (except long polls),
    and logs exchanges through a `NetLog` with `password`, `sid`, `auth`
    and `tid` redacted.
  - `clock_sync`: `ClockSync` estimates the server's clock from replies
    stamped `timeonserver` (whole seconds): each request's send and
    arrival times bound the offset, the bounds are intersected newest
    first (dropping samples that contradict newer ones), and the
    estimate sits above the lower bound by the expected rounding
    (`1/(k+1)` s for `k` samples) plus half the quickest round trip. It
    works on the monotonic clock. Each `GameServer` (and its `Actions`)
    feeds one from every stamped reply (`Http::post_timed` times from
    when the request goes out, after the spacing wait), and the states
    it returns carry `ServerClock::estimate`d instants.
  - `state`: `GameState` from a reply: players, the moves played without
    numbers (`split_moves`, which applies the server's `takeback` lines), result (`parse_result`), and `ServerClock` worked out
    as the browser client does (`running` is false until the server
    starts the turn's clock, which it doesn't before the game starts;
    `turn_started_at`/`game_started_at` place the turn's and game's
    start on the local clock when estimated).
    `parse_chat` splits the `chat` field into `ChatLine`s
    (`<side> <label>: <text>`, raw text, the label sometimes missing).
  - `finished`: `RecentGame` (the lobby's `recentgames`) and
    `FinishedGame`, read from a finished game's viewer page
    (`opengamewin.cgi`, the `arimaa.vars` lines), with `record()` making a
    `GameRecord` (players, ratings, time control, date, `GameId`, result,
    and each move's time as `%emt`) and `state()` a final `GameState`.
  - `players`: the player search (`searchPlayers.cgi`, `PlayerMatch`),
    a player's past games (`pastgames.cgi?id=`, `PastGames`, 50 a page),
    and the postal games being played (`postalgames.cgi`, `PostalGame`,
    `Lobby::postal_games`; the lobby's live list leaves them out), read
    leniently from their HTML. The page writes times in the
    session's zone (the login's `timezone`, `Lobby::set_timezone`; the
    app sets the computer's), labelled "YLT", which is dropped. `Lobby::search_players` and
    `Lobby::player_games` fetch them. `Error::Expired` is a lobby session
    that's gone (an ASIP error or the "Session Expired" page).
  - `examples/probe.rs` runs it against the live server by hand (see the
    parent repo's notes on probing first; `probe play` takes seats and
    plays from commands on stdin). Never in tests or CI.
- `app/src-tauri`: thin shell.
  - `session.rs` has the pure state logic: a `GameTree`, the line being
    shown (`line`, root to a leaf) with a cursor ply on it, the in-progress
    turn, setup draft, and stable piece ids for animation (computed along
    the line). Committing anywhere adds a branch (`show` moves to it);
    nothing is truncated. Unit-tested, no Tauri types.
    - `followed` remembers the child last shown after each node, so a line
      through a node (`Session::line_through`) continues the way the user
      last went rather than down the main continuation, as lichess does.
    - `goto_sibling` (`↑`/`↓`) and `goto_branch` (`Shift+←`/`→`, the
      previous or next move on the line with alternatives) navigate the
      tree; `set_comment`, `toggle_glyph` and `toggle_collapsed` (folded
      variations, which stay open while the cursor is inside one) edit it.
    - It's either free play or a *match*: a player per side (human or
      engine), an optional time control per side (with the game time limit
      and turn limit enforced), and a `generation` counter bumped on every
      new game or match change, so stale engine replies are ignored.
    - A match plays one line, ending at `Match::live` (always on the main
      line). A human's move at the live node on their turn is played
      (`plays_live`); any other move is a plan, added as a variation and
      never sent. Setups can't be planned (`can_input`). Plans after the
      live node stay when the game ends, as analysis: only results on the
      board (`Node::is_terminal`) stop input, and the main line stops at a
      node with a result.
      `goto_live` shows the live node.
    - Continuous entry (`continue_turns`, on unless the setting turns it
      off): a step after a full turn finishes it (`full_turn`) and starts
      the other side's, so a line can be stepped through without
      committing. It never plays a move: at the live node the turn becomes
      a plan. `commit_turn(plan)` plays the turn (or keeps it as a plan);
      with no turn, it plays `plan_to_play`, the plan's move after live
      on the shown line.
    - Undo (`undo_step`) with no step to undo reopens the move into the
      shown position minus its last step (`reopen`; the move stays in the
      tree). At the live node it's a takeback, if the match allows them
      (`MatchSpec.takebacks`): `take_back_target` goes back to the last
      position with a human to move (one ply between engines, never into
      the setups), and the clocks are restored from `Match::turn_starts`.
      The taken-back moves stay as the continuation, and become a
      variation when a different move is played. The controller stops an
      engine whose turn was taken back and ignores its reply.
    - Engine moves enter through `apply_engine_move`, as the live node's
      first child (a matching plan becomes the move). The board follows
      them only if you're watching the live position.
    - A `Player::Remote` plays elsewhere (a gameroom game). Its moves
      arrive through `sync_remote`, the full list the server reports: new
      moves are played, and a shorter or different list moves the live node
      back (a takeback on the server). With a remote side the server keeps
      the clock (`Match::server_clock`): `set_remote_clock` sets it,
      `finish_remote` ends the game, and the session never flags time or
      the turn limit itself. A human's move at the live node in such a
      game is *sent*, not played (`Match::outgoing`, `SessionView.sent`):
      it becomes the main continuation and is shown, `outgoing_move()`
      gives it (with capture tokens) to whoever talks to the server, and
      `sync_remote` plays it once the server's list has it. Only one move
      is on its way at a time; a different server move or the game's end
      drops it, and `move_refused` turns it back into a plan (Enter
      resends it).
    - Takeback requests in such a game (`Match.takeback`):
      `request_takeback` asks for the user's last move back (refused
      while a move is on its way or a request is open, and in rated games:
      `forbid_takeback_requests`), `answer_takeback`
      answers the opponent's. `outgoing_takeback` gives what to send,
      `takeback_sent`/`takeback_failed` record how that went, and
      `sync_takeback` follows the server's `takeback` field (after
      `sync_remote`, whose going back settles an accepted request; a
      shown request of the user's that disappears without that was
      declined). `SessionView.takeback` and `canAskTakeback` show it.
  - `controller.rs`: a background coordinator task per session, plus one
    actor task per engine process. `Controller::shutdown` ends it when its
    session closes, which quits its engines.
    - It watches the session (`engine_turn`, `turn_deadline`), asks the
      engine to think, and keeps the engine's move list in sync (`newgame`
      plus `makemove`s when the history diverges).
    - It sends `stop` shortly before the deadline, flags timeouts, and turns
      engine failures into forfeits.
    - Commands call `Controller::poke()` after every change.
    - Engine output goes out as `engine://output`.
    - Analysis (design in `docs/ANALYSIS.md`): a separate actor searches
      `Session::analysis_target()` (the shown node; a turn being entered
      is ignored) until it changes. A `Think` mid-search stops the search
      and waits for its `bestmove` first, and only the newest queued
      `Think` runs. PVs are checked with `Game::play_notation` into
      `PvTurn`s, scores turned to gold's side, and snapshots sent as
      `analysis://update` (`AnalysisView`) at most every 100 ms. The
      session keeps the deepest line per node (`store_analysis`), shown as
      `SessionView.storedAnalysis` on return. `add_line` adds a PV as a
      variation (a plan in a match); `preview_line` is the hover preview.
      No analysis while the user plays an online game
      (`Session::plays_online`): `set_analysis` refuses an engine,
      `start_match` turns it off, and `SessionView.analysisAllowed`
      disables the button. Ending the match mid-game ("Leave game")
      sets `left_online`, which keeps it refused until another game
      replaces the session. Watching is fine.
  - `gameroom.rs`: spectating arimaa.com games (use case 3). `Gameroom`
    holds the login and lists live and recently finished games
    (`gameroom_games`). `open` (command `open_gameroom_game`) opens a game
    by id: a finished one is loaded whole as a record, shown from the
    start (`Session::load_record_from_start`), a live one is watched as
    below. "Remember password" saves the
    login in `gameroom-login.json` in the config dir (`SavedLogin`, mode
    600): the password XORed with a fixed key and hex-encoded, which is
    obfuscation, not encryption. Lobby calls go through `with_lobby!`: an
    expired login is renewed with the saved one (`relogin`), or else the
    user is logged out. `search_players` and `player_games` find a
    player's games. The password never goes back to the
    frontend: a login with an empty password uses the saved one, and a
    login without "remember" deletes the file. `watch` seats a viewer (`Lobby::watch`), starts a match between
    two `Player::Remote`s in the session (with record tags), and spawns a
    task that long-polls the game server (`maxwait` 300 s, as the browser
    client does). Each reply goes through `apply`: `sync_remote`,
    `set_remote_clock`, `finish_remote`. The task stops when the game ends,
    on `stop_watching`, or when the session's generation changes; failed
    polls are retried with a pause growing to 30 s, or to 5 s after a
    network error (`NETWORK_BACKOFF`), so play resumes soon after the
    network does. `SleepWatch` notices the computer having slept (the
    wall clock ran ahead of the monotonic one) and drops the old poll
    for a full `gamestate`. `Http` sets TCP keepalive (15 s idle, 5 s
    probes, 3 tries), so a long poll on a connection that died silently
    fails in about half a minute. The sender checks the state before
    sending a move again and doesn't resend while that check fails
    (the server would refuse a move it already has). When the game server
    drops the seat (a server error), `reseat` takes a new one, up to three
    times in a row; if the game ended meanwhile it asks `findgameid` and
    applies the finished game's final state (a gameroom id alone could
    open an unrelated old game with the same number). Its state goes out
    as `gameroom://watch` (`WatchView`, only when changed). Playing
    (`play`, command `play_gameroom_game`) takes the user's seat at a
    side the browser's way (`Lobby::play`); `start` makes the seat's side
    (`GameState.role`) `Player::Human`, and a second task, `send_moves`,
    posts `Session::outgoing_move` with the seat's `Actions`. It wakes on
    `Watch::poke` (from `Backend::mutate`) and on a timer: a move not back
    after `CONFIRM_WAIT` (growing with each try) is checked against a
    `gamestate` and sent again, since the server answers `ok` to moves it
    drops; an error reply calls `move_refused` and shows the message in
    `WatchView.refused` until a move goes through. The game's chat is
    `WatchView.chat`, parsed afresh from each state; `Watch::chat`
    (command `send_gameroom_chat`) posts a line from the user's seat,
    with newlines made spaces and empty messages refused (the server
    would add an empty line). `Watch::resign`
    (command `resign_gameroom_game`) posts `resign` with the seat's
    `Actions`, which the `Watch` keeps; the result arrives with the next
    update. In a server game the user's player shows their username (the
    record's tag) rather than "Human". The same task sends takeback requests and answers
    (`send_takeback`); a request the server hasn't shown after
    `CONFIRM_WAIT` is checked with a `gamestate` and dropped if missing. A player's reseat takes the same side again.
    `create` (command `create_gameroom_game`) makes a game with `newgame`
    and sits as `play` does (the browser's way; `newgame`'s own seat goes
    unused); `cancel_gameroom_game` cancels an open one, and starts a new
    game in the session if it was playing it. Until an opponent sits
    (`WatchView.waiting`: not started, the other seat empty) the sender
    holds the user's first move, and `follow` wakes it when they arrive.
    An opponent who sits later is named from the state
    (`Session::set_remote_name`, with the record's tags), and the clock
    stands still until the server starts it (`RemoteClock.running`).
    The session's remote clock is anchored at the estimated turn start
    (`RemoteClock.turn_started`, falling back to counting back from the
    reply), and on the user's own turn it starts earlier by the one-way
    delay, so it shows the time left for a move sent now. A
    session's `Watch` lives in its `SessionHandle`; `new_game`,
    `load_game`, `start_match` and `end_match` stop and forget it. The
    record gets the archive's `Event` and `Site` ("Over the Net"), and at
    the end the permanent id as `GameId` (`WatchView.finishedId` too): from
    the final state's `finishedId`, or else `find_id` asks `findgameid` up
    to four times, 2, 4, 6 and 8 s apart, since the server may not know it
    right after the end.
  - `engines.rs`: the engine list (`engines.json` in the app config dir).
    It defaults to the bundled `aei-test-engine` when that sits next to the
    app binary (`cargo build -p howdah-aei --bin aei-test-engine`).
  - `dto.rs` holds the view types sent to the UI, with ts-rs derives.
  - `backend.rs`: `Backend` holds the command logic, free of Tauri types.
    It holds several sessions keyed by `SessionId`, each with its own
    controller (`open_session`, `close_session`, `list_sessions`).
    `MAIN_SESSION` (1) is the main window's: it always exists, and
    `api.ts` has the same number (a test checks). Every session command
    takes a `session` argument, and a session's events carry a `session`
    field (`SessionEvents` adds it).
    Commands are **intents**. Mutating commands return
    `Result<(), ApiError>`, and the new state arrives as a `game://changed`
    event (`SessionUpdate {view, animation}`). Queries (`get_state`,
    `legal_targets`, `export_game`) return data. Events go out through an
    `EventSink` (the `AppHandle` in the app). `dispatch` runs a command by
    name with JSON args for the dev bridge; a test checks it covers every
    command in `api.ts`, but a new command still needs adding to both
    `dispatch` and `commands.rs`/`lib.rs`.
  - `commands.rs`: thin `#[tauri::command]` wrappers over `Backend`.
  - `src/bin/dev-bridge.rs` (feature `dev-bridge`): serves `Backend` over
    HTTP on 127.0.0.1:1421 (`POST /invoke/<cmd>` with JSON arguments,
    including `"session": 1` for session commands; SSE `GET /events`), so the
    frontend can run in a plain browser against the real session and
    engines. Its engine list lives in `target/debug/dev-bridge-config/`.
- `app/src`: Svelte 5 (runes) + Vite, no SvelteKit.
  - `lib/api.ts`: typed invoke wrappers. `session` is the window's
    session: `?session=<id>` in its URL, otherwise `MAIN_SESSION`; every
    session command passes it.
  - `lib/devBridge.ts`: in dev outside Tauri (`main.ts` checks), `mockIPC`
    forwards every `invoke` to the dev bridge (Vite proxies `/bridge`) and
    replays its event stream as Tauri events.
  - `lib/devHooks.ts`: in dev builds, `window.__arimaa` for scripted UI
    checks: `state()`, `board()` (text diagram), `message()`, `idle()`,
    `drag('d2','d5',['d3','d4'])`, `click(sq, toward?)`, `hover(sq, toward?)` (`toward` leans the
    pointer toward a neighbour, for step mode), `hoverTargets()`,
    `squareCenter(sq)`, `analysis()` (the latest `AnalysisView`), `api`. `idle()` also waits for pending hover arrows.
    Input goes through the board's own pointer handlers. Board pieces carry
    accessible names ("gold camel d5, frozen") and `data-square`.
  - `MoveList.svelte` shows the whole game tree from `SessionView.tree`
    (display order, as the record format writes it: each move, then the
    variations replacing it, indented by `depth`). Moves off the shown line
    are dimmed. A click calls `goto_node`; the right-click menu makes a
    line main, moves it up or down (`promote`/`demote`), deletes from a
    move (`delete_from`; refused for a running match's line), or copies
    moves. The session refuses edits that would move a running match's
    line off the main line (`Session::edit_lines`). Variations with more
    than one move have a fold toggle (`MoveNodeView.collapsible`/`folded`).
  - `AnalysisPanel.svelte` (engine picker, eval/depth/speed, PV chips with
    a `MiniBoard` hover preview, engine log), `EvalBar.svelte` beside the
    board, and the PV's first turn drawn by `LastMoveLayer` with `pv`.
    `lib/analysis.ts` formats evals and picks the engine.
  - `CommentBox.svelte`, under the move list, edits the shown move's
    comment (the game comment at the start; saved on blur or Ctrl+Enter,
    Esc reverts) and toggles its move glyphs.
  - `WatchDialog.svelte` (toolbar button "arimaa.com"): the gameroom
    login (with "Remember password"; a saved login fills the username, and
    the password field says "Saved password"), then the live games with a
    Watch button each, the recently finished ones with Open, a player
    search (an exact username goes straight to their games, with "Older
    games" paging), and a game id field (a permanent id loads a finished
    game, a gameroom id watches). Above those: the user's games (Play to
    take their seat again, Cancel while nobody has sat; "Your move" first,
    from `mygames`' `turn`), the open games others created (Play as the
    free side), and a New game form (side, time control in the gameroom's
    format with a few presets, the gameroom's postal ones among them,
    rated; kept in localStorage). "Show postal games" lists the postal
    games being played (`gameroom_postal_games`, one page fetch, only when
    asked), to watch. Postal is the server's call from the time control
    (days per move, or `0/0/0/0/0`, its "No time limit", which `start`
    makes untimed); `WatchView.postal` labels the panel, and clocks of a
    day or more read `Nd h:mm` (`formatClock`). It refreshes the lists every 20 s while
    open, and after an error checks the login, showing the login form if
    it has expired. `WatchPanel.svelte` under the comment box
    shows the followed game's state (spectators don't get the chat),
    "Waiting for an opponent" with Cancel game while the user's seat
    waits, and,
    at a player's seat, "Ask for takeback" or Accept/Decline for the
    opponent's request, Resign (a second click confirms), "Sending your
    move…" while `SessionView.sent` is set, and a refusal as a warning.
    TurnBar shows the player to
    move and "Stop watching" (`end_match`; "Leave game" when the user
    plays, since the game goes on at arimaa.com), and, when watching,
    its turn buttons only once the user starts planning a move. Following the live game is the
    match's usual behaviour (the board follows at the live node, otherwise
    the missed-move alert).
  - `ChatPanel.svelte`, below the bottom player bar as in the
    arimaa.com web client, shows a gameroom game's chat (names from the
    player bars, the move label, text as plain text) with a message box
    when the user plays; App shows it at a player's seat, or when a
    watched game has chat.
  - `RecordDialog.svelte` exports the full record or, with "Main line
    only", a plain record (`export_game(mainLineOnly)`).
  - `lib/events.ts`: typed `on()`. It drops events from other sessions.
  - `lib/board/`: SVG board. `BoardModel` plays `AnimStep`s: slide, then
    fade out on capture, with fade-in for restored pieces going backward.
    - Drag-to-route: `DragPath` records the squares the pointer crosses
      (cutting back on revisits, subdividing fast moves). The board previews
      `plan_route` while dragging, and a drop calls `try_route`, which always
      takes a shortest route. A piece dropped one step away stays where it
      was dropped; after a longer route it jumps back and replays the steps
      at `ROUTE_STEP_MS` each (`BoardModel.dropAt`).
    - Animated updates are queued and played in order. Duration per step is
      the base speed (`setBaseSpeed`; default `STEP_MS`, 220 ms), made
      faster for each move waiting behind the current one.
    - A live move's animation is also capped at the time the move took off
      the clock (`SessionUpdate.animationBudgetMs`).
    - With more than `MAX_BEHIND` (6) moves waiting, each is shown instantly,
      `INSTANT_GAP_MS` apart, until it catches up.
    - An update without animation that doesn't change the position (a clock
      or "thinking" change) doesn't interrupt; any other one cancels the
      queue and snaps.
    - Sounds follow 4steps (see `lib/sound.ts`): `AnimHooks.onSlide(last)`
      gives a soft click per step and a louder one for a move's last (all
      soft while a turn is being entered; the loud one comes on Play), and
      `onCapture(own)` tells a capture from losing the mover's own piece,
      using `AnimStep.mover`. A step with a capture plays only the
      capture's sound. `onRestore` is a captured piece coming back as a
      step is undone.
    - Hover input (setting `hoverInput`: off, arrows or step mode) reads
      each square's legal single steps from a cache filled from
      `legal_targets` and cleared when `positionKey` changes. Arrows: a
      hovered piece's steps (red for pushes/pulls of an enemy piece), kept
      while the pointer is on one; a click on an arrow calls `try_step`. Step
      mode (`hoverInput.ts`, after 4steps): the step toward the nearest edge
      of the square under the pointer, or a neighbour's step into an empty
      square. A click on a piece takes its step only if released on the
      same square, so dragging still works.
    - Forward at the latest move replays it: `move_replay` gives the pieces
      before the move and its animation, and `BoardModel.replay` jumps back
      and plays it.
    - `LastMoveLayer` draws `SessionView.lastMove` under the pieces once the
      board is at rest: `lastMoveTrails` joins each piece's steps into one
      trail (dashed when pushed or pulled), and captured pieces show as
      ghosts. It's hidden once the player takes a step.
  - `PlayerBar.svelte` shows names, clocks and captures (from
    `SessionView.captured`: the opponent's pieces, rabbits grouped as ×n).
    The bars show in free play too, named from the record's `Gold`/`Silver`
    tags (`SessionView.tagNames`) when there's no match, and show ratings
    from `GoldRating`/`SilverRating` (`SessionView.tagRatings`).
  - `lib/sound.ts`: Web Audio. Sounds are embedded (`?inline`), decoded
    once by our own `lib/wav.ts` (8/16-bit PCM), and played as buffer
    sources, so overlapping sounds mix.
    - Don't go back to `HTMLAudioElement`: in WebKitGTK, short clips and
      overlapping plays were often silent.
    - Clipped sounds in WebKitGTK: playing sources are kept in a set until
      they end (unreferenced ones were cut short, probably collected
      mid-play; that fixed most of it). Sounds also go through one
      long-lived gain node with a silent loop playing into it, so the
      output never goes idle between sounds. The loop is what fixed the
      rest; the shared node alone didn't. Keep both.
    - Sounds are named by event (`SoundName`: step, lastStep, capture,
      ownLoss, restore, gameStart, win, loss, tick), each mapped to a classic file.
      A match starting plays gameStart; the game ending plays loss when
      the lone human player lost, otherwise win. The low-time tick
      (`Metal2_3.wav`) plays while a human's clock runs, on 4steps'
      schedule (`lib/clock.ts`).
  - Game end: `GameEndDialog.svelte` shows the result in words
    (`lib/result.ts`) after the final move's animation, with rematch and
    swap sides (repeating the last New game spec). `justEnded` keeps it to
    games ending as they're played, not loaded records or going to the end.
  - Away from the live position in a match, an engine's or remote
    player's move shows an alert over the board (`missedMove` in `App.svelte`, from
    `SessionView.liveMove`) with a sound, until the user goes back (End,
    or the alert or TurnBar's button). The board doesn't follow; going
    back replays the missed move from the position before it
    (`replayShownMove`), unless the return already animated it.
  - The turn's main button is Play (Enter, `commit_turn`); while planning
    away from the live game it's End turn.
  - `lib/attention.ts`: when a human's opponent has moved and the window
    isn't focused, the OS attention request (Tauri permission
    `core:window:allow-request-user-attention`), or a title prefix in a
    browser.
  - Keyboard: `lib/shortcuts.ts` is the one table of shortcuts; the key
    handler in `App.svelte` and the `?` help (`HelpDialog.svelte`) both
    read it. Add a key there, with its action in `shortcutActions`. The
    help's mouse section is a list in `HelpDialog.svelte`.
  - `lib/theme.ts` plus `themes/<dir>/*.theme.json`: data-driven themes
    (image or procedural board/pieces).
  - `lib/settings.svelte.ts`: display preferences (theme, coordinates,
    sound and volume, animation speed (`stepMs`, the board's base speed),
    hover input, and `humanAtBottom`, which turns the board when a match
    with one human side starts) as one reactive `settings` object,
    saved to localStorage as JSON (`parse` validates and reads the older
    `theme`/`muted` keys).
    `SettingsDialog.svelte` binds to it directly; add new options to both.

## Conventions

- **The frontend never decides legality.** It renders state and sends intents.
  Drag hints come from `legal_targets` and `plan_route`, but `try_route`
  (or `try_step`) is authoritative.
- Event names are `domain://event`. Planned: `tournament://progress`.
  The dev bridge's frontend (`devBridge.ts`) forwards a fixed list of
  event names; add new ones there. Analysis uses an event
  (`analysis://update`, throttled to 10 Hz) rather than a Tauri
  `Channel<T>`, since the dev bridge forwards events; revisit for several
  engines at once.
- TypeScript types come from Rust via ts-rs into `app/src/lib/bindings/`
  (committed; don't hand-edit). Regenerate after changing DTOs or core serde
  types: `cd app && npm run bindings`. `.cargo/config.toml` sets
  `TS_RS_EXPORT_DIR`.
- Core serde/ts derives are behind the `serde`/`ts` features. Only the app
  enables `ts`.
- Square serializes as its index (0..63). Enums serialize lowercase/camelCase.
- rustfmt: `max_width = 110`. Conventional commits.
- The reference implementation for rules questions is `../AEI/pyrimaa/board.py`.
  `crates/howdah-arimaa/tests/movegen.rs` checks move-generation counts
  against it (both sides to move). To add cases, compute counts with
  pyrimaa's `get_moves()`. `tests/pyrimaa_cases.rs` ports the step-level
  cases from `pyrimaa/tests/test_board.py`.

## Build / test / run

```bash
cargo test --workspace            # all Rust tests (test profile uses opt-level 1)
cargo run -p howdah-aei --example match -- --gold "CMD" --silver "CMD" [--tc 2s/10s] [--transcript]
cargo run -p howdah-gameroom --example probe -- live   # live server; ARIMAA_USERNAME/ARIMAA_PASSWORD
cargo clippy --workspace --all-targets
cd app && npm install
npm run check                     # svelte-check
npm test                          # vitest (frontend unit tests, e.g. BoardModel)
npm run bindings                  # regenerate TS types
npm run tauri dev                 # run the app
npm run bridge                    # dev bridge for the browser preview (port 1421)
npm run dev -- --port 1430        # Vite for a browser; it uses the bridge (1420 is for tauri dev)
npm run e2e                       # Playwright smoke tests (starts both if needed)
npm run bridge -- -- --port 1422  # a second bridge, when 1421 is taken
BRIDGE_PORT=1422 npm run dev -- --port 1431   # a Vite that uses it
```

- **Checking UI changes:** run `npm run bridge` and `npm run dev -- --port 1430`
  (not 1420, which `tauri dev` needs for its own Vite), then use
  a browser: the accessibility tree reads the board, and `window.__arimaa`
  drives it. Use the full Tauri app only for what differs in the webview
  (WebKitGTK rendering, sound, window behavior).
- Playwright uses a system Chromium when it finds one (`CHROMIUM_PATH`
  overrides); otherwise run `npx playwright install chromium`. The bridge
  holds one session, so tests run serially and start with `newGame`.

- Linux system packages: see README.md (WebKitGTK 4.1 is the essential one).
- TypeScript is pinned to 6.x because svelte-check doesn't support TS 7 yet.
- Tauri 3 is in alpha; stay on 2.x.

## Adding a theme

Create `app/src/themes/<dir>/<name>.theme.json` (see `lib/theme.ts` for the
schema) next to its images. For image boards, `grid` is the 8×8 playing area
in image pixels. It's picked up automatically.
