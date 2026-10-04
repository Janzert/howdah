//! A game tree: moves with variations.
//!
//! Each node is the position after a move, and the root is the empty board
//! with gold to set up (ply 0). A node's first child continues the main
//! line; the others are variations, in the order they're shown. Node ids
//! are stable for the life of the tree and never reused, so the UI can
//! refer to a move by id. See `docs/VARIATIONS.md` for the design.

use crate::error::GameError;
use crate::game::{Game, Move, build_turn};
use crate::notation::{self, MoveBody};
use crate::outcome::{GameResult, outcome_after_turn};
use crate::position::Position;
use crate::setup::{Placement, apply_setup};
use crate::turn::{Turn, TurnBuilder};
use crate::types::{Color, data_type};

data_type! {
    /// Identifies a node in a [`GameTree`].
    pub struct NodeId(pub u32);
}

impl NodeId {
    fn index(self) -> usize {
        self.0 as usize
    }
}

data_type! {
    /// A numeric annotation glyph, numbered as in PGN (`$1` = `!`).
    pub struct Glyph(pub u8);
}

/// The PGN glyphs that have a symbol: 1 to 6 are the move glyphs.
const GLYPH_SYMBOLS: [(u8, &str); 6] = [(1, "!"), (2, "?"), (3, "!!"), (4, "??"), (5, "!?"), (6, "?!")];

/// Glyphs that judge the same thing replace each other.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GlyphClass {
    /// `$1`-`$9`: how good the move is.
    Move,
    /// `$10`-`$19`: who stands better.
    Position,
    Other,
}

impl Glyph {
    pub const GOOD: Glyph = Glyph(1);
    pub const MISTAKE: Glyph = Glyph(2);
    pub const BRILLIANT: Glyph = Glyph(3);
    pub const BLUNDER: Glyph = Glyph(4);
    pub const INTERESTING: Glyph = Glyph(5);
    pub const DUBIOUS: Glyph = Glyph(6);

    /// The symbol for a move glyph (`!`, `?!`, ...), if it has one.
    pub fn symbol(self) -> Option<&'static str> {
        GLYPH_SYMBOLS.iter().find(|(n, _)| *n == self.0).map(|(_, s)| *s)
    }

    /// Reads a symbol (`!?`) or a numbered glyph (`$14`).
    pub fn parse(text: &str) -> Option<Glyph> {
        if let Some(n) = text.strip_prefix('$') {
            return n.parse().ok().map(Glyph);
        }
        GLYPH_SYMBOLS.iter().find(|(_, s)| *s == text).map(|(n, _)| Glyph(*n))
    }

    fn class(self) -> GlyphClass {
        match self.0 {
            1..=9 => GlyphClass::Move,
            10..=19 => GlyphClass::Position,
            _ => GlyphClass::Other,
        }
    }
}

impl std::fmt::Display for Glyph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.symbol() {
            Some(s) => f.write_str(s),
            None => write!(f, "${}", self.0),
        }
    }
}

/// A comment and glyphs on a move. The root's annotation is the game comment.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Annotation {
    /// Comment after the move.
    pub comment: Option<String>,
    /// Introduction to a variation, shown before its first move. Only kept
    /// on a variation's first move.
    pub intro: Option<String>,
    pub glyphs: Vec<Glyph>,
}

impl Annotation {
    pub fn is_empty(&self) -> bool {
        self.comment.is_none() && self.intro.is_none() && self.glyphs.is_empty()
    }

    /// Adds a glyph, replacing one of the same kind: a move can't be both
    /// `!` and `?`.
    pub fn set_glyph(&mut self, glyph: Glyph) {
        let class = glyph.class();
        if class != GlyphClass::Other {
            self.glyphs.retain(|g| g.class() != class);
        }
        if !self.glyphs.contains(&glyph) {
            self.glyphs.push(glyph);
        }
    }

    pub fn remove_glyph(&mut self, glyph: Glyph) {
        self.glyphs.retain(|g| *g != glyph);
    }
}

