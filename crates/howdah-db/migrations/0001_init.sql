-- Howdah's game database, first schema. The design is in docs/DATABASE.md.
-- Plain SQLite 3 only: no STRICT tables, JSONB or generated columns, so
-- older SQLite builds and other tools can read the file.

-- Facts about this database: its id (`uuid`, 32 hex digits), when it was
-- made, and the oldest schema version that can still read it
-- (`min_reader`).
CREATE TABLE meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
) WITHOUT ROWID;

-- The databases data came from. Row 1 is this database itself; an import
-- adds a row for each database it merges in, so what came from it can be
-- filtered or removed later.
CREATE TABLE origins (
    id INTEGER PRIMARY KEY,
    uuid BLOB NOT NULL UNIQUE,
    name TEXT NOT NULL DEFAULT '',
    added_at INTEGER NOT NULL
);

-- Players. `site` and `site_id` key a player on a server (`arimaa.com` and
-- its player id); local players (engines, the user) have neither.
-- `kind` is 'h' (human) or 'b' (bot).
CREATE TABLE players (
    id INTEGER PRIMARY KEY,
    site TEXT,
    site_id TEXT,
    name TEXT NOT NULL,
    kind TEXT,
    country TEXT,
    title TEXT,
    UNIQUE (site, site_id)
);
CREATE INDEX players_name ON players (name);

-- Every name a player has been seen under, the current one included.
CREATE TABLE player_names (
    player INTEGER NOT NULL REFERENCES players (id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    PRIMARY KEY (player, name)
) WITHOUT ROWID;
CREATE INDEX player_names_name ON player_names (name);

-- Games: the headers every game has as columns, the main line as one blob.
CREATE TABLE games (
    id INTEGER PRIMARY KEY,
    -- Random, 16 bytes; the same in every database the game is copied to.
    gid BLOB NOT NULL UNIQUE,
    -- Where the game is from, for games with a key there
    -- (`arimaa.com` and its permanent id).
    source TEXT,
    source_id TEXT,
    origin INTEGER NOT NULL REFERENCES origins (id),
    gold INTEGER REFERENCES players (id),
    silver INTEGER REFERENCES players (id),
    -- The names as the game had them, which can differ from the players'
    -- names now.
    gold_name TEXT,
    silver_name TEXT,
    -- Ratings before the game, and their uncertainty where the server
    -- gives one (arimaa.com's RU).
    gold_rating INTEGER,
    silver_rating INTEGER,
    gold_rating_k INTEGER,
    silver_rating_k INTEGER,
    event TEXT,
    site TEXT,
    time_control TEXT,
    rated INTEGER,
    postal INTEGER,
    -- `YYYY.MM.DD`, as the record's Date tag.
    date TEXT,
    -- Unix seconds.
    start_ts INTEGER,
    end_ts INTEGER,
    -- 'g' or 's' for the winner, or old arimaa.com's 'd' (draw) and 'u'
    -- (unknown); NULL while there's no result.
    winner TEXT,
    -- The ending's letter (RESULT-CODES.md), as the source gives it.
    termination TEXT,
    -- Moves on the main line, setups included.
    plies INTEGER NOT NULL DEFAULT 0,
    -- For a game from a set position: the absolute ply of its first
    -- position (2 for 2g, 3 for 2s) and the board in the short format.
    start_ply INTEGER NOT NULL DEFAULT 0,
    start_board TEXT,
    -- 'old' for arimaa.com's rules before 2008.07.01 (play went on after
    -- an elimination, and a third repetition lost), NULL for today's.
    rules TEXT,
    -- The main line (codec.rs). NULL for a game known only by its header
    -- (a row from a server's game list), whose moves aren't fetched yet.
    moves BLOB,
    -- A hash of the start and the moves, to find a game imported without
    -- an id.
    fingerprint BLOB,
    -- Set when moves after the main line's last could not be replayed;
    -- the original move list is then kept as a `movelist` annotation.
    replay_error TEXT,
    added_at INTEGER NOT NULL,
    modified_at INTEGER NOT NULL,
    fetched_at INTEGER,
    UNIQUE (source, source_id)
);
CREATE INDEX games_gold ON games (gold);
CREATE INDEX games_silver ON games (silver);
CREATE INDEX games_start ON games (start_ts);
CREATE INDEX games_date ON games (date);
CREATE INDEX games_fingerprint ON games (fingerprint);

-- Every other tag a game's record had, in order.
CREATE TABLE game_tags (
    game INTEGER NOT NULL REFERENCES games (id) ON DELETE CASCADE,
    ord INTEGER NOT NULL,
    name TEXT NOT NULL,
    value TEXT NOT NULL,
    PRIMARY KEY (game, ord)
) WITHOUT ROWID;

-- Nodes off the main line. A node reference is a main-line ply (0 for
-- the start) when it's zero or more, and `-id` of a row here when it's
-- negative. `ord` is the node's place among its parent's children.
CREATE TABLE nodes (
    game INTEGER NOT NULL REFERENCES games (id) ON DELETE CASCADE,
    id INTEGER NOT NULL,
    parent INTEGER NOT NULL,
    ord INTEGER NOT NULL,
    move BLOB NOT NULL,
    PRIMARY KEY (game, id)
) WITHOUT ROWID;

-- Data on a game (node 0, the start) or on a node: a comment, glyphs, the
-- move times, a chat, and kinds to come. One per node, kind and author;
-- an empty author is the database's own user. Payloads are written in one
-- canonical form, so they could be signed later.
CREATE TABLE annotations (
    game INTEGER NOT NULL REFERENCES games (id) ON DELETE CASCADE,
    node INTEGER NOT NULL,
    kind TEXT NOT NULL,
    author TEXT NOT NULL DEFAULT '',
    origin INTEGER NOT NULL REFERENCES origins (id),
    created_at INTEGER NOT NULL,
    modified_at INTEGER NOT NULL,
    payload BLOB NOT NULL,
    PRIMARY KEY (game, node, kind, author)
) WITHOUT ROWID;

-- Data on a position, shared by every game and line that reaches it
-- (engine evals): keyed by the position's Zobrist hash, side to move
-- included.
CREATE TABLE position_data (
    hash INTEGER NOT NULL,
    kind TEXT NOT NULL,
    author TEXT NOT NULL DEFAULT '',
    origin INTEGER NOT NULL REFERENCES origins (id),
    created_at INTEGER NOT NULL,
    modified_at INTEGER NOT NULL,
    payload BLOB NOT NULL,
    PRIMARY KEY (hash, kind, author)
) WITHOUT ROWID;

-- Archive files imported, so a rerun skips the ones unchanged.
CREATE TABLE archive_files (
    name TEXT PRIMARY KEY,
    size INTEGER NOT NULL,
    mtime INTEGER NOT NULL,
    games INTEGER NOT NULL,
    imported_at INTEGER NOT NULL
) WITHOUT ROWID;
