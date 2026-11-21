-- arimaa.com's games name the server as their site, as online chess
-- does (lichess, chess.com), rather than the archive's "Over the Net",
-- which every one of them had. Games fetched before they reached the
-- archive had none.
UPDATE games SET site = 'arimaa.com'
WHERE source = 'arimaa.com' AND (site IS NULL OR site = 'Over the Net');
