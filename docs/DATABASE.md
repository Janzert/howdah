# Game database: survey and plan

Status: plan agreed (2026-11-17). Build steps 1 to 7 are done; see
"Built so far" at the end.

`VARIATIONS.md` already decided that a game database will be the primary
store for games played and analysed, with records for sharing. This note
surveys what chess and Go programs offer in their databases, then sketches
how Howdah's could work, with three goals beyond the usual ones:

1. **A local cache of arimaa.com.** Besides a bulk import of the game
   archive, opening an arimaa.com game looks in the local database first,
   and a game fetched from the server is kept, so the database fills
   itself as the user browses.
2. **Extensible data per game and per move**: engine evaluations,
   annotations, clocks, and kinds of data not thought of yet, without a
   schema change for each.
3. **Distribution and sharing**: a base database shipped with (or
   downloaded by) Howdah, and an easy way for users to share all or part
   of theirs and to merge in what others share.

## Survey

The survey is from the programs' documentation and from use. Details of
current versions are worth checking before relying on one.

### Chess

**ChessBase** (and Fritz, which shares its database engine) is the
reference most other programs are measured against.
- Its own binary format (`.cbh` plus companion files), with the game
  list, players, tournaments, annotators, sources and teams as separate
  indexed tables. Spelling variants of names are merged through a
  player encyclopedia (birth dates, photos, ratings history).
- Searches: header fields, a position (exact), material and pawn
  structure, piece maneuvers, patterns over a range of plies, and
  "medal" themes (tactics, strategy, endgame type). Search results are a
  filtered game list that can be searched again.
- **Opening reference / tree**: for the current position, the moves
  played, with game counts, score, average rating, performance and the
  latest year, built from the open database or a precomputed tree
  ("opening book" files are trees with weights).
- **Opening reports and player dossiers**: generated pages summarising a
  line's main plans and games, or a player's results and repertoire.
- **Annotations**: text before and after a move, glyphs, colored squares
  and arrows, training questions, embedded media, and per-move engine
  evals. Multiple annotators can be merged into one game ("merge games"
  combines variations and comments of the same game from two sources).
- **Mega Database** is a curated base database sold yearly, with weekly
  updates as small databases to append. Duplicate removal and a "game
  identity" check keep repeated imports clean.
- **Let's Check**: a shared, server-side store of engine analysis keyed
  by position. Users who analyse upload their deepest line; anyone sees
  the best known eval for a position, with the engine and depth. This is
  the closest existing thing to sharing evals across users.
- **Cloud databases**: databases kept on ChessBase's server and shared
  with chosen users.

**SCID / SCID vs PC** (open source, the main free desktop database).
- Its own compact format (`.si4`/`.si5` index, name file, game file).
  Moves are stored as one or two bytes each, with the index holding
  enough per game (material signature, home pawn data) to reject most
  games of a position search without decoding them.
- A **filter** (the current subset) that searches combine with AND/OR,
  over headers, board position (exact, pawns only, files, any side), and
  material/patterns.
- A **tree window** (move statistics from the current position) with a
  tree cache file, and an optional book.
- **Spellcheck files** that normalise player, event and site names and
  carry player info (titles, ratings, countries).
- Duplicate detection, compaction (deleted games stay until compacted),
  ECO classification of openings, "Find novelty", and per-player
  reports. Merging databases is a copy of games, with duplicate removal
  as a separate step.

**Chess Assistant** (and Aquarium): a tree that stores engine evals per
position alongside game statistics, so the tree doubles as an analysed
opening book. The "IDeA" project fills a position tree with engine
analysis automatically, expanding the most interesting lines first.

**En Croissant** (Tauri + React, GPL-3.0, the same stack as Howdah).
- Databases are **SQLite** files (from PGN imports), with tables for
  games, players, events and sites, and moves stored as a compact binary
  blob rather than rows per move.
- Downloads a user's games from lichess and chess.com by account and
  keeps them up to date; there are also ready-made databases to
  download (curated master games).
- Position search (exact or partial) scans the games' move blobs, with
  the material counts stored per game as a pre-filter, rather than
  keeping a position index.
- An opening explorer over the local database, player statistics, and
  per-move engine reports from whole-game analysis.

**lichess** (server-side, but it sets the expectations for users).
- An **opening explorer** with three sources: masters games, all lichess
  games (filtered by rating band and speed) and one player's games.
- A **cloud eval** cache: evals by position (multi-PV, depth) served to
  everyone, filled by the site's own analysis. Requests are by FEN.
- **Studies**: shared, collaboratively edited annotated games, with
  chapters, comments, shapes and evals, exported as PGN with `[%eval]`,
  `[%clk]`, `[%cal]`, `[%csl]` comment commands. Copying a study to
  your account is the sharing model.
- Monthly database dumps of every game (PGN, zstd), and a puzzle
  database generated from them, are free downloads.
- "Insights": charts of a player's results by opening, phase, material,
  time and more.

