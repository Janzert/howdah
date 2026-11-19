//! Position search and the move tree.
//!
//! Both scan the games' move blobs: [`Replay`] plays a blob's moves on a
//! [`Position`] with the raw step mechanics (the moves were checked when
//! they were saved), and each game a [`Filter`] lets through is followed
//! until it reaches the [`PositionQuery`]'s position, or can't any more
//! (it has lost a piece the position still has). A [`PositionQuery`] is
//! either a whole position (the pieces and the side to move), or a
//! pattern: these pieces on these squares, whatever else is on the board
//! and whoever is to move.
//!
//! The scan reads the blobs on the calling thread and replays them on up
//! to 7 more, and its result is kept for the next few queries until the
//! database changes ([`Connection::total_changes`]), so the move tree,
//! the game list and the setups over the same position share one scan.
//! [`Database::explore`] gives the move tree: the games reaching a
//! position and, grouped by the position each leads to, the moves played
//! next. [`Filter::position`] narrows the game list to the games
//! reaching it, with the ply where each first did.
//!
//! Games from a set position aren't searched (as with the setups).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, mpsc};

use howdah_arimaa::{Color, Dir, Move, Piece, PieceKind, Placement, Position, Square, Step};
use rusqlite::types::ValueRef;

use crate::codec::FORMAT;
use crate::search::{Filter, conditions};
use crate::setups::{Setup, expected};
use crate::{Database, Result};

/// How a [`PositionQuery`] matches.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PositionMatch {
    /// The same pieces on the same squares, nothing else, and the same
    /// side to move.
    Exact,
    /// The position's pieces on their squares, anything on the other
    /// squares, either side to move.
    Pieces,
    /// As [`PositionMatch::Exact`], or its mirror image (files a–h
    /// swapped), as the setup explorer counts setups. A game reaching
    /// the mirror image counts as reaching the position, its next move
    /// mirrored too.
    Mirrored,
}

/// A position to find in the games.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PositionQuery {
    pub position: Position,
    pub mode: PositionMatch,
}

impl PositionQuery {
    pub fn exact(position: Position) -> PositionQuery {
        PositionQuery { position, mode: PositionMatch::Exact }
    }

    pub fn pieces(position: Position) -> PositionQuery {
        PositionQuery { position, mode: PositionMatch::Pieces }
    }

    pub fn mirrored(position: Position) -> PositionQuery {
        PositionQuery { position, mode: PositionMatch::Mirrored }
    }
}

/// The position after a gold setup and silver's reply ([`Setup`] keys,
/// as the setup explorer gives them) and then `moves` in notation, with
/// the moves as played (`Move::notation`).
pub fn line_position(gold: &str, reply: &str, moves: &[String]) -> std::result::Result<Position, String> {
    use howdah_arimaa::GameTree;
    let setup = |key: &str| Setup::parse(key).ok_or_else(|| format!("{key:?} isn't a setup"));
    let mut tree = GameTree::new();
    let mut at = tree.root();
    for (key, color) in [(gold, Color::Gold), (reply, Color::Silver)] {
        at = tree.add_setup(at, setup(key)?.placements(color)).map_err(|e| format!("{key}: {e}"))?;
    }
    for m in moves {
        at = tree.add_notation(at, m).map_err(|e| format!("{m}: {e}"))?;
    }
    Ok(tree[at].position().clone())
}

/// A position mirrored left to right (files a–h swapped).
pub fn mirror(pos: &Position) -> Position {
    let pieces = pos.pieces().map(|(sq, p)| (p, Square::new(sq.index() ^ 7).expect("on the board")));
    Position::from_pieces(pos.side_to_move(), pieces).expect("a mirror image has no clashes")
}

/// A move in the codec mirrored left to right: a setup's nibbles reversed
/// within each rank, a step's square mirrored and east and west swapped.
fn mirror_bytes(bytes: &[u8], setup: bool) -> Vec<u8> {
    if setup {
        let mut n: Vec<u8> =
            (0..16).map(|i| bytes.get(i / 2).map_or(0, |b| b >> (4 * (i % 2)) & 0xF)).collect();
        n[..8].reverse();
        n[8..].reverse();
        return n.chunks(2).map(|p| p[0] | p[1] << 4).collect();
    }
    let mut out = bytes.to_vec();
    for b in out.iter_mut().skip(1) {
        let dir = match *b & 3 {
            2 => 3,
            3 => 2,
            d => d,
        };
        *b = ((*b >> 2) ^ 7) << 2 | dir;
    }
    out
}

