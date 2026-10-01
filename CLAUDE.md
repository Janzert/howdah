# Arimaa Desktop: notes for Claude

Cross-platform Arimaa client. It's a Tauri 2 spike, but it's structured the
way the real project will be. Long term it covers gameroom play, AEI bots,
analysis, tournaments, postal games and bot tooling.

Design notes live in `docs/`. `docs/UI-SURVEY.md` collects UI and feature
ideas from other Arimaa and chess clients.

## Architecture

- `crates/arimaa-core`: pure Rust, no UI or Tauri deps (`thiserror` only).
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
- `crates/arimaa-aei`: async AEI controller (tokio; no UI deps).
  - `Engine`: spawns the process (no shell), runs the handshake, and sends
    typed commands. Messages are parsed with deadlines (`recv_until`), and
    `info pv` is split into turns.
  - `play_match`: engine vs engine with clocks, following pyrimaa's
    `game.py` (option names, timeouts, `stop` before the deadline, and
    losses by illegal move, crash, timeout, resignation or turn limit).
  - `src/bin/aei-test-engine.rs` is a random-move engine with misbehaviour
    modes, used by the tests. `examples/match.rs` plays one game from the
    command line.
- `app/src-tauri`: thin shell.
  - `session.rs` has the pure state logic: cursor ply, in-progress turn,
    setup draft, and stable piece ids for animation. Unit-tested, no Tauri types.
    - It's either free play or a *match*: a player per side (human or
      engine), an optional time control per side (with the game time limit
      and turn limit enforced), and a `generation` counter bumped on every
      new game or match change, so stale engine replies are ignored.
    - In a match, humans may only input on their own turn at the live end
      (`can_input`).
    - Engine moves enter through `apply_engine_move`. The board follows them
      only if you're watching the live position.
  - `controller.rs`: a background coordinator task, plus one actor task per
    engine process.
    - It watches the session (`engine_turn`, `turn_deadline`), asks the
      engine to think, and keeps the engine's move list in sync (`newgame`
      plus `makemove`s when the history diverges).
    - It sends `stop` shortly before the deadline, flags timeouts, and turns
      engine failures into forfeits.
    - Commands call `Controller::poke()` after every change.
    - Engine output goes out as `engine://output`.
  - `engines.rs`: the engine list (`engines.json` in the app config dir).
    It defaults to the bundled `aei-test-engine` when that sits next to the
    app binary (`cargo build -p arimaa-aei --bin aei-test-engine`).
  - `dto.rs` holds the view types sent to the UI, with ts-rs derives.
  - `backend.rs`: `Backend` holds the command logic, free of Tauri types.
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
    HTTP on 127.0.0.1:1421 (`POST /invoke/<cmd>`, SSE `GET /events`), so the
    frontend can run in a plain browser against the real session and
    engines. Its engine list lives in `target/debug/dev-bridge-config/`.
- `app/src`: Svelte 5 (runes) + Vite, no SvelteKit.
  - `lib/api.ts`: typed invoke wrappers.
  - `lib/devBridge.ts`: in dev outside Tauri (`main.ts` checks), `mockIPC`
    forwards every `invoke` to the dev bridge (Vite proxies `/bridge`) and
    replays its event stream as Tauri events.
  - `lib/devHooks.ts`: in dev builds, `window.__arimaa` for scripted UI
    checks: `state()`, `board()` (text diagram), `message()`, `idle()`,
    `drag('d2','d5',['d3','d4'])`, `click(sq)`, `squareCenter(sq)`, `api`.
    Input goes through the board's own pointer handlers. Board pieces carry
    accessible names ("gold camel d5, frozen") and `data-square`.
  - `MoveList.svelte` stays scrolled to the bottom while following the
    latest move, until the user scrolls up.
  - `lib/events.ts`: typed `on()`.
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
    - Every step plays `place.wav`, as the arimaa.com clients do.
  - `lib/sound.ts`: Web Audio. Sounds are embedded (`?inline`), decoded
    once by our own `lib/wav.ts` (8/16-bit PCM), and played as buffer
    sources, so overlapping sounds mix.
    - Don't go back to `HTMLAudioElement`: in WebKitGTK, short clips and
      overlapping plays were often silent.
  - `lib/theme.ts` plus `themes/<dir>/*.theme.json`: data-driven themes
    (image or procedural board/pieces).

## Conventions

- **The frontend never decides legality.** It renders state and sends intents.
  Drag hints come from `legal_targets` and `plan_route`, but `try_route`
  (or `try_step`) is authoritative.
- Event names are `domain://event`. Planned: `engine://info`,
  `gameroom://update`, `tournament://progress`. Use Tauri `Channel<T>` for
  per-request high-rate streams (e.g. one analysis run).
- TypeScript types come from Rust via ts-rs into `app/src/lib/bindings/`
  (committed; don't hand-edit). Regenerate after changing DTOs or core serde
  types: `cd app && npm run bindings`. `.cargo/config.toml` sets
  `TS_RS_EXPORT_DIR`.
- Core serde/ts derives are behind the `serde`/`ts` features. Only the app
  enables `ts`.
- Square serializes as its index (0..63). Enums serialize lowercase/camelCase.
- rustfmt: `max_width = 110`. Conventional commits.
- The reference implementation for rules questions is `../AEI/pyrimaa/board.py`.
  `crates/arimaa-core/tests/movegen.rs` checks move-generation counts
  against it (both sides to move). To add cases, compute counts with
  pyrimaa's `get_moves()`. `tests/pyrimaa_cases.rs` ports the step-level
  cases from `pyrimaa/tests/test_board.py`.

## Build / test / run

```bash
cargo test --workspace            # all Rust tests (test profile uses opt-level 1)
cargo run -p arimaa-aei --example match -- --gold "CMD" --silver "CMD" [--tc 2s/10s] [--transcript]
cargo clippy --workspace --all-targets
cd app && npm install
npm run check                     # svelte-check
npm test                          # vitest (frontend unit tests, e.g. BoardModel)
npm run bindings                  # regenerate TS types
npm run tauri dev                 # run the app
npm run bridge                    # dev bridge for the browser preview (port 1421)
npm run dev                       # Vite on 1420; in a browser it uses the bridge
npm run e2e                       # Playwright smoke tests (starts both if needed)
```

- **Checking UI changes:** run `npm run bridge` and `npm run dev`, then use
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