/// One position in the tree and the move that led to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    parent: Option<NodeId>,
    mv: Option<Move>,
    position: Position,
    ply: usize,
    children: Vec<NodeId>,
    result: Option<GameResult>,
    end_marker: Option<String>,
    annotation: Annotation,
}

impl Node {
    pub fn parent(&self) -> Option<NodeId> {
        self.parent
    }

    /// The move that led here; `None` only at the root.
    pub fn mv(&self) -> Option<&Move> {
        self.mv.as_ref()
    }

    /// The position after the move.
    pub fn position(&self) -> &Position {
        &self.position
    }

    /// Moves from the root: 0 at the root, 2 once both sides have set up.
    pub fn ply(&self) -> usize {
        self.ply
    }

    /// Continuations; the first one is the main line.
    pub fn children(&self) -> &[NodeId] {
        &self.children
    }

    /// How the game ended here, by the rules or for an outside reason.
    pub fn result(&self) -> Option<GameResult> {
        self.result
    }

    /// True if the position decides the game (goal, elimination,
    /// immobilization), so no move can follow. After an outside result
    /// such as a resignation, analysis can still continue.
    pub fn is_terminal(&self) -> bool {
        self.result.is_some_and(|r| r.reason.is_on_board())
    }

    /// End marker from a loaded record, such as `resigns`. Not interpreted.
    pub fn end_marker(&self) -> Option<&str> {
        self.end_marker.as_deref()
    }

    pub fn annotation(&self) -> &Annotation {
        &self.annotation
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameTree {
    /// Arena of nodes; a deleted node leaves `None`, so ids stay stable.
    nodes: Vec<Option<Node>>,
}

impl Default for GameTree {
    fn default() -> Self {
        GameTree::new()
    }
}

impl std::ops::Index<NodeId> for GameTree {
    type Output = Node;

    /// Panics if the node doesn't exist; use [`GameTree::node`] for ids that
    /// may be stale.
    fn index(&self, id: NodeId) -> &Node {
        self.node(id).expect("node exists")
    }
}

impl GameTree {
    pub const ROOT: NodeId = NodeId(0);

    pub fn new() -> GameTree {
        GameTree {
            nodes: vec![Some(Node {
                parent: None,
                mv: None,
                position: Position::empty(Color::Gold),
                ply: 0,
                children: Vec::new(),
                result: None,
                end_marker: None,
                annotation: Annotation::default(),
            })],
        }
    }

    pub fn root(&self) -> NodeId {
        GameTree::ROOT
    }

    /// The node, or `None` if it was deleted or never existed.
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id.index()).and_then(Option::as_ref)
    }

    fn get(&self, id: NodeId) -> Result<&Node, GameError> {
        self.node(id).ok_or(GameError::NoSuchNode)
    }

    fn get_mut(&mut self, id: NodeId) -> Result<&mut Node, GameError> {
        self.nodes.get_mut(id.index()).and_then(Option::as_mut).ok_or(GameError::NoSuchNode)
    }

    pub fn contains(&self, id: NodeId) -> bool {
        self.node(id).is_some()
    }

    /// Number of nodes, including the root.
    pub fn len(&self) -> usize {
        self.nodes.iter().flatten().count()
    }

    /// Always false: the root can't be deleted.
    pub fn is_empty(&self) -> bool {
        false
    }

    pub fn annotation_mut(&mut self, id: NodeId) -> Result<&mut Annotation, GameError> {
        Ok(&mut self.get_mut(id)?.annotation)
    }

    fn check_can_move(&self, parent: NodeId) -> Result<&Node, GameError> {
        let node = self.get(parent)?;
        if node.is_terminal() { Err(GameError::GameOver) } else { Ok(node) }
    }

