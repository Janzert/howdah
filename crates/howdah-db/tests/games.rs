use howdah_arimaa::{Color, GameRecord, GameTree, Glyph};
use howdah_db::{Database, GameData, PlayerInfo, SourceKey, StoredResult, gid_for};

const RECORD: &str = r#"[Event "Casual game"]
[Site "arimaa.com"]
[Date "2018.11.01"]
[Gold "Tri21"]
[Silver "bot_ArimaaScoreP1"]
[GoldRating "1400"]
[TimeControl "2/2/100/10/8"]
[Round "3"]

{The game comment.}
1g Ra1 Rb1 Rc1 Rd1 Re1 Rf1 Rg1 Rh1 Da2 Hb2 Cc2 Ed2 Me2 Hf2 Cg2 Dh2 {[%emt 0:01:10]}
1s ch7 hg7 df7 me7 ed7 dc7 hb7 ca7 rh8 rg8 rf8 re8 rd8 rc8 rb8 ra8 {[%emt 0:00:01]}
2g Ed2n Ed3n Me2n Me3n ! {Strong. [%emt 0:00:40] [%cal Ge3e5]}
  ( {Or the dog.} 2g Da2n Da3n Ed2n Ed3n ?! {Slow.}
    2s ca7s ra8s ca6s ra7s
    ( 2s hg7s hg6s rg8s rg7s ) )
2s ca7s ra8s ca6s ra7s {[%emt 0:00:06]}
3g Da2n Da3n Da4s ca5s
"#;

fn record() -> GameRecord {
    GameRecord::parse(RECORD).unwrap()
}

fn same_trees(a: &GameTree, b: &GameTree) {
    fn walk(a: &GameTree, an: howdah_arimaa::NodeId, b: &GameTree, bn: howdah_arimaa::NodeId) {
        assert_eq!(a[an].position(), b[bn].position());
        assert_eq!(a[an].annotation(), b[bn].annotation(), "annotations at {:?}", a.label_of(an));
        assert_eq!(a[an].result(), b[bn].result());
        assert_eq!(a[an].children().len(), b[bn].children().len());
        for (&ac, &bc) in a[an].children().iter().zip(b[bn].children()) {
            walk(a, ac, b, bc);
        }
    }
    walk(a, a.root(), b, b.root());
}

