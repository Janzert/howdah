-- The game list's order: newest first, by when the game started, else
-- when it ended (a game forfeited before it began), else when it was
-- added (one saved from a board).
CREATE INDEX games_when ON games (coalesce(start_ts, end_ts, added_at));