    /// Adds a child of `parent`, or returns the existing child that already
    /// leads to the same move.
    fn add_child(
        &mut self,
        parent: NodeId,
        mv: Move,
        position: Position,
        result: Option<GameResult>,
    ) -> NodeId {
        let p = &self[parent];
        // Setups are compared by position, since the same setup can list
        // its pieces in any order.
        let same = |n: &Node| match (&mv, n.mv()) {
            (Move::Setup(_), Some(Move::Setup(_))) => n.position == position,
            (m, Some(other)) => m == other,
            (_, None) => false,
        };
        if let Some(&existing) = p.children.iter().find(|&&c| same(&self[c])) {
            return existing;
        }
        let ply = p.ply + 1;
        let id = NodeId(u32::try_from(self.nodes.len()).expect("fewer than 2^32 nodes"));
        self.nodes.push(Some(Node {
            parent: Some(parent),
            mv: Some(mv),
            position,
            ply,
            children: Vec::new(),
            result,
            end_marker: None,
            annotation: Annotation::default(),
        }));
        self.nodes[parent.index()].as_mut().expect("parent exists").children.push(id);
        id
    }

    pub fn add_setup(&mut self, parent: NodeId, placements: Vec<Placement>) -> Result<NodeId, GameError> {
        let node = self.check_can_move(parent)?;
        if !Game::is_setup_ply(node.ply) {
            return Err(GameError::ExpectedTurn);
        }
        let next = apply_setup(&node.position, &placements)?;
        Ok(self.add_child(parent, Move::Setup(placements), next, None))
    }

    /// Starts a turn from the position at `parent`.
    pub fn begin_turn(&self, parent: NodeId) -> Result<TurnBuilder, GameError> {
        let node = self.check_can_move(parent)?;
        if Game::is_setup_ply(node.ply) {
            return Err(GameError::ExpectedSetup);
        }
        Ok(TurnBuilder::new(&node.position))
    }

    /// True if a turn from `parent` ending in `end` would repeat a position
    /// (same pieces, same side to move) for the third time. Only positions
    /// on the path from the root to `parent` count.
    pub fn is_third_repetition(&self, parent: NodeId, end: &Position) -> bool {
        let mut seen = 0;
        let mut at = self.node(parent);
        while let Some(n) = at
            && n.ply >= 2
        {
            if n.position == *end {
                seen += 1;
                if seen >= 2 {
                    return true;
                }
            }
            at = n.parent.and_then(|p| self.node(p));
        }
        false
    }

    /// Adds a turn built with [`GameTree::begin_turn`].
    pub fn add_turn(&mut self, parent: NodeId, turn: Turn) -> Result<NodeId, GameError> {
        let node = self.check_can_move(parent)?;
        if Game::is_setup_ply(node.ply) {
            return Err(GameError::ExpectedSetup);
        }
        if turn.start != node.position {
            return Err(GameError::StaleTurn);
        }
        if self.is_third_repetition(parent, &turn.end) {
            return Err(GameError::Repetition);
        }
        let result = outcome_after_turn(&turn.end, turn.start.side_to_move());
        let mv = Move::Steps(turn.effects());
        Ok(self.add_child(parent, mv, turn.end, result))
    }

    /// Adds one move given in notation without a move number: a setup or
    /// steps with optional capture tokens.
    pub fn add_notation(&mut self, parent: NodeId, text: &str) -> Result<NodeId, GameError> {
        let (body, _marker) = notation::parse_move_body(text)?;
        match body {
            MoveBody::Empty => Err(GameError::EmptyMove),
            MoveBody::Setup(p) => self.add_setup(parent, p),
            MoveBody::Steps(steps) => {
                let turn = build_turn(self.begin_turn(parent)?, &steps)?;
                self.add_turn(parent, turn)
            }
        }
    }

    /// Ends the game at `node` for a reason outside the rules (timeout,
    /// resignation, ...). Moves after it, such as a likely continuation
    /// after a resignation, stay as analysis; the main line ends here.
    pub fn end_line(&mut self, node: NodeId, result: GameResult) -> Result<(), GameError> {
        if self.get(node)?.result.is_some() {
            return Err(GameError::GameOver);
        }
        self.get_mut(node)?.result = Some(result);
        Ok(())
    }

