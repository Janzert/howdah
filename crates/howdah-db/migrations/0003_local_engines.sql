-- The app's engines, by their id in its engine list, and the name their
-- data on positions is kept under (the engine's own `id name` and
-- `id version`), so evals can be found again before the engine runs.
-- This computer's alone: not for sharing.
CREATE TABLE local_engines (
    engine_id TEXT PRIMARY KEY,
    author TEXT NOT NULL
) WITHOUT ROWID;