const PIECES: [Piece; 12] = {
    let mut out = [Piece::new(Color::Gold, PieceKind::Rabbit); 12];
    let mut i = 0;
    while i < 12 {
        let color = if i < 6 { Color::Gold } else { Color::Silver };
        out[i] = Piece::new(color, PieceKind::ALL[i % 6]);
        i += 1;
    }
    out
};

/// A [`PositionQuery`] ready to test positions against.
struct Matcher {
    target: Position,
    hash: u64,
    exact: bool,
    /// The target's mirror image, when it counts and differs.
    mirror: Option<(u64, Position)>,
    boards: [u64; 12],
    counts: [u32; 12],
}

impl Matcher {
    fn new(q: &PositionQuery) -> Matcher {
        let boards = PIECES.map(|p| q.position.bitboard(p));
        Matcher {
            hash: q.position.hash(),
            exact: q.mode != PositionMatch::Pieces,
            mirror: (q.mode == PositionMatch::Mirrored)
                .then(|| mirror(&q.position))
                .filter(|m| *m != q.position)
                .map(|m| (m.hash(), m)),
            counts: boards.map(u64::count_ones),
            boards,
            target: q.position.clone(),
        }
    }

    /// Whether `pos` matches: `Some(true)` as the target's mirror image.
    fn matches(&self, pos: &Position) -> Option<bool> {
        if !self.exact {
            return PIECES
                .iter()
                .zip(&self.boards)
                .all(|(&p, &b)| b & !pos.bitboard(p) == 0)
                .then_some(false);
        }
        let h = pos.hash();
        if h == self.hash && *pos == self.target {
            return Some(false);
        }
        self.mirror.as_ref().is_some_and(|(mh, m)| h == *mh && pos == m).then_some(true)
    }

    /// Whether a game at `pos` can still reach the target: captures are
    /// for good, so it can't once it has fewer of a piece than the target.
    fn reachable(&self, pos: &Position) -> bool {
        PIECES.iter().zip(&self.counts).all(|(&p, &n)| pos.count(p) >= n)
    }
}

/// Plays a line blob's moves (see [`crate::codec`]) from the empty board
/// with the raw step mechanics: no legality checks, since the moves were
/// checked when they were saved.
pub struct Replay<'a> {
    bytes: &'a [u8],
    at: usize,
    pub position: Position,
    /// Moves played so far.
    pub ply: usize,
}

/// One move a [`Replay`] played.
pub struct Played<'a> {
    /// Its bytes in the codec.
    pub bytes: &'a [u8],
    /// Whether it captured anything.
    pub captured: bool,
}

impl<'a> Replay<'a> {
    /// A replay of a game from the setups (`None` for an unknown format).
    pub fn new(blob: &'a [u8]) -> Option<Replay<'a>> {
        (blob.first() == Some(&FORMAT)).then(|| Replay {
            bytes: blob,
            at: 1,
            position: Position::empty(Color::Gold),
            ply: 0,
        })
    }

    /// Plays the next move: `None` at the end or at a move that can't be
    /// played.
    pub fn next_move(&mut self) -> Option<Played<'a>> {
        let start = self.at;
        let rest = &self.bytes[start..];
        let side = self.position.side_to_move();
        let mut captured = false;
        if self.ply < 2 {
            let setup = rest.get(..8)?;
            for (i, sq) in home_squares(side).enumerate() {
                let nibble = setup[i / 2] >> (4 * (i % 2)) & 0xF;
                if nibble != 0 {
                    let kind = *PieceKind::ALL.get(nibble as usize - 1)?;
                    self.position.set(sq, Some(Piece::new(side, kind)));
                }
            }
            self.at += 8;
        } else {
            let count = *rest.first()? as usize;
            let steps = rest.get(1..1 + count)?;
            for &b in steps {
                let from = Square::new(b >> 2)?;
                let piece = self.position.piece_at(from)?;
                let effect =
                    self.position.apply_step(Step::new(piece, from, Dir::ALL[(b & 3) as usize])).ok()?;
                captured |= effect.capture.is_some();
            }
            self.at += 1 + count;
        }
        self.position.set_side_to_move(side.opponent());
        self.ply += 1;
        Some(Played { bytes: &self.bytes[start..self.at], captured })
    }
}

