-- Where each variation came from, as annotations already say: the origin
-- of a move added by a merge (a shared file, an engine's analysis as a
-- record), or NULL for this database's own (as origin 1). Saves carry it
-- across the renumbering; a newer analysis from the same origin can
-- replace the lines an earlier one added, and removing a source removes
-- its lines. Older builds read and write the table as before; a game
-- they save loses its lines' origins, which then count as the user's.
ALTER TABLE nodes ADD COLUMN origin INTEGER REFERENCES origins (id);