    pub(crate) fn set_end_marker(&mut self, node: NodeId, marker: Option<String>) -> Result<(), GameError> {
        self.get_mut(node)?.end_marker = marker;
        Ok(())
    }

    /// Makes `node` its parent's first child, leaving the others in order.
    pub fn make_first(&mut self, node: NodeId) -> Result<(), GameError> {
        let (parent, i) = self.sibling_index(node)?;
        let children = &mut self.get_mut(parent)?.children;
        let c = children.remove(i);
        children.insert(0, c);
        Ok(())
    }

    /// The nodes from the root to `node`, both included.
    pub fn path(&self, node: NodeId) -> Vec<NodeId> {
        let mut path = Vec::new();
        let mut at = Some(node).filter(|&n| self.contains(n));
        while let Some(id) = at {
            path.push(id);
            at = self[id].parent;
        }
        path.reverse();
        path
    }

    /// The end of the main continuation from `node`: following first
    /// children until a node has none, or has a result. Moves after a
    /// result (analysis of how the game would have ended) are always
    /// variations.
    pub fn line_end(&self, node: NodeId) -> NodeId {
        let mut at = node;
        while let Some(n) = self.node(at)
            && n.result.is_none()
            && let Some(&next) = n.children.first()
        {
            at = next;
        }
        at
    }

    /// The path to `node`, then its main continuation to the end.
    pub fn line_through(&self, node: NodeId) -> Vec<NodeId> {
        self.path(self.line_end(node))
    }

    /// The main line, from the root to its last move.
    pub fn main_line(&self) -> Vec<NodeId> {
        self.line_through(GameTree::ROOT)
    }

    /// True if `node` is on the main line.
    pub fn is_main_line(&self, node: NodeId) -> bool {
        let mut at = node;
        while let Some(parent) = self.node(at).and_then(|n| n.parent) {
            if self[parent].children.first() != Some(&at) || self[parent].result.is_some() {
                return false;
            }
            at = parent;
        }
        self.contains(node)
    }

    /// The line from the root to `node` as a plain game, with `node`'s
    /// result and end marker.
    pub fn to_game(&self, node: NodeId) -> Result<Game, GameError> {
        let end = self.get(node)?;
        let path = self.path(node);
        let moves = path[1..].iter().map(|&id| self[id].mv.clone().expect("only the root has no move"));
        let positions = path.iter().map(|&id| self[id].position.clone()).collect();
        Ok(Game::from_parts(moves.collect(), positions, end.result, end.end_marker.clone()))
    }

    /// True if the tree is one line with no comments or glyphs, so a plain
    /// record holds all of it.
    pub fn is_plain(&self) -> bool {
        let line = self.main_line();
        let end = *line.last().expect("the main line has the root");
        self[end].children.is_empty()
            && line.iter().all(|&id| self[id].children.len() <= 1 && self[id].annotation.is_empty())
    }

    /// The main line as a plain game.
    pub fn main_game(&self) -> Game {
        self.to_game(self.line_end(GameTree::ROOT)).expect("the main line exists")
    }

    /// A tree holding `game` as its main line. Returns the tree and the id
    /// of the game's last move.
    pub fn from_game(game: &Game) -> (GameTree, NodeId) {
        let mut tree = GameTree::new();
        let mut at = GameTree::ROOT;
        for (ply, mv) in game.moves().iter().enumerate() {
            let position = game.position_at(ply + 1).expect("a position per ply").clone();
            // The last move carries the game's result; a rules result also
            // appears there.
            let result = if ply + 1 == game.ply_count() { game.result() } else { None };
            at = tree.add_child(at, mv.clone(), position, result);
        }
        tree.get_mut(at).expect("just added").end_marker = game.end_marker().map(str::to_string);
        (tree, at)
    }

