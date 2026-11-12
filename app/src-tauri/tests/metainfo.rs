//! The AppStream metadata (`linux/io.github.janzert.howdah.metainfo.xml`)
//! names this version as its newest release, so a version bump can't
//! leave it out.

const METAINFO: &str = include_str!("../linux/io.github.janzert.howdah.metainfo.xml");

#[test]
fn the_newest_release_is_this_version() {
    let newest = METAINFO
        .split("<release ")
        .nth(1)
        .and_then(|r| r.split("version=\"").nth(1))
        .and_then(|v| v.split('"').next())
        .expect("the metainfo lists no release");
    assert_eq!(
        newest,
        env!("CARGO_PKG_VERSION"),
        "add a <release> for this version at the top of the metainfo's <releases>"
    );
}

#[test]
fn its_id_is_the_app_identifier() {
    let conf: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
    let id = conf["identifier"].as_str().unwrap();
    assert!(METAINFO.contains(&format!("<id>{id}</id>")), "the metainfo's <id> isn't {id}");
}