fn home_squares(color: Color) -> impl Iterator<Item = Square> {
    let first = if color == Color::Gold { 0 } else { 48 };
    (first..first + 16).filter_map(Square::new)
}

/// A move in the codec as a [`Move`], played from `pos`.
fn decode_move(pos: &Position, bytes: &[u8], setup: bool) -> Option<Move> {
    let side = pos.side_to_move();
    if setup {
        let placements = home_squares(side)
            .enumerate()
            .filter_map(|(i, square)| {
                let nibble = bytes.get(i / 2)? >> (4 * (i % 2)) & 0xF;
                let kind = *PieceKind::ALL.get((nibble as usize).checked_sub(1)?)?;
                Some(Placement { piece: Piece::new(side, kind), square })
            })
            .collect();
        return Some(Move::Setup(placements));
    }
    let mut p = pos.clone();
    let mut effects = Vec::new();
    for &b in bytes.get(1..)? {
        let from = Square::new(b >> 2)?;
        let piece = p.piece_at(from)?;
        effects.push(p.apply_step(Step::new(piece, from, Dir::ALL[(b & 3) as usize])).ok()?);
    }
    Some(if effects.is_empty() { Move::Pass } else { Move::Steps(effects) })
}

/// A game reaching the position.
#[derive(Clone, Debug)]
pub(crate) struct Hit {
    pub id: i64,
    /// The moves before the position (setups included).
    pub ply: usize,
    /// The move played next and the position after it, by hash.
    pub next: Option<(Vec<u8>, u64)>,
    pub winner: Option<char>,
    pub ratings: [Option<i64>; 2],
    pub when: i64,
}

/// One game's row as the scan reads it.
struct Row {
    id: i64,
    moves: Vec<u8>,
    winner: Option<char>,
    ratings: [Option<i64>; 2],
    when: i64,
}

fn find(m: &Matcher, row: Row) -> Option<Hit> {
    let mut replay = Replay::new(&row.moves)?;
    let found = |replay: &Replay, next: Option<(Vec<u8>, u64)>| Hit {
        id: row.id,
        ply: replay.ply,
        next,
        winner: row.winner,
        ratings: row.ratings,
        when: row.when,
    };
    // The move after a match, and then the position after it (both
    // mirrored for a match of the mirror image).
    let mut at_match: Option<(usize, bool)> = None;
    loop {
        if at_match.is_none() {
            at_match = m.matches(&replay.position).map(|mirrored| (replay.ply, mirrored));
        }
        let played = replay.next_move();
        if let Some((ply, mirrored)) = at_match {
            let next = played.map(|p| match mirrored {
                false => (p.bytes.to_vec(), replay.position.hash()),
                true => (mirror_bytes(p.bytes, ply < 2), mirror(&replay.position).hash()),
            });
            let mut hit = found(&replay, next);
            hit.ply = ply;
            return Some(hit);
        }
        let played = played?;
        if played.captured && !m.reachable(&replay.position) {
            return None;
        }
    }
}

/// How many scans [`Database`] keeps.
const KEEP: usize = 8;

/// Kept scans: the filter (with its position), the database's change
/// count when made, and the games found.
#[derive(Default)]
pub(crate) struct ScanCache {
    scans: Vec<(Filter, u64, Arc<Vec<Hit>>)>,
}