#[test]
fn record_round_trips() {
    let mut db = Database::open_in_memory().unwrap();
    let original = record();
    let data = GameData::from_record(&original);
    assert_eq!(data.tags, [("Round".to_string(), "3".to_string())]);
    let id = db.save(&data).unwrap();
    let back = db.load(id).unwrap();
    same_trees(&original.tree, back.tree.as_ref().unwrap());
    let rec = back.to_record();
    for tag in ["Event", "Site", "Date", "Gold", "Silver", "GoldRating", "TimeControl", "Round"] {
        assert_eq!(rec.tag(tag), original.tag(tag), "{tag}");
    }
    // The times are a blob on the game, not in the comments' rows.
    let comments: i64 = db
        .connection()
        .query_row(
            "SELECT count(*) FROM annotations WHERE kind = 'comment' AND payload LIKE '%emt%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(comments, 0);
    // Setups come back in square order, so the text can differ from the
    // original's; from then on it's stable.
    let again = db.save(&GameData { id: None, ..GameData::from_record(&rec) }).unwrap();
    assert_eq!(db.load(again).unwrap().to_record().to_record(), rec.to_record());
}

#[test]
fn resaving_updates_in_place() {
    let mut db = Database::open_in_memory().unwrap();
    let mut data = GameData::from_record(&record());
    let id = db.save(&data).unwrap();
    let created: i64 = db
        .connection()
        .query_row(
            "SELECT created_at FROM annotations WHERE game = ?1 AND node = 0 AND kind = 'comment'",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    // Change a glyph, drop the game comment, add a move.
    let tree = data.tree.as_mut().unwrap();
    let main = tree.main_line();
    tree.annotation_mut(main[3]).unwrap().set_glyph(Glyph::BRILLIANT);
    tree.annotation_mut(main[0]).unwrap().comment = None;
    let end = *main.last().unwrap();
    tree.add_notation(end, "hg7s hg6s rg8s rg7s").unwrap();
    data.id = Some(id);
    assert_eq!(db.save(&data).unwrap(), id);
    assert_eq!(db.game_count().unwrap(), 1);
    let back = db.load(id).unwrap();
    same_trees(data.tree.as_ref().unwrap(), back.tree.as_ref().unwrap());
    let gone: i64 = db
        .connection()
        .query_row(
            "SELECT count(*) FROM annotations WHERE game = ?1 AND node = 0 AND kind = 'comment'",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(gone, 0);
    assert!(created > 0);
}

#[test]
fn results_outside_the_rules_come_back() {
    let mut db = Database::open_in_memory().unwrap();
    let mut data = GameData::from_record(&record());
    data.result = Some(StoredResult { winner: 's', termination: Some('r') });
    let id = db.save(&data).unwrap();
    let rec = db.load(id).unwrap().to_record();
    assert_eq!(rec.tag("Result"), None);
    assert!(rec.to_record().contains("0-1"), "{}", rec.to_record());
    let end = *rec.tree.main_line().last().unwrap();
    assert_eq!(rec.tree[end].result().map(|r| r.winner), Some(Color::Silver));

    // An old draw has no place in the tree, but stays in the row.
    data.result = Some(StoredResult { winner: 'd', termination: Some('p') });
    data.id = Some(id);
    db.save(&data).unwrap();
    assert_eq!(db.load(id).unwrap().result, data.result);
}

#[test]
fn set_positions_round_trip() {
    let full = record();
    let at = full.tree.main_line()[4];
    let mut tree = GameTree::from_position(full.tree[at].position().clone(), 3).unwrap();
    let next = tree.add_notation(tree.root(), "Da2n Da3n Da4s ca5s").unwrap();
    tree.add_notation(next, "hg7s hg6s rg8s rg7s").unwrap();
    let original = GameRecord { tags: Vec::new(), tree };
    let mut db = Database::open_in_memory().unwrap();
    let id = db.save(&GameData::from_record(&original)).unwrap();
    let back = db.load(id).unwrap().to_record();
    assert_eq!(back.tree.start_ply(), original.tree.start_ply());
    same_trees(&original.tree, &back.tree);
}

#[test]
fn header_only_games_and_source_keys() {
    let mut db = Database::open_in_memory().unwrap();
    let mut data = GameData::new();
    data.source = Some(SourceKey::arimaa_com("620100"));
    data.gid = gid_for(data.source.as_ref().unwrap());
    data.tree = None;
    data.players[0] = PlayerInfo { name: Some("Tri21".into()), ..Default::default() };
    let id = db.save(&data).unwrap();
    assert_eq!(db.find_source("arimaa.com", "620100").unwrap(), Some(id));
    assert!(!db.has_moves(id).unwrap());
    assert!(db.load(id).unwrap().tree.is_none());

    // The full game later replaces it, found by the same gid; its player
    // takes the id the archive gives.
    let mut full = GameData::from_record(&record());
    full.source = data.source.clone();
    full.gid = data.gid;
    full.players[0].site_id = Some("54851".into());
    assert_eq!(db.save(&full).unwrap(), id);
    assert!(db.has_moves(id).unwrap());
    let players: i64 = db
        .connection()
        .query_row("SELECT count(*) FROM players WHERE name = 'Tri21'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(players, 1);
    let site_id: Option<String> = db
        .connection()
        .query_row("SELECT site_id FROM players WHERE name = 'Tri21'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(site_id.as_deref(), Some("54851"));
}

#[test]
fn reopens_from_a_file() {
    let dir = std::env::temp_dir().join(format!("howdah-db-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("games.db");
    let _ = std::fs::remove_file(&path);
    let id = {
        let mut db = Database::open(&path).unwrap();
        db.save(&GameData::from_record(&record())).unwrap()
    };
    let db = Database::open(&path).unwrap();
    assert!(db.load(id).unwrap().tree.is_some());
    let ro = Database::open_read_only(&path).unwrap();
    assert!(ro.is_read_only());
    assert_eq!(ro.game_count().unwrap(), 1);
    drop((db, ro));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_source_copy_keeps_the_users_work() {
    use howdah_db::SaveMode;
    let mut db = Database::open_in_memory().unwrap();
    let mut source = GameData::from_record(&record());
    source.source = Some(SourceKey::arimaa_com("1"));
    source.gid = gid_for(source.source.as_ref().unwrap());
    source.start_ts = Some(1_541_048_963);
    source.players[0].site_id = Some("54851".into());
    let id = db.save_as(&source, SaveMode::Source).unwrap();

    // The user's copy, edited through a record.
    let mut rec = db.load(id).unwrap().to_record();
    let main = rec.tree.main_line();
    rec.tree.annotation_mut(main[4]).unwrap().set_comment_text("Mine.");
    db.save_record(Some(id), &rec).unwrap();
    let saved = db.load(id).unwrap();
    assert_eq!(saved.start_ts, Some(1_541_048_963), "headers the record lacks stay");
    assert_eq!(saved.players[0].site_id.as_deref(), Some("54851"));

    // The source again, with the same moves: the comment and variations stay.
    db.save_as(&source, SaveMode::Source).unwrap();
    let again = db.load(id).unwrap();
    same_trees(saved.tree.as_ref().unwrap(), again.tree.as_ref().unwrap());

    // With different moves it replaces them.
    let mut longer = source.clone();
    let tree = longer.tree.as_mut().unwrap();
    let end = *tree.main_line().last().unwrap();
    tree.add_notation(end, "hg7s hg6s rg8s rg7s").unwrap();
    db.save_as(&longer, SaveMode::Source).unwrap();
    let replaced = db.load(id).unwrap();
    same_trees(longer.tree.as_ref().unwrap(), replaced.tree.as_ref().unwrap());
}

#[test]
fn ratings_by_source_key() {
    let mut db = Database::open_in_memory().unwrap();
    for (id, ratings) in [("1", [Some(1800), Some(1650)]), ("2", [None, Some(1500)]), ("3", [None, None])] {
        let mut data = GameData::new();
        data.source = Some(SourceKey::arimaa_com(id));
        data.gid = gid_for(data.source.as_ref().unwrap());
        data.tree = None;
        data.players[0].rating = ratings[0];
        data.players[1].rating = ratings[1];
        db.save(&data).unwrap();
    }
    let found = db.source_ratings("arimaa.com", &["1", "2", "3", "4"]).unwrap();
    assert_eq!(found.len(), 2);
    assert_eq!(found["1"], [Some(1800), Some(1650)]);
    assert_eq!(found["2"], [None, Some(1500)]);
}