**chess.com** offers per-player archives through a public API (monthly
lists of a player's games), which desktop programs use for import.

### Go

**Kombilo** (open source): a database of SGF files with
- **pattern search**: a region of the board with stones, empty points
  and wildcards, anywhere on the board or fixed, with the color swap
  and symmetries; results list the continuations played next (with
  counts and win rates) and the games.
- Header search, date ranges, and a game-info index; the pattern search
  runs over a precomputed index of positions for speed.
- Tags on games (user-defined), and a "references" link from a game to
  commentary in books.

**GoGoD** is a curated collection of professional games (over 100,000)
distributed as SGF files, with an encyclopedia and yearly updates. It's
the Go equivalent of Mega Database, and it ships as files that each
program indexes itself. **Waltheri's** online pattern search does
Kombilo's searches over a large professional collection on a server.

**SmartGo** and **CGoban/KGS**: pattern search with continuation
statistics ("fuseki library"), and server-side archives that clients
download per player.

**KaTrain, Lizzie, Sabaki**: no database, but they store per-move engine
evals (winrate, score, visits) inside the SGF file as custom properties,
which is the usual way Go tools carry extended data: SGF allows
private properties, and readers keep the ones they don't understand.

### Shogi and others

Shogi programs (ShogiGUI, Kifu for Windows) follow the chess model:
a game list with header search, a joseki/opening tree with statistics,
and engine evals stored per move in the record. Nothing they do goes
beyond the chess programs above.

### What the survey suggests

- **Every program separates the game list (headers) from the moves**,
  and stores moves compactly in one blob per game. Rows per move or per
  position are kept only for indexes.
- **Position search** comes in two styles: an index of positions
  (Kombilo, ChessBase's tree files), or a fast scan with a per-game
  pre-filter (SCID, En Croissant). With Arimaa's game count a scan is
  likely fast enough, so an index can wait.
- **The move tree with statistics** (explorer) is the feature users use
  most. For Arimaa the setup explorer is the first version of it.
- **Name normalisation** (aliases, spellcheck files) matters as soon as
  databases from different sources are merged.
- **Duplicate handling and game identity** are what make repeated
  imports and merges painless. Programs that bolt it on afterwards
  (SCID's separate duplicate pass) are worse for it.
- **Extended per-move data** is handled either as typed columns (clock,
  eval) or as an open-ended key/value property (SGF private properties,
  PGN comment commands). Programs that keep unknown properties on
  round-trips are the ones whose files can be shared safely.
- **Evals by position, not by game** (Let's Check, lichess's cloud eval,
  Chess Assistant's tree) let analysis carry across games and
  transpositions, and make sharing analysis natural.
- **Base databases** are distributed as a curated snapshot plus small
  periodic updates (Mega Database, GoGoD, lichess's monthly dumps).
- **Sharing** today is by exporting files (PGN, SGF, a database file) or
  by a server (Let's Check, lichess studies, cloud databases). No desktop
  program merges annotations from several people with provenance, so
  that would be new.

## Plan sketch

### Storage

- **SQLite**, one file per database, through a new crate `howdah-db`
  so the CLI, the tourney daemon and the app share it, using `rusqlite`
  with the bundled SQLite (see "Library" below).
- **One main database** in the app's data directory, holding everything
  the user has: games played, analysed, imported and cached. The base
  database and shared databases are files in the same format that are
  **imported into it** (or, for a new user, copied as its starting
  point). One format for the base, for sharing, and for backups keeps
  everything else simple: an update to the base is just another import.
  Other databases can still be opened read-only to browse without
  importing.

### Size

From the 425,130 games in the arimaa.com archive (2026-10 snapshot):
14.2M moves per side, so about 28.5M turns. The text move lists are
706 MB, and the server event logs 955 MB.

- A step fits in one byte (a from-square and a direction; the piece and
  any capture follow from the position), so a game's moves as a blob are
  about 4 bytes a turn: around 110 MB for the whole archive, less after
  compression. Headers add perhaps 50 MB.
- The event logs give the time used per move and both reserves; kept as
  two small numbers per turn that's tens of MB. The logs themselves stay
  out of the base database (they can be fetched for a single game).
- So a base database of every arimaa.com game is likely 100–200 MB on
  disk and well under 100 MB compressed. That's too big for the
  installers, so it's a separate download offered on first run (a
  release asset, checked against a digest), with updates the same way.

These are estimates; the first step is to build it and measure.

### Games and identity

- Each game has a random **`gid`** (128 bits) that stays the same in
  every database it's copied to, plus a **source key** where there is
  one: (`arimaa.com`, permanent id), or a tourney's test and game
  number. The source key is unique.
- An import matches games by source key, then by `gid`, then by a
  **fingerprint** (a hash of the setups and moves, with the players and
  date when present) for record files that carry neither. A match merges
  instead of adding a copy.
- Headers are columns for what every game has (players, ratings, event,
  site, time control, rated, postal, start and end times, result,
  termination, ply count) and a **tag table** for everything else, so a
  record's unknown tags survive a round-trip.
- **Players** are a table with a source key too (arimaa.com username and
  id), the type (human or bot), country and title, plus **aliases**: an
  arimaa.com player's earlier usernames, and merges the user makes.
- A game can be **header-only**: a row from a game list (recent games, a
  player's past games) whose moves haven't been fetched yet. That lets
  lists work offline and shows what's missing.

### Variations and node identity

Variations are stored with the game when it has any. The important part
for merging is how a node is named:

- A node is identified by **its game and the moves from the start**,
  stored as (parent node, move), unique. The main line's nodes are the
  plies of the main-line blob; a variation's nodes are rows.
- Then two copies of the same game with different variations merge by
  union: a node that exists in both is the same node, and annotations on
  it from either side land on the same place. Reordering (promote,
  demote, main line) is a per-database preference, not identity.
- Deleting is local: a merge never deletes, it only adds.

### Extended data

Two kinds, both open-ended:

- **Annotations on a game or a node** (`annotations` table): game, node
  (or none, for the whole game), a **kind** (`comment`, `intro`,
  `glyph`, `shape`, `clock`, `eval`, ...), an **author**, a created
  time, a stable id of its own (so a re-import updates instead of
  duplicating), and a payload (JSON, or a blob for bulky kinds such as
  clocks for a whole game). Howdah knows a list of kinds and their
  payload versions; kinds it doesn't know are kept and exported as they
  are, so newer versions and other tools can add kinds without breaking
  older ones.
- **Data on a position** (`position_data` table), shared by every game
  and line that reaches it: keyed by the position's Zobrist hash (already
  stable across builds) with the side to move, plus a kind, an author or
  engine, and a payload. Engine evals go here: engine name and version
  (from `engine.json` where there is one), settings, depth, nodes, time,
  score and PV, so one analysis run serves every transposition and
  every later game, as `ANALYSIS.md` wants. Several engines' evals sit
  side by side; which one shows is a setting.
  - A 64-bit hash over the archive's ~28.5M positions makes a collision
    unlikely (about 1 in 40,000 for the whole archive), but a stored eval
    could keep a short check (more hash bits) to rule it out.
  - An eval depends a little on history (repetition), which the key
    ignores. That's the same trade-off lichess and Let's Check make.
- **Provenance.** Every annotation and position datum records where it
  came from: the author name, and the id of the database it was first
  written in. So after merging someone's database the user can filter
  by source, hide it, or remove everything from it.

### arimaa.com as a cache

- **Opening a finished game** by id (from a list, a chat link, a
  comment page, a typed id) looks for the source key first. If the game
  is there with its moves, it opens from the database, offline too. If
  not (or only its header is), Howdah fetches it as it does today and
  stores it before opening it.
- **Game lists** (recent games, a player's past games) store their rows
  as header-only games, and a list can show which games are already
  local.
- **What can change after a game ends**: comments (fetched live and not
  treated as part of the game), and rarely the rating or a voided game.
  Each cached game records when it was fetched, and a refresh is an
  explicit action.
- **Bulk import** of the archive files from the CLI and the app (decided
  in `VARIATIONS.md`), and **updating** the database: the newer archive
  months, then the recent games list for what's newer than the archive,
  checking for a gap between the two. Both follow arimaa.com's preferred
  ways of downloading, to keep the load on the server down.
- Games played in Howdah (against engines, BvB, the tourney's results,
  live arimaa.com games once finished) go in with their own sources.

### Searches and the explorer

- **Header search** with SQL indexes: player (with aliases), opponent,
  dates, event, time control, rated, postal, result, termination,
  length, ratings. The result is a game list that can be narrowed again
  (SCID's filter).
- **Setup explorer** first: the setups are a small table (each side's
  setup, normalised), with statistics per setup and per pair, and per
  player.
- **Move tree / explorer** for the first turns after that, computed on
  demand by scanning the move blobs (in parallel, at most 8 threads),
  with a cache. Arimaa games diverge quickly, so the tree's useful depth
  is shallow.
- **Position search**, exact and then partial (pieces on given squares,
  Kombilo-style), by scanning with a per-game pre-filter (material over
  the game, captures). An index can come later if a scan is too slow.
- **Player statistics** (the career-wide counts the player pages wait
  for) come from the same tables.

### Sharing

- **Export** a selection (a filter, a player's games, a set of games
  with their analysis, or only evals) as a database file in the same
  format, choosing what goes with it: annotations by author, evals by
  engine, variations, clocks. Records (the PGN-shaped format from
  `VARIATIONS.md`) stay the way to share single games with other tools.
- **Import** merges as above: games by identity, nodes by move path,
  annotations by their own id (the newer edit wins), evals keeping the
  best per position and engine (deepest, then newest). Nothing is
  overwritten silently: a conflict on an annotation keeps both, under
  their authors.
- **Later, a shared server**: an eval and annotation store like Let's
  Check. The provenance and ids above are
  what it would need, so the local design shouldn't have to change for
  it.

### Build order (proposed)

1. `howdah-db`: schema with migrations, the compact move codec, saving
   and loading a `GameTree` (variations and annotations included), and
   the archive import in the CLI. Measure size and import time on the
   whole archive.
2. The app: arimaa.com games open through the cache; games played and
   analysed are saved to the database.
3. A game list with header search, opening games from it.
4. Evals by position: the session's `evals` are written to and read from
   `position_data`, so analysis persists and carries across games.
5. The base database: a build script, a release asset, the first-run
   download, and update by import.
6. The setup explorer, the move tree and position search (built).
7. Export and import of selections, with provenance filters (built).

### Decided (Brian, 2026-11-17)

- **One main database, with imports.** The base and shared databases
  are imported into it rather than attached. Since a reset or a clean
  update of the base relies on it, every game and annotation records
  the database it came from.
- **The tourney daemon's games stay out** of the database. They can be
  imported by hand, a whole test or single games.
- **A game's chat is stored** (as extra data on the game, out of the
  headers), **but not exported by default**: export has an option to
  include it.
- **The base database's download** (2026-11-17): the database with its
  indexes and fingerprints dropped and `VACUUM`ed, compressed whole.
  Under 200 MB is good enough for now; the app rebuilds the indexes
  after the download. On the 2026-10 archive that's 344 MB, and 147 MB
  with bzip2 (gzip 228 MB, zstd -19 184 MB, xz -9 167 MB on the full
  400 MB file).
- **Authors are a free name** for now, but the format leaves room to
  sign annotations later:
  - An annotation names its node by the move path, never a local row
    id, so the signed content means the same thing in every database.
  - Its payload has a canonical encoding (fixed field order, no
    insignificant whitespace), written that way from the start, so a
    signature over (id, game `gid`, node path, kind, payload, created)
    can be checked later.
  - An edit makes a new revision under the same id, never changes a
    revision in place.
  - Nullable `author_key` and `signature` columns can come in a later
    migration without changing anything else.

### Library

`rusqlite` rather than `sqlx` (proposed). Both link the same SQLite
library (`libsqlite3-sys`, bundled), so the files are the same either
way. `rusqlite` is synchronous, which suits a desktop app whose scans
run on their own threads, and it gives the whole SQLite API: the backup
API (for export), incremental blob I/O, custom SQL functions (a
position test inside a query), hooks, and the session extension
(changesets). `sqlx`'s advantages are async, queries checked at compile
time (which need a database or saved query data at build time), and
built-in migrations; the plan below covers the last.

To stay readable by older SQLite builds (other tools, users' own
scripts), the schema avoids recent features: `STRICT` tables (3.37),
JSONB (3.45), and generated columns (3.31).

### Schema versions and migrations

- **The file says what it is.** `PRAGMA application_id` is a fixed
  Howdah value, so a stray SQLite file is refused instead of
  "migrated", and `PRAGMA user_version` is the schema version.
- **Migrations are an ordered list in `howdah-db`**, each one either SQL
  (`include_str!` of `migrations/NNNN_name.sql`) or a Rust function for
  what SQL can't do (re-encoding move blobs, rebuilding a derived
  table). Opening a database runs the ones past its `user_version`, each
  in its own transaction that also sets the new version, so a failure
  leaves the file at the last good version. Migrations are never edited
  once released; a fix is a new one. (`rusqlite_migration` does most of
  this, but a list of our own is small and allows the Rust steps.)
- **A newer file than the app knows** is opened read-only, or refused
  if it's too far ahead: each migration says whether older readers can
  still read past it (adding a table or a column is fine, changing an
  encoding isn't), and the database stores the oldest version that can
  read it.
- **Before a migration that rewrites data**, the main database is
  copied aside with the backup API, and the copy kept until the next
  successful open.
- **Imported and shared files** are never changed in place: an older
  file is copied to a temporary file, migrated there, and imported from
  the copy. A file that's newer is imported only if it's readable by
  this version, and kinds of extra data it doesn't know are kept as
  they are.
- **Exports** are written at the current version, so sharing with
  someone on an older Howdah needs them to update, unless the format
  hasn't changed in a way they can't read.
- **Tests:** a fixture database for every released version, each
  migrated to the newest and compared with a fresh database (the
  schema from `sqlite_master`, and the data read back the same), so a
  migration that drifts from the fresh schema fails CI.

## Built so far

### Step 1 (2026-11-17): `howdah-db`

The crate, its schema and migrations, saving and loading games, and the
archive import, as planned above, with these details:

- **Identity.** A game with a source key gets a `gid` made from it (the
  first 16 bytes of a SHA-256 of `arimaa.com:<id>`), so the same
  arimaa.com game has the same `gid` in every database, imported or
  cached. Other games get a random one.
- **Results** are stored as letters (`winner` g/s/d/u and
  `termination`), so old draws and the `p`, `n` and `a` endings keep
  their place although the tree can't hold them.
- **Old rules.** `GameTree::set_old_rules` plays by arimaa.com's rules
  before 2008.07.01: losing every rabbit doesn't end the game, and a
  third repetition is a legal move (it lost, `p`). The archive import
  uses them for games that ended before 2008-06-18 18:50 UTC, when play
  past eliminations stopped, and the game's `rules` column says so,
  so it loads the same way.
- **Takebacks** in a move list leave the taken-back move as a variation,
  as the record reader does.
- **Setups** come back in square order, so a record's text can differ
  from what was saved, though the positions are the same.
- **Move times** are one blob per game, and are put back on load as
  `%emt` first among each comment's commands.
- **Annotations** so far are the user's own (author `''`): comments,
  variation intros, glyphs, times, the chat, and the move list of a game
  that didn't replay. Other authors and position data have their tables
  but nothing writes them yet.

Measured on the whole archive (254 `allgames` files, 425,129 games,
28.2M plies), on 8 threads:

- The import takes 100 s and peaks at 490 MB of memory.
- 51 games (0.012%) have moves that don't replay: 16 third repetitions
  under today's rules, a few unreadable tokens and impossible steps. Their
  moves stop there, with the move list kept.
- The file is 400 MB: games 238 MB (the move blobs 140 MB, about 5 bytes
  a ply; headers the rest), annotations 90 MB (the move times 67 MB), and
  indexes about 70 MB. For the base database's download, dropping the
  indexes and compressing is enough for now (see "Decided"); storing
  the repeated event, site and time control strings once would shrink
  it more if that's ever needed.
- Loading every game back with the rules checked (`howdah-db verify`)
  takes 182 s on one thread, 2,300 games a second, with none failing.
  That's fine for opening a game; scans for the explorer and position
  search will need a decoder that skips the legality checks.

### Step 2 (2026-11-17): the app saves and caches games

- **Every game is saved as it changes.** A session's game is saved once
  it has a move, every 2 seconds while it changes and when its board
  closes: games played (local and on arimaa.com, watched or played),
  records opened, and analysis boards. Puzzles (their solution stays
  out of the tree until found) and engine tests' games (decided) are
  not. A game loaded from the database is saved back into its own row,
  so analysis of an arimaa.com game stays with it.
- **Finished arimaa.com games open from the database first.** The
  frontend says when an id is a finished game's permanent id (lists of
  finished games, the rating chart, chat links), since a live game's
  gameroom id can be the same number as an old game's permanent id.
  Such a game opens from the database with no request and no login; one
  that isn't there is fetched as before and kept. The game id field,
  which takes either kind, still goes to arimaa.com, and keeps what it
  gets if it's a finished game.
- **A source's copy never wipes the user's work** (`SaveMode::Source`):
  re-importing an archive file or fetching a game again updates its
  headers and move times, and keeps the user's variations and comments
  while the main line is the same.
- A watched game is saved without an id while it's played, and gets
  its `GameId` at the end; if the database already has that game, the
  two merge into the existing one.

Checked in the browser preview: game 620100 (imported) opened from the
database while logged out in 12 ms, with move times; a variation and a
comment added there were saved into its row, with the archive's
headers kept, and came back in a new board; game 93252 (not there) took
0.5 s from arimaa.com the first time and 0.02 s from the database the
second; a record opened was saved as a new game.

### Step 3 (2026-11-17): the game list

The lobby's **Games** panel (under Home in the sidebar) lists the
database's games, newest first: by start time, else end time (88
archive games were forfeited before they started), else when they were
added. Filters: a player (by any name they've had, any case), an
opponent (or either player), the player's side, won or lost, the
ending (arimaa.com's letters, old ones included), a date range (UTC
days), arimaa.com or this app, and rated. 50 games a page with Show
more; each opens on a board, and Delete takes one out (a second click
confirms). A game known only by its header is fetched when opened.

The order has its own index, added by the second migration (an index
on an expression, which SQLite has had since 3.9). On the whole archive
a page takes 1–2 ms unfiltered and 4–8 ms by player, counting
included. Checked in the browser preview on the whole archive.

### Step 4 (2026-11-17): evals by position

What analysis finds is kept in `position_data` (kind `eval`) by the
position's Zobrist hash, side to move included, and an author: the
engine's own `id name` and `id version` (OpFor gives no version, so the
version of the release it was installed from stands in), so each
engine version's evals are its own. The payload is compact JSON with
its fields in a fixed order (`v`, `depth`, `eval`, `nodes`, `pv`,
`timeMs`; the score from gold's side as `{"cr": 58}` or
`{"win": "g", "turns": 2}`), and the PV is the turns in notation,
checked again when read.

- The session's saver (every 2 s, and when a board closes) writes the
  lines found since its last pass, and looks up the positions it hasn't
  asked about yet. A stored line is taken unless the board has one from
  more search, and it shows without analysis being on.
- The engine's author is only known once it has started, so the app's
  engine ids and their authors are kept in a local table
  (`local_engines`, the third migration; this computer's alone, not for
  sharing). Until an engine has run once, its stored evals can't be
  found.
- Evals by other authors (another engine version, or later someone
  else's) are kept but not shown yet; choosing among them is for the
  sharing step.
- **Nothing stored shows in a game the user is playing online** (Brian,
  2026-11-17): stored evals follow the rule for analysis
  (`Session::analysis_allowed`), so while the user plays an arimaa.com
  game, or after they left one mid-game, positions aren't looked up and
  the move list shows no evals. They're looked up once the game is
  over. Watching a game is fine, as for analysis. Local games against
  engines allow analysis, so they show stored evals too.

Checked in the browser preview: OpFor analysed game 620100 for 20 s;
its lines were written under "OpFor 2026.10.6", and a second board
opening the game showed them in the move list with analysis off. In
those short runs OpFor left many quick-pass positions without a line
(40 searched, 10 kept); that's the run's own behaviour, which the
database only mirrors.

### Step 5 (2026-11-17): the base database

- **Building it:** `howdah-db publish OUTDIR ARCHIVE... [--new-base]
  [--cap PCT]` imports the whole archive into a new database, clears
  the fingerprints, drops the indexes (the unique keys stay) and
  vacuums it, then publishes it into `OUTDIR` (`base::publish`). The
  base is packed with zstd (level 22, a 128 MB window) as
  `howdah-games-<time>.db.zst` the first time, and again with
  `--new-base`. zstd won on the measurements: 143 MB, unpacked in half
  a second (bzip2 147 MB in 9.5 s; xz 140 MB in 6 s). Packing it takes
  about five minutes.
- **Deltas:** the archive gains about 4,000 games a year now (2,776 in
  2026 to October), about 1.5 MB packed, so a 143 MB download for a
  week's games would be a waste. Each publish imports the whole archive
  afresh, and the archive only grows, so the publisher keeps one record,
  `OUTDIR/seen.txt`: the arimaa.com game ids, in blocks under the time
  of the publish that first had them (`@<time>`). A publish appends a
  block, so it's small to check in and to diff; it's loaded into a
  table (`seen-work.db`) while the publish runs.
  A delta *since* a time holds every game first published after it, so
  it brings any database at that time or later up to date; games it
  repeats are skipped by the merge. Each publish with new games
  rebuilds the deltas, all ending at the newest games, starting 1, 2,
  4, 8, … publishes back and at the base, older ones stopping once one
  is larger than `--cap` percent of the base (10 by default; the
  base's is kept whatever its size, since a new database needs it).
  They're `howdah-games-<time>-since-<time>.db.zst`. A publish with
  nothing new writes nothing. Games the archive changes or drops later
  aren't carried by deltas; a new base has them.
- **The manifest,** `base.json` beside the files: `at`, when the newest
  games were first published, the games in all and the newest one's
  date, the base and the deltas. Each `Pack` gives its file's name, size
  and SHA-256, the unpacked size, the schema version, its games, the
  `at` a database reaches with it, and for a delta its `since`.
- **Publishing it:** in its own repository,
  `github.com/Janzert/howdah-game-database` (its README has the steps),
  not Howdah's, so its
  updates keep their own schedule and never become Howdah's latest
  release, which the update check reads. One release, tagged `current`,
  holds `base.json` and the files it names, and the app reads
  `https://github.com/Janzert/howdah-game-database/releases/download/current/base.json`,
  taking the files from beside it. That repository keeps what a
  publish needs besides the archive files: `base.json`, `seen.txt`, and
  `howdah-rev`, the Howdah commit whose `howdah-db` its `publish.sh`
  builds and runs; `upload.sh` brings the release in line with
  `base.json`. Swapping assets isn't atomic, so the
  file names carry times and are never reused (`pack` refuses to write
  over a file): the new files go up first, then the manifest, and then
  the files the publish lists as no longer named come down. A new base
  is needed only when the deltas grow large next to it (years at
  today's rate) or to pick up corrections; databases from before it
  still get a delta.
- **Checking for it:** the app fetches `base.json` at most once a day,
  keeping it in `meta` between, so opening the Games panel or restarting
  doesn't ask GitHub again; the panel's refresh button checks at once,
  and installing always fetches it afresh. Debug builds use the release
  too, unless `HOWDAH_BASE_URL` names another manifest (a local server)
  or is `off` (the end-to-end tests).
- **Getting it:** the app keeps the time it reached in `meta`
  (`base_at`, with its games and date). `base::plan` works out what a
  database needs: nothing at the manifest's `at` or later, else the
  delta starting at or closest before its time, else the base and the
  delta after it: two files at most. The Games panel offers that when
  it's anything and this version of Howdah can read every file's
  schema, as "N new games" (the manifest's count less the database's)
  when it's only a delta. Each file streams to `base-download/` beside
  `games.db`, is checked against the manifest's size and SHA-256 (and
  refused past the size it gives), unpacked and merged in, and the time
  is recorded after each. The folder is removed after.
- **Merging** (`Database::merge_from`, also `howdah-db merge`): games
  are matched by `gid` and then source key. A new game comes with its
  tags, variations and annotations; one known only by its header gets
  the other's moves; one already here with its moves is left as it is
  (joining two copies' variations and annotations is for sharing).
  Players are matched by site key, else by name; data on positions is
  added where there's none from the same author. The whole archive
  merges into a database in 8.5 s, using 15 MB of memory.

Checked in the browser preview against a local server: the card offered
425,129 games through 2026.10.11 at 142 MB, the install went through
checking, unpacking and merging, the list then had every game, and the
card went away.
Then with deltas, published from the archive files of 2019 (the base,
7,827 games), to mid-2020, to 2021, and to April 2021 with a new base:
an empty database after the second publish got the base and the delta
after it (12,148 games, 4.4 MB). After the fourth, that database, now
behind the new base, was offered "8,977 new games", the 3 MB delta
since the second publish, not the 7.3 MB base; it went in alone, the
count matched the archive's 21,125, and nothing more was offered.

Games merged from the base have no fingerprint; it's only needed to
match a record file that carries no ids, so it can be filled in later.

### Step 6, first part (2026-11-18): the setup explorer

`howdah-db`'s `setups.rs`, and the Games panel's Setups view.

- **Keys.** A `Setup` is one side's setup seen from gold's side: its
  16 squares a1 … h1, a2 … h2, silver's ranks 8 and 7 read as 1 and 2,
  so a setup has the same key for either color. Written as 16 letters
  (`RCDHME`, `.` for an empty square), e.g. `RRRCCRRRRHDMEDHR`.
- **Mirror images count as one,** written with the elephant on files
  e–h. On the archive gold's elephant is on d2 and e2 about equally
  (202k and 205k games) and silver's more often on e7 (224k to 174k),
  so e–h shows the more common way for both. Silver's reply to a gold
  setup is mirrored with gold's, so it keeps its place relative to
  gold's pieces; silver's setup alone is mirrored on its own.
- **No derived table.** `Database::setup_stats` reads the first 17
  bytes of every game's move blob that the filter lets through (the two
  setups are the first two moves, 8 bytes each) and groups in Rust: all
  425k games in 160 ms (release build), the replies to the top gold
  setup in 100 ms, one player's games in 6 ms. So there's nothing to
  keep in step with saves, merges and the base download. A table can
  come later if the move tree needs one.
- **Statistics** per setup: games, gold's and silver's wins, and over
  the games with both ratings gold's wins and the Elo-expected wins,
  since setups follow strength (the strongest players' setups win
  more); the average rating of the side playing it; the newest game.
  The archive has 19,960 distinct gold setups and 23k silver ones, with
  the top five gold setups in a third of the games.
- **Filter by setup.** `Filter`'s `gold_setup` and `silver_setup` (the
  reply when there's a gold setup, else silver's alone) use the SQL
  function `howdah_setup(moves, start_ply, 'g'|'s'|'r')`, registered on
  every connection `howdah-db` opens and never in the schema, so other
  tools can still read and write the file. It's a scan, 0.7 s for a
  page of the whole archive in a debug build, so `library_games` now
  runs on a blocking thread, as `library_setups` does, not on Tauri's
  main thread.
- **The panel.** Games has List and Setups views over the same filter
  (`lobby.gamesView`). Setups lists gold's setups, or silver's; a gold
  setup opens silver's replies to it (`lobby.setupExplorer` keeps
  where); Games on a row shows its games in the list with the setups
  as chips (Setups… goes back, Any setups clears them).
  `SetupDiagram.svelte` draws a setup's two rows with the theme's
  pieces, labelled with its pieces in words for screen readers and
  `read_page`. A row's score is green or red when the side playing the
  setup did 3 points better or worse than its ratings predicted, over
  20 or more rated games.
- `howdah-db setups DB [gold|silver|KEY] [PLAYER]` prints the top 20,
  timed.

Checked in the browser preview on the published base (425,307 games):
gold setups, the replies to the second most played (55,781 games, 6,037
replies), and the top reply's games (5,932, matching its row).
Not done yet: per-player pages of setups beyond the filter, and partial
setups (only some pieces, such as the elephant and camel; a search by
pieces covers part of it).

### Step 6, second part (2026-11-18): the move tree and position search

`howdah-db`'s `explore.rs`, the board's Explorer and the Games panel's
position filter.

- **One scan for both.** A `PositionQuery` is a whole position (pieces
  and side to move, `Exact`) or a pattern (these pieces on these
  squares, anything else anywhere, either side to move, `Pieces`).
  `Filter::position` holds one; the games the rest of the filter lets
  through are read on the calling thread and replayed on up to 7 more
  (`Replay`: the codec's moves on a `Position` with the raw step
  mechanics, no legality checks, since saving checked them), each until
  it reaches the position or has fewer of some piece than it (captures
  are for good). No index and no derived table, as with the setups.
- **Speed** on the whole archive (425k games, 16-core laptop, release
  build): 250–350 ms for an exact position (most games stop at their
  first capture), 500–600 ms for a pattern of a few pieces, which
  replays most games to their end. The dev bridge was 20 times slower
  until the workspace's dev profile optimized `howdah-arimaa`,
  `howdah-db` and `libsqlite3-sys`; it's now about 0.5 s.
- **Kept scans.** The last 8 scans are kept by filter, while
  `Connection::total_changes` stays the same, so the tree, the game
  list over it and the setups over it share one, and going back a move
  is instant. Any save drops them (a board's own game saves as it
  changes), which is correct and costs a scan only after a change.
- **The move tree** (`Database::explore`): the games reaching the
  position, and the moves played next grouped by the position they lead
  to, so the same steps in another order count as one move (shown as
  its most played spelling). The same statistics as the setup explorer
  (`Tally`), the rating being the side to move's; games that ended at
  the position. Positions are exact: no mirror images, unlike the
  setups, since a move has to be played on the board as it is. At the
  start it lists gold's setups, after them silver's.
- **The game list** with a position: newest first from the scan, a page
  read by id, with `GameSummary::position_ply`, the moves before the
  game first reached it; opening a game there shows that ply
  (`open_library_game`'s `ply`). The setups view with a position counts
  only those games.
- Games from a set position aren't searched, as with the setups.
- **The board's Explorer** (`ExplorerPanel.svelte`, under the comment
  box, folded until opened): the shown position's games and next moves
  (ten, then thirty), with a Player field; a move plays on the board as
  a variation (`add_line`), and Games lists the games reaching it, each
  opening on a board of its own at that ply. It leaves out the board's
  own game (`Filter::exclude`), and is off while the user plays an
  online game, as analysis is (`Session::explorer_position`), and in
  puzzles.
- **The Games panel's position filter:** Position… opens the position
  editor over the panel (`PositionEditor`'s `onSearch`: Find position
  or Find these pieces in place of Analyse and Play, no playable
  position needed); the filter shows the position small, with its match
  (this position, or these pieces), Edit position… and Any position.
- **Down the line from the setups.** In the Games panel's Setups view a
  silver reply opens the moves played after the pair, and each move the
  moves after it, with a breadcrumb of the setups and moves to go back
  and Games for any point (`library_line`: the setup keys and the moves
  in notation, `howdah_db::line_position`). Since the setups count a
  setup and its mirror image as one, so does the line: `Mirrored` matches
  the position or its mirror image, and a mirrored game's next move is
  mirrored too (`mirror`, `mirror_bytes`), so a reply's 8,252 games are
  the line's 8,252 and the moves read as the setups are drawn. Its Games
  sets the list's position with that match ("This position or its
  mirror image"), which the Setups view itself then ignores, as it does
  the setup chips.
- `howdah-db explore DB [--pieces] [MOVES|POSITION]` prints the tree,
  timed.

Checked in the browser preview on the bridge's copy of the archive:
after a common setup pair the tree's 3,940 games and its top move
(2,296 games) agree with the CLI; playing the move from the tree shows
silver's replies to it; the board's game list and opening a game at its
ply (ply 3, the same position); and in the Games panel, the pattern
gold elephant d4, camel e4 (1,355 games won by elimination), whose
game opened at 36g, the first time it stood so. The real app's new
window from a game window's Games list wasn't checked (the browser
preview reuses the tab).

Ideas left: a position's games by player in the tree (beyond the
Player field), mirror images in the board's Explorer, and an index if the archive
grows past where a scan is quick enough.

### Step 7 (2026-11-19): sharing

`howdah-db`'s `share.rs` (export, origins) and `merge.rs` (import), and
the Games panel's Import…, Export… and Sources….

- **Export** (`Database::export`, `ExportOptions`): a new database file
  at this schema holding the games a `Filter` lets through (or chosen
  ids, or none), their players and tags, and as chosen: variations,
  comments (with intros, glyphs and kinds this build doesn't know),
  move times, the chat (off by default), and engine evals (none, the
  positions the games reach, or all; by chosen engines). The move list
  of a game that didn't replay always goes. Whole archive: 3.7 s, a
  player's 792 games 0.03 s.
- **Provenance travels.** Every row keeps its origin, and the export
  carries the `origins` rows its rows use (only those, so it names no
  unrelated file or person), this database's own named with the
  export's author. An import maps them by uuid, adding new ones, so data
  passed on twice still says where it was first written, and this
  database's own data coming back from an export of it is its own again
  (origin 1). An unnamed origin takes the name the import was given
  (the file's name). A user's edit of an imported annotation or eval
  makes it origin 1. Variations keep their origin too (`nodes.origin`,
  migration 5; none is this database's own, as origin 1): saves carry
  it, and the origin of the user's annotations, across the renumbering
  of variations by matching nodes by their moves from the start.
- **Import** (`Database::import_file`): the file is copied beside the
  database with `VACUUM INTO` (so a file in WAL mode comes whole),
  brought up to this schema there, merged and the copy removed; the
  file itself is never changed. The merge (also the base download's):
  - games by `gid`, then source key; a game both have keeps its headers
    and the trees are joined by move path: the other's moves this game
    lacks become variations (`nodes` rows after the existing siblings).
    Only games whose moves, variations or annotations differ are
    looked at, so re-importing the whole archive takes 1.7 s;
  - lines a game here already has from the import's own sources (its
    lines' and comments' origins) that the import's lines don't reach
    are **earlier lines**, such as an earlier analysis by the same
    engine. `EarlierLines` says what to do: `Keep` (beside the new
    ones, as before; the base download and `merge_from`), `Replace`
    (removed, but only lines wholly from those sources: a line the user
    extended or commented on stays), or `Ask` (nothing changes if a game
    has any; `MergeReport::earlier` lists them and `undecided` is set, so
    the caller asks and imports again). Only `Replace` ever deletes;
  - annotations by node, kind and author: added where missing; where
    both differ, the newer edit (the import's on a tie) wins if both
    come from the same origin,
    otherwise this database's stays and the other's is kept beside it
    under its origin's name as the author (`conflict_author`; not shown
    yet, as other authors' evals aren't). Source kinds (times, chat,
    move list) are never replaced, and times only go with an identical
    main line;
  - evals: added where missing, replacing this database's when deeper
    (the number `depth` starts with), or as deep and newer; other kinds
    of position data when newer.
- **Record files** (`Database::import_records`, which the user asked
  for with the file dialogs): the records are saved into a scratch
  database and merged as above, so a record of a game already here
  joins its variations and comments into it. Record games have made-up
  ids, so for them only, games are also matched by fingerprint (moves
  and start) with the same players' names. Matching shared databases by
  fingerprint would join different games with the same moves. Records
  with an `Annotator` tag come from that annotator rather than the file:
  a scratch database each, whose uuid comes from the annotator's name
  (`annotator_uuid`), so every analysis by one engine is one source and
  a newer one finds the earlier one's lines. An analysis should name
  the engine alone (not its version or net), so a newer version's is
  still the same analysis to ask about. With `Ask` the groups
  are only looked at first, and imported only if none has earlier lines.
- **Sources** (`Database::origins`, `remove_origin`, `Filter::from_db`):
  each origin with its counts; the list can be narrowed to what came
  from one (the games it brought or annotated). Removing one deletes its
  annotations, its evals and its games, except games with annotations
  from elsewhere (which become this database's), and its lines on games
  kept here, except those someone else extended or commented on, which
  become this database's. Its row stays, so a second import is still
  known.
- **The app**: `tauri-plugin-dialog` for the system's Open and Save
  dialogs (`lib/files.ts`; the browser preview has none, so it asks for
  a path). Import… takes a game database or a record file (told apart
  by SQLite's header) and says what it did (`importWords`). An import
  that finds earlier lines stops and asks in the panel ("a newer
  analysis by <source> of a game you already have"): Replace earlier lines,
  Keep both or Cancel (`library_import` again with `earlier`). Boards
  showing a joined game take the stored game in (`Session::take_stored`):
  its new lines, and, when the board hasn't changed since the save
  before the import, its newer comments and the removal of replaced
  lines; an edited board only takes in what was added. Export…
  (`ExportDialog.svelte`) exports the list's games with the choices
  above and the user's name (kept in localStorage); Sources…
  (`SourcesDialog.svelte`) lists the sources with Games and Remove; the
  Source filter offers each imported source.
- CLI: `export`, `import-db`, `import-records` (both with
  `--replace-lines` or `--keep-lines`; without either an import that
  finds earlier lines stops and lists them), `origins`,
  `remove-origin`.

Checked in the browser preview on the bridge's copy of the archive: a
shared file's 2 games added; a record file with a new comment and
variation for them joined (1 move and 1 comment added, opened on a
board with both); the list narrowed by source; an export of them with
an eval, its origins as expected; a source removed, keeping the game
another source had annotated. In the real app (Xephyr): Import… and
Export… open the GTK Open and Save dialogs with their filters and name
(cancelled; nothing written).

Earlier lines (2026-11-24), checked in the browser preview on the
bridge's copy of the archive with an engine's quick and final analyses
of one game (`Annotator` set, a line at every move), with a board open on
it: the first joined (260 moves); the second asked in the Games panel
(73 moves of earlier lines); replacing removed those and added the new
ones; switching back and forth twice more did the same each time, and
the board and the database ended with exactly the final analysis's
lines, every line still the engine's.

**Published game database:** a base built at schema 5 is refused by
releases at schema 4 ("needs a newer Howdah"), so the game database's
`howdah-rev` stays before this change until a release with it is out.

Not done: other authors' annotations kept from conflicts aren't shown
or chosen anywhere yet; a board showing a game while an import joins
into it saves its own tree back over the joined variations if it then
changes; nodes have no origin, so removing a source leaves the moves it
added.