impl Database {
    /// The games `filter` lets through that reach its position, in no
    /// particular order. Without a position, none.
    pub(crate) fn scan(&self, filter: &Filter) -> Result<Arc<Vec<Hit>>> {
        let Some(query) = &filter.position else { return Ok(Arc::new(Vec::new())) };
        let changes = self.connection().total_changes();
        {
            let mut cache = self.scan_cache.borrow_mut();
            if let Some(i) = cache.scans.iter().position(|(f, c, _)| f == filter && *c == changes) {
                let kept = cache.scans.remove(i);
                let hits = kept.2.clone();
                cache.scans.push(kept);
                return Ok(hits);
            }
        }
        let hits = Arc::new(self.scan_games(filter, query)?);
        let mut cache = self.scan_cache.borrow_mut();
        cache.scans.retain(|(_, c, _)| *c == changes);
        if cache.scans.len() >= KEEP {
            cache.scans.remove(0);
        }
        cache.scans.push((filter.clone(), changes, hits.clone()));
        Ok(hits)
    }

    fn scan_games(&self, filter: &Filter, query: &PositionQuery) -> Result<Vec<Hit>> {
        let (mut clauses, params) = conditions(filter);
        clauses.push("moves IS NOT NULL AND start_ply = 0".into());
        let sql = format!(
            "SELECT id, moves, winner, gold_rating, silver_rating, coalesce(start_ts, end_ts, added_at)
             FROM games WHERE {}",
            clauses.join(" AND ")
        );
        let matcher = Matcher::new(query);
        let workers = std::thread::available_parallelism().map_or(2, |n| n.get()).clamp(2, 8) - 1;
        let (send, receive) = mpsc::sync_channel::<Vec<Row>>(workers * 2);
        let receive = Mutex::new(receive);
        std::thread::scope(|s| {
            let handles: Vec<_> = (0..workers)
                .map(|_| {
                    s.spawn(|| {
                        let mut hits = Vec::new();
                        loop {
                            let batch = match receive.lock().expect("the scan's queue").recv() {
                                Ok(b) => b,
                                Err(_) => return hits,
                            };
                            hits.extend(batch.into_iter().filter_map(|row| find(&matcher, row)));
                        }
                    })
                })
                .collect();
            let read = (|| -> Result<()> {
                let mut q = self.connection().prepare(&sql)?;
                let mut rows = q.query(rusqlite::params_from_iter(params.iter()))?;
                let mut batch = Vec::with_capacity(BATCH);
                while let Some(r) = rows.next()? {
                    let ValueRef::Blob(moves) = r.get_ref(1)? else { continue };
                    let winner: Option<String> = r.get(2)?;
                    batch.push(Row {
                        id: r.get(0)?,
                        moves: moves.to_vec(),
                        winner: winner.and_then(|w| w.chars().next()),
                        ratings: [r.get(3)?, r.get(4)?],
                        when: r.get(5)?,
                    });
                    if batch.len() == BATCH {
                        // The workers outlive the reader, so this can't fail.
                        let _ = send.send(std::mem::replace(&mut batch, Vec::with_capacity(BATCH)));
                    }
                }
                let _ = send.send(batch);
                Ok(())
            })();
            drop(send);
            let hits: Vec<Hit> = handles.into_iter().flat_map(|h| h.join().expect("a scan thread")).collect();
            read.map(|()| hits)
        })
    }