    /// Deletes `node` and everything after it. The root can't be deleted.
    pub fn delete(&mut self, node: NodeId) -> Result<(), GameError> {
        let parent = self.get(node)?.parent.ok_or(GameError::IsRoot)?;
        self.get_mut(parent)?.children.retain(|&c| c != node);
        let mut stack = vec![node];
        while let Some(id) = stack.pop() {
            if let Some(n) = self.nodes[id.index()].take() {
                stack.extend(n.children);
            }
        }
        Ok(())
    }

    /// The position of `node` among its siblings, and the parent.
    fn sibling_index(&self, node: NodeId) -> Result<(NodeId, usize), GameError> {
        let parent = self.get(node)?.parent.ok_or(GameError::IsRoot)?;
        let index = self[parent].children.iter().position(|&c| c == node).expect("child of its parent");
        Ok((parent, index))
    }

    /// Moves `node` one place earlier among its siblings. At the front
    /// already, it stays.
    pub fn promote(&mut self, node: NodeId) -> Result<(), GameError> {
        let (parent, i) = self.sibling_index(node)?;
        if i > 0 {
            self.get_mut(parent)?.children.swap(i - 1, i);
        }
        Ok(())
    }

    /// Moves `node` one place later among its siblings. At the back
    /// already, it stays.
    pub fn demote(&mut self, node: NodeId) -> Result<(), GameError> {
        let (parent, i) = self.sibling_index(node)?;
        let children = &mut self.get_mut(parent)?.children;
        if i + 1 < children.len() {
            children.swap(i, i + 1);
        }
        Ok(())
    }

