# Engines

How the AEI engines the app has been run with behave, and what the
controller does about it. The protocol is `AEI_PROTOCOL.md` in
[AEI](https://github.com/Janzert/AEI), whose `pyrimaa/aei.py` is a
working Python controller. Engine quirks live in one place in the code,
`howdah_aei::Profile`, picked from the engine's `id name`.

## What the controller relies on

- **Stdio only.** Engines are run directly (no shell). AEI's socket
  channel and the legacy 2008cc mode aren't supported; add them if an
  engine needs them.

- **Positions go as `newgame` plus a `makemove` per move**, never
  `setposition`: Sharp ignores the side to move in `setposition`, and the
  move list gives the engine the history it needs for repetition. When the
  new move list extends the last one, only the new moves are sent.
- **Moves are sent in normal form**, with capture tokens (`Eg4s rh4x`).
  Sharp and pyrimaa's `simple_engine` both accept it.
- **Clocks:** the time-control options (`tcmove`, `tcreserve`, ...) at
  `newgame`, then `greserve`/`sreserve` before every `go`, as pyrimaa does.
  Engines that keep their own reserve (Sharp does) need the latter.
- **Every engine move is checked** through the core before it's played.
  An illegal move, a crash, a hang past the deadline or garbage loses the
  game; the game itself is never corrupted.
- **Analysis** (`docs/ANALYSIS.md`) needs `go` to search until `stop`,
  progress while it searches, and an answer to `stop`. A new search stops
  the old one and waits up to 2 s for its `bestmove`, so moves never reach
  an engine mid-search. An early `bestmove` (a proven win, a single legal
  move, a setup) isn't an error.
- **Scores** are read from the mover's side and shown on one scale,
  centi-rabbits (a rabbit up in the opening is about +100), with proven
  wins kept apart.

## bot_Sharp

[arimaasharp](https://github.com/lightvector/arimaasharp) by David Wu,
C++: the strongest bot as of 2015 and the reference strong engine for play
and engine-vs-engine tests. We use our fork,
[Janzert/arimaasharp](https://github.com/Janzert/arimaasharp), which builds
with CMake and current compilers and allows `verbose` outside dev builds.
Run it as `sharp aei`. Built and checked on Linux with GCC 13 and on
Windows with clang-cl; the fork's CI builds it and runs its self-tests on
Linux, macOS and Windows.

- **Search output is `log` lines**, not `info`. By default only a summary
  when the search ends: `log Depth 12.0233+ Eval 71 Time 2.81 Seed …`.
  The `verbose` option adds a line per finished iteration (`log ID Depth:
  …`) and per new best move (`log FS Depth: …`), both with the PV.
  Upstream allows `verbose` only in dev builds; the build we use removes
  that check. It takes only `true`/`false`. `SearchLog` parses all three
  kinds; analysis sets `verbose`, matches don't.
- **PVs** separate turns with two spaces and mark a turn that ends before
  its fourth step with `qpss`.
- **Evals** are from the mover's side, about 1000 per rabbit (measured: a
  rabbit up in the opening is about +1000 with either side to move).
  Proven results are near ±1,000,000, written `Win12`/`Loss5` in verbose
  lines.
- **No `go infinite`.** `setoption name ignoretc value true` (allowed in
  release builds) makes `go` search until `stop`, which it answers at once.
- **A `stop` right after `go`** (within a few ms, before its search thread
  starts) gets no `bestmove`: the search reports a null move, which Sharp
  logs as `Error: Bot tried to make illegal move:` before waiting for
  commands again. `Profile::ends_search_without_move` recognizes it, so
  analysis moves on; fast arrow-key navigation hit it in the app.
- **`setposition`** wants exactly `[` + 64 squares + `]`, and ignores the
  side: with `s` it still searches for gold.
- **Clock:** it keeps its own reserve (a default of 60 s showed in its log),
  so it needs `greserve`/`sreserve` as well as the time control.
- `threads` and `hash` work.
- For a setup it answers with a `bestmove` and no PV.
- Not looked into: at `2s/0` (no reserve) it moved in about 0.02 s at
  depth 3-4 instead of using its 2 s; at `3s/10s` it used 2.8 s.

## bot_OpFor

[OpFor](https://github.com/Janzert/OpFor), D (the D2 port builds with
LDC).

- **Standard `info` lines**, one field each (`depth`, `time`, `nodes`,
  `score`, `pv`), at each new depth and each new best move. Later PV turns
  start with a side letter (`… b ed7s w Ee6n`).
- **Scores** are centi-rabbits from the mover's side; proven wins are
  about ±32,650 (`WIN_SCORE` 64000 divided by 1.96).
- **With no time control and no depth limit** (the defaults), `go`
  searches until `stop`. It stops by itself on a proven win or a single
  legal move.
- **`stop`** takes 0.2-0.3 s since its search runs in 0.1 s slices (it
  was 0.5-1.2 s with 1 s slices).
- `setposition` honors the side. On an immobilized position it answers
  with an empty `bestmove`; the app treats such positions as over anyway.

## Simple engines

- `aei-test-engine` (`crates/howdah-aei/src/bin`): random legal moves,
  with misbehaviour modes and `--until-stop` for the tests.
- pyrimaa's `simple_engine` and the other simple AEI engines: run as
  `python3 -m pyrimaa.simple_engine` with the AEI checkout as the working
  directory. Good for integration tests.