    /// The move tree at `filter`'s position: the games reaching it and
    /// the moves played next, the `limit` most played. The position's
    /// match should be [`PositionMatch::Exact`]; with a pattern the
    /// moves are grouped all the same, by the position they lead to.
    pub fn explore(&self, filter: &Filter, limit: usize) -> Result<Explorer> {
        let hits = self.scan(filter)?;
        let Some(query) = &filter.position else { return Ok(Explorer::default()) };
        let mover = query.position.side_to_move();
        // A setup is due when the side to move has nothing on the board.
        let setup = query.position.occupied_by(mover) == 0;
        let mut all = Tally::default();
        // By the position a move leads to: its spellings (the steps in
        // different orders) and how often each was played.
        let mut by_next: HashMap<u64, (HashMap<&[u8], i64>, Tally)> = HashMap::new();
        for h in hits.iter() {
            all.add(h, mover);
            match &h.next {
                Some((bytes, key)) => {
                    let (spellings, tally) = by_next.entry(*key).or_default();
                    *spellings.entry(bytes.as_slice()).or_default() += 1;
                    tally.add(h, mover);
                }
                None => all.ended += 1,
            }
        }
        let distinct = by_next.len() as i64;
        let mut moves: Vec<_> = by_next.into_values().collect();
        moves.sort_by(|a, b| b.1.games.cmp(&a.1.games).then(b.1.last.cmp(&a.1.last)));
        let exact = query.mode != PositionMatch::Pieces;
        let moves = moves
            .into_iter()
            .take(limit)
            .filter_map(|(spellings, tally)| {
                // The most played spelling, and of those the first in byte order.
                let bytes = spellings.into_iter().max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(a.0)))?.0;
                let notation = if exact {
                    decode_move(&query.position, bytes, setup)?.notation()
                } else {
                    String::new()
                };
                Some(MoveStat { notation, tally: tally.finish() })
            })
            .collect();
        Ok(Explorer { games: all.finish(), moves, distinct })
    }

    /// The games `filter` lets through that reach its position, newest
    /// first, `limit` after `offset`, as (id, ply), and how many there are.
    pub(crate) fn position_page(
        &self,
        filter: &Filter,
        offset: i64,
        limit: i64,
    ) -> Result<(Vec<(i64, usize)>, i64)> {
        let hits = self.scan(filter)?;
        let mut order: Vec<&Hit> = hits.iter().collect();
        order.sort_by(|a, b| b.when.cmp(&a.when).then(b.id.cmp(&a.id)));
        let page = order
            .iter()
            .skip(offset.max(0) as usize)
            .take(limit.max(0) as usize)
            .map(|h| (h.id, h.ply))
            .collect();
        Ok((page, hits.len() as i64))
    }
}

const BATCH: usize = 2048;

/// Results over some games.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tally {
    pub games: i64,
    pub gold_wins: i64,
    pub silver_wins: i64,
    /// Games with both players' ratings, gold's wins among them, and the
    /// wins gold's rating predicted.
    pub rated_games: i64,
    pub rated_gold_wins: i64,
    pub expected_gold_wins: f64,
    /// The average rating of the side to move at the position.
    pub rating: Option<f64>,
    /// Unix seconds: the newest game.
    pub last: i64,
    /// The newest game's date, `YYYY.MM.DD` (UTC).
    pub last_date: String,
    /// Games that ended at the position (only in [`Explorer::games`]).
    pub ended: i64,
    rating_sum: f64,
    rating_n: i64,
}

impl Tally {
    fn add(&mut self, h: &Hit, mover: Color) {
        self.games += 1;
        let gold_won = h.winner == Some('g');
        self.gold_wins += i64::from(gold_won);
        self.silver_wins += i64::from(h.winner == Some('s'));
        if let [Some(g), Some(s)] = h.ratings
            && g > 0
            && s > 0
        {
            self.rated_games += 1;
            self.rated_gold_wins += i64::from(gold_won);
            self.expected_gold_wins += expected(g, s);
        }
        if let Some(r) = h.ratings[mover.index()].filter(|&r| r > 0) {
            self.rating_sum += r as f64;
            self.rating_n += 1;
        }
        self.last = self.last.max(h.when);
    }

    fn finish(mut self) -> Tally {
        self.rating = (self.rating_n > 0).then(|| self.rating_sum / self.rating_n as f64);
        self.last_date = if self.games > 0 { crate::game::utc_date(self.last) } else { String::new() };
        self
    }
}

/// A move played from the position.
#[derive(Clone, Debug, PartialEq)]
pub struct MoveStat {
    /// The move's notation (empty for a pattern's moves).
    pub notation: String,
    pub tally: Tally,
}

/// The move tree at one position.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Explorer {
    /// Every game reaching the position.
    pub games: Tally,
    /// The moves played next, most played first.
    pub moves: Vec<MoveStat>,
    /// Distinct moves (by the position they lead to).
    pub distinct: i64,
}

#[cfg(test)]
mod tests {
    use howdah_arimaa::GameRecord;

    use super::*;
    use crate::{GameData, StoredResult};