    /// Makes the line through `node` the main line: every node on its path
    /// becomes its parent's first child. The others keep their order.
    pub fn make_main_line(&mut self, node: NodeId) -> Result<(), GameError> {
        self.get(node)?;
        let path = self.path(node);
        for &child in &path[1..] {
            self.make_first(child)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WinReason;
    use crate::setup::default_setup;

    /// A tree with both default setups played; returns the node after them.
    fn started() -> (GameTree, NodeId) {
        let mut t = GameTree::new();
        let g = t.add_setup(GameTree::ROOT, default_setup(Color::Gold)).unwrap();
        let s = t.add_setup(g, default_setup(Color::Silver)).unwrap();
        (t, s)
    }

    fn play(t: &mut GameTree, mut at: NodeId, moves: &[&str]) -> NodeId {
        for m in moves {
            at = t.add_notation(at, m).unwrap_or_else(|e| panic!("{m}: {e}"));
        }
        at
    }

    fn notation(t: &GameTree, ids: &[NodeId]) -> Vec<String> {
        ids.iter().filter_map(|&id| t[id].mv().map(Move::notation)).collect()
    }

    #[test]
    fn branches_instead_of_truncating() {
        let (mut t, start) = started();
        let main = play(&mut t, start, &["Ee2n", "ee7s", "Ee3n"]);
        let alt = play(&mut t, start, &["Db2n"]);
        assert_eq!(t[start].children().len(), 2);
        assert_eq!(t.line_end(start), main, "the first move played stays the main line");
        assert_eq!(notation(&t, &t.main_line())[2..], ["Ee2n", "ee7s", "Ee3n"]);
        assert_eq!(t[alt].ply(), 3);
        assert!(!t.is_main_line(alt));
        assert!(t.is_main_line(main));
        assert_eq!(t.len(), 7);
    }

    #[test]
    fn replaying_a_known_move_reuses_the_node() {
        let (mut t, start) = started();
        let a = play(&mut t, start, &["Ee2n", "ee7s"]);
        let b = play(&mut t, start, &["Ee2n", "ee7s"]);
        assert_eq!(a, b);
        assert_eq!(t.len(), 5);
        // A setup in another order is the same setup.
        let mut reversed = default_setup(Color::Gold);
        reversed.reverse();
        let g = t.add_setup(GameTree::ROOT, reversed).unwrap();
        assert_eq!(t[GameTree::ROOT].children(), &[g]);
    }

    #[test]
    fn repetition_counts_only_the_path() {
        let (mut t, start) = started();
        let shuffle = ["Ha2n", "ha7s", "Ha3s", "ha6n"];
        let once = play(&mut t, start, &shuffle);
        let twice = play(&mut t, once, &shuffle[..3]);
        assert_eq!(t.add_notation(twice, "ha6n"), Err(GameError::Repetition));
        // A side line from `start` returns to the same position. `once`
        // holds it too, but isn't on this path, so it's only the second
        // time here.
        let side = play(&mut t, start, &["Hh2n", "hh7s", "Hh3s", "hh6n"]);
        assert_eq!(t[side].position(), t[once].position());
        // A third time on this path is refused.
        let again = play(&mut t, side, &shuffle[..3]);
        assert_eq!(t.add_notation(again, "ha6n"), Err(GameError::Repetition));
    }

    #[test]
    fn analysis_continues_after_an_outside_result() {
        let (mut t, start) = started();
        let a = play(&mut t, start, &["Ee2n"]);
        let resign = GameResult { winner: Color::Gold, reason: WinReason::Resignation };
        t.end_line(a, resign).unwrap();
        assert_eq!(t[a].result(), Some(resign));
        assert!(!t[a].is_terminal());
        assert_eq!(t.end_line(a, resign), Err(GameError::GameOver));
        // The likely continuation after the resignation is analysis: the
        // main line still ends at the resignation.
        let after = play(&mut t, a, &["ee7s", "Ee3n"]);
        assert_eq!(t.line_end(GameTree::ROOT), a);
        assert!(!t.is_main_line(after) && t.is_main_line(a));
        assert!(!t.is_plain());
        assert_eq!(t.line_end(t[after].parent().unwrap()), after);
        // A result can also go on a move that already has continuations.
        let b = play(&mut t, start, &["Db2n"]);
        play(&mut t, b, &["ee7s"]);
        t.end_line(b, resign).unwrap();
    }

    #[test]
    fn nothing_follows_a_result_on_the_board() {
        // Reaching a real goal needs a long game; a goal result recorded on
        // a move behaves the same.
        let goal = GameResult { winner: Color::Gold, reason: WinReason::Goal };
        let (mut t, start) = started();
        let a = play(&mut t, start, &["Ee2n"]);
        t.end_line(a, goal).unwrap();
        assert!(t[a].is_terminal());
        assert_eq!(t.add_notation(a, "ee7s"), Err(GameError::GameOver));
    }

    #[test]
    fn promote_demote_and_main_line() {
        let (mut t, start) = started();
        let a = play(&mut t, start, &["Ee2n"]);
        let b = play(&mut t, start, &["Db2n"]);
        let c = play(&mut t, start, &["Dg2n"]);
        t.promote(c).unwrap();
        assert_eq!(t[start].children(), &[a, c, b]);
        t.promote(c).unwrap();
        t.promote(c).unwrap();
        assert_eq!(t[start].children(), &[c, a, b]);
        t.demote(c).unwrap();
        assert_eq!(t[start].children(), &[a, c, b]);

        let deep = play(&mut t, b, &["ee7s", "Db3n"]);
        let before_deep = t[deep].parent().unwrap();
        play(&mut t, before_deep, &["Db3e"]);
        t.make_main_line(deep).unwrap();
        assert_eq!(t[start].children(), &[b, a, c]);
        assert_eq!(t.line_end(GameTree::ROOT), deep);
        assert_eq!(t.promote(GameTree::ROOT), Err(GameError::IsRoot));
    }

    #[test]
    fn delete_removes_the_subtree() {
        let (mut t, start) = started();
        let a = play(&mut t, start, &["Ee2n"]);
        let a2 = play(&mut t, a, &["ee7s", "Ee3n"]);
        let b = play(&mut t, start, &["Db2n"]);
        t.delete(a).unwrap();
        assert!(!t.contains(a) && !t.contains(a2));
        assert_eq!(t[start].children(), &[b]);
        assert_eq!(t.len(), 4);
        assert_eq!(t.delete(a), Err(GameError::NoSuchNode));
        assert_eq!(t.delete(GameTree::ROOT), Err(GameError::IsRoot));
        // Ids are not reused.
        let c = play(&mut t, start, &["Ee2n"]);
        assert!(c != a && c.0 > b.0);
    }

    #[test]
    fn game_round_trip() {
        let mut g = Game::new();
        g.play_setup(default_setup(Color::Gold)).unwrap();
        g.play_setup(default_setup(Color::Silver)).unwrap();
        g.play_notation("Ee2n Ee3n").unwrap();
        g.end_game(GameResult { winner: Color::Gold, reason: WinReason::Timeout }).unwrap();
        let (mut t, last) = GameTree::from_game(&g);
        assert_eq!(t.main_game(), g);
        assert_eq!(t[last].ply(), 3);
        // A variation doesn't change the main game.
        let start = t[last].parent().unwrap();
        let alt = play(&mut t, start, &["Db2n"]);
        assert_eq!(t.main_game(), g);
        let line = t.to_game(alt).unwrap();
        assert_eq!(line.ply_count(), 3);
        assert_eq!(line.result(), None);
        assert!(line.to_record().ends_with("2g Db2n\n"));
    }

    #[test]
    fn setups_then_turns() {
        let mut t = GameTree::new();
        let g =
            t.add_notation(GameTree::ROOT, "Ra7 Eb1 Rc1 Rd1 Re1 Rf1 Rg1 Rh1 Ha2 Mb2 Cc2 Dd2 Re2 Cf2 Dg2 Hh2");
        assert!(g.is_err(), "a7 isn't a home square");
        let g = t
            .add_notation(GameTree::ROOT, &notation::format_placements(&default_setup(Color::Gold)))
            .unwrap();
        let s = t.add_notation(g, &notation::format_placements(&default_setup(Color::Silver))).unwrap();
        assert_eq!(t[s].result(), None);
        assert_eq!(t.begin_turn(GameTree::ROOT).err(), Some(GameError::ExpectedSetup));
        assert_eq!(t.add_setup(s, default_setup(Color::Gold)), Err(GameError::ExpectedTurn));
    }

    #[test]
    fn glyphs_replace_their_own_kind() {
        let mut a = Annotation::default();
        a.set_glyph(Glyph::GOOD);
        a.set_glyph(Glyph(14));
        a.set_glyph(Glyph::DUBIOUS);
        assert_eq!(a.glyphs, [Glyph(14), Glyph::DUBIOUS]);
        a.set_glyph(Glyph(146));
        a.set_glyph(Glyph(146));
        assert_eq!(a.glyphs.len(), 3);
        assert_eq!(Glyph::parse("!?"), Some(Glyph::INTERESTING));
        assert_eq!(Glyph::parse("$14"), Some(Glyph(14)));
        assert_eq!(Glyph::parse("!!!"), None);
        assert_eq!(Glyph(14).to_string(), "$14");
        assert_eq!(Glyph::BLUNDER.to_string(), "??");
    }

    #[test]
    fn stale_and_missing_nodes() {
        let (mut t, start) = started();
        let tb = t.begin_turn(start).unwrap();
        let a = play(&mut t, start, &["Ee2n"]);
        let mut tb2 = tb.clone();
        tb2.try_move("d2".parse().unwrap(), "d3".parse().unwrap()).unwrap();
        assert_eq!(t.add_turn(a, tb2.finish().unwrap()), Err(GameError::StaleTurn));
        assert_eq!(t.add_notation(NodeId(99), "Ee2n"), Err(GameError::NoSuchNode));
        assert!(t.path(NodeId(99)).is_empty());
        t.annotation_mut(a).unwrap().comment = Some("main".into());
        assert_eq!(t[a].annotation().comment.as_deref(), Some("main"));
    }
}
