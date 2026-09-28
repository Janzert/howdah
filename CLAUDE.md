# Arimaa Desktop: notes for Claude

Cross-platform Arimaa client. It's a Tauri 2 spike, but it's structured the
way the real project will be. Long term it covers gameroom play, AEI bots,
analysis, tournaments, postal games and bot tooling. This repo is a submodule
of a private parent repo (`../`) that also holds the AEI reference repo and
the original arimaa.com asset archives.

**Next steps and open to-dos are in `HANDOFF.md`.** Read it at the start of
a session and update it at the end.

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
      engine), an optional clock, and a `generation` counter bumped on every
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
  - `commands.rs`: commands are **intents**. Mutating commands return
    `Result<(), ApiError>`, and the new state arrives as a `game://changed`
    event (`SessionUpdate {view, animation}`). Queries (`get_state`,
    `legal_targets`, `export_game`) return data.
- `app/src`: Svelte 5 (runes) + Vite, no SvelteKit.
  - `lib/api.ts`: typed invoke wrappers.
  - `lib/events.ts`: typed `on()`.
  - `lib/board/`: SVG board. `BoardModel` plays `AnimStep`s: slide, then
    fade out on capture, with fade-in for restored pieces going backward.
    - Animated updates are queued. Each move waiting behind the current one
      speeds up the animation, and more than `MAX_BEHIND` (6) waiting jumps
      straight to the latest.
    - An update without animation that doesn't change the position (a clock
      or "thinking" change) doesn't interrupt; any other one cancels the
      queue and snaps.
    - Every step plays `place.wav`, as the arimaa.com clients do.
  - `lib/theme.ts` plus `themes/<dir>/*.theme.json`: data-driven themes
    (image or procedural board/pieces).

## Conventions

- **The frontend never decides legality.** It renders state and sends intents.
  Drag hints come from `legal_targets`, but `try_step` is authoritative.
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
```

- Linux system packages: see README.md (WebKitGTK 4.1 is the essential one).
- TypeScript is pinned to 6.x because svelte-check doesn't support TS 7 yet.
- Tauri 3 is in alpha; stay on 2.x.

## Adding a theme

Create `app/src/themes/<dir>/<name>.theme.json` (see `lib/theme.ts` for the
schema) next to its images. For image boards, `grid` is the 8×8 playing area
in image pixels. It's picked up automatically.