    const SETUPS: &str = "1g Ra1 Rb1 Rc1 Rd1 Re1 Rf1 Rg1 Rh1 Da2 Hb2 Cc2 Ed2 Me2 Hf2 Cg2 Dh2\n\
        1s ch7 hg7 df7 me7 ed7 dc7 hb7 ca7 rh8 rg8 rf8 re8 rd8 rc8 rb8 ra8\n";

    fn game(moves: &str, winner: char) -> GameData {
        let record = GameRecord::parse(&format!("{SETUPS}{moves}")).unwrap();
        let mut g = GameData::from_record(&record);
        g.players[0].rating = Some(1500);
        g.players[1].rating = Some(1500);
        g.result = Some(StoredResult { winner, termination: Some('r') });
        g
    }

    /// The position after a game's first `plies` moves.
    fn after(moves: &str, plies: usize) -> Position {
        let record = GameRecord::parse(&format!("{SETUPS}{moves}")).unwrap();
        let line = record.tree.main_line();
        record.tree[line[plies]].position().clone()
    }

    fn at(position: Position) -> Filter {
        Filter { position: Some(PositionQuery::exact(position)), ..Default::default() }
    }

    const A: &str = "2g Ed2n Ed3n Me2n Me3n\n2s ed7s ed6s me7s me6s\n";
    /// The same position after 2g by other steps, then another reply.
    const B: &str = "2g Me2n Ed2n Ed3n Me3n\n2s hb7s hb6s ed7s ed6s\n";
    const C: &str = "2g Da2n Da3n Hb2n Hb3n\n";

    #[test]
    fn the_tree_groups_transpositions_and_counts_results() {
        let mut db = Database::open_in_memory().unwrap();
        for (moves, w) in [(A, 'g'), (B, 's'), (C, 'g'), (A, 'g')] {
            db.save(&game(moves, w)).unwrap();
        }
        // After the setups: every game, three different moves... two, since
        // A and B's first turns end in the same position.
        let tree = db.explore(&at(after(A, 2)), 10).unwrap();
        assert_eq!((tree.games.games, tree.games.gold_wins, tree.distinct), (4, 3, 2));
        assert_eq!(tree.moves[0].notation, "Ed2n Ed3n Me2n Me3n");
        assert_eq!((tree.moves[0].tally.games, tree.moves[0].tally.silver_wins), (3, 1));
        assert_eq!(tree.moves[1].notation, "Da2n Da3n Hb2n Hb3n");

        let tree = db.explore(&at(after(A, 3)), 10).unwrap();
        let replies: Vec<_> = tree.moves.iter().map(|m| (m.notation.as_str(), m.tally.games)).collect();
        assert_eq!(replies, [("ed7s ed6s me7s me6s", 2), ("hb7s hb6s ed7s ed6s", 1)]);

        // The end of a game: reached, nothing played after.
        let tree = db.explore(&at(after(C, 3)), 10).unwrap();
        assert_eq!((tree.games.games, tree.games.ended, tree.moves.len()), (1, 1, 0));

        // The empty board: every game, gold's setups.
        let tree = db.explore(&at(Position::empty(Color::Gold)), 10).unwrap();
        assert_eq!((tree.games.games, tree.distinct), (4, 1));
        assert!(tree.moves[0].notation.starts_with("Ra1 "), "{}", tree.moves[0].notation);
    }

    #[test]
    fn the_list_finds_games_by_position_or_pieces() {
        let mut db = Database::open_in_memory().unwrap();
        let ids: Vec<i64> =
            [(A, 'g'), (B, 's'), (C, 'g')].iter().map(|(m, w)| db.save(&game(m, *w)).unwrap()).collect();
        let page = db.search(&at(after(A, 3)), 0, 10).unwrap();
        assert_eq!(page.total, 2);
        let mut found: Vec<(i64, Option<i64>)> = page.games.iter().map(|g| (g.id, g.position_ply)).collect();
        found.sort();
        assert_eq!(found, [(ids[0], Some(3)), (ids[1], Some(3))]);

        // The elephant on d4 and the camel on e4, anything else anywhere:
        // after 2g in A and B, and not at all in C.
        let pattern = Position::from_pieces(
            Color::Gold,
            [
                (Piece::new(Color::Gold, PieceKind::Elephant), "d4".parse().unwrap()),
                (Piece::new(Color::Gold, PieceKind::Camel), "e4".parse().unwrap()),
            ],
        )
        .unwrap();
        let f = Filter { position: Some(PositionQuery::pieces(pattern)), ..Default::default() };
        let page = db.search(&f, 0, 10).unwrap();
        assert_eq!(page.total, 2);
        assert!(page.games.iter().all(|g| g.position_ply == Some(3)));

        // With a player filter as well, and the setups over the same games.
        let f = Filter { termination: Some('r'), ..f };
        assert_eq!(db.search(&f, 0, 10).unwrap().total, 2);
        let setups = db.setup_stats(&f, &crate::SetupLevel::Gold, 10).unwrap();
        assert_eq!(setups.games, 2);
    }

    #[test]
    fn mirror_images_count_as_one_with_their_moves_mirrored() {
        // A's setups mirrored, then A's first turn mirrored.
        const SETUPS_M: &str = "1g Ra1 Rb1 Rc1 Rd1 Re1 Rf1 Rg1 Rh1 Dh2 Hg2 Cf2 Ee2 Md2 Hc2 Cb2 Da2\n\
            1s cA7 hb7 dc7 md7 ee7 df7 hg7 ch7 ra8 rb8 rc8 rd8 re8 rf8 rg8 rh8\n";
        let mut db = Database::open_in_memory().unwrap();
        db.save(&game(A, 'g')).unwrap();
        db.save(&game(C, 's')).unwrap();
        let record =
            GameRecord::parse(&format!("{}2g Ee2n Ee3n Md2n Md3n\n", SETUPS_M.replace("cA7", "ca7")))
                .unwrap();
        let mut g = GameData::from_record(&record);
        g.result = Some(StoredResult { winner: 'g', termination: Some('r') });
        db.save(&g).unwrap();

        let start = after(A, 2);
        let only = db.explore(&at(start.clone()), 10).unwrap();
        assert_eq!(only.games.games, 2, "the mirrored game isn't the same position");
        let f = Filter { position: Some(PositionQuery::mirrored(start.clone())), ..Default::default() };
        let both = db.explore(&f, 10).unwrap();
        assert_eq!((both.games.games, both.distinct), (3, 2));
        assert_eq!((both.moves[0].notation.as_str(), both.moves[0].tally.games), ("Ed2n Ed3n Me2n Me3n", 2));
        assert_eq!(db.search(&f, 0, 10).unwrap().total, 3);

        // Down the line from the setup explorer's keys: they have gold's
        // elephant on e2, so A's line is seen mirrored.
        let gold =
            db.setup_stats(&Filter::default(), &crate::SetupLevel::Gold, 1).unwrap().rows[0].setup.clone();
        let reply =
            db.setup_stats(&Filter::default(), &crate::SetupLevel::Replies(gold.clone()), 1).unwrap().rows[0]
                .setup
                .clone();
        let pos = line_position(&gold, &reply, &[]).unwrap();
        assert_eq!(pos, mirror(&start));
        let f = Filter { position: Some(PositionQuery::mirrored(pos)), ..Default::default() };
        let tree = db.explore(&f, 10).unwrap();
        assert_eq!(tree.games.games, 3);
        assert_eq!(tree.moves[0].notation, "Ee2n Ee3n Md2n Md3n", "seen as the keys have it");
        let next = line_position(&gold, &reply, &[tree.moves[0].notation.clone()]).unwrap();
        assert_eq!(next, mirror(&after(A, 3)));
    }

    #[test]
    fn a_kept_scan_is_dropped_when_the_games_change() {
        let mut db = Database::open_in_memory().unwrap();
        db.save(&game(A, 'g')).unwrap();
        let f = at(after(A, 3));
        assert_eq!(db.search(&f, 0, 10).unwrap().total, 1);
        db.save(&game(B, 'g')).unwrap();
        assert_eq!(db.search(&f, 0, 10).unwrap().total, 2);
    }
}
