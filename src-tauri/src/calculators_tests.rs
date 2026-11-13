use super::*;
use crate::{
    downloads::tests::{fixture, map, score},
    library,
};
#[test]
fn calculator_switch_preserves_original_results_and_rejects_stale_inputs() {
    let (root, songs, path) = fixture("calculators");
    let mut c = library::db(&path).unwrap();
    let text = String::from_utf8(map("Engine fixture")).unwrap();
    let header = text.split("[HitObjects]").next().unwrap();
    let objects = (0..100)
        .map(|i| {
            format!(
                "{},{},{},1,0,0:0:0:0:",
                64 + (i % 5) * 90,
                64 + (i % 3) * 90,
                1000 + i * 150
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let bytes = format!("{header}[HitObjects]\n{objects}\n").into_bytes();
    let hash = format!("{:x}", md5::compute(&bytes));
    score(&c, "fixture", &hash, "Player");
    c.execute(
        "UPDATE scores SET data=json_set(data,'$.n300',99,'$.combo',90,'$.rawScore',320000)",
        [],
    )
    .unwrap();
    c.execute("UPDATE scores SET data=json_set(data,'$.pp',999,'$.fcPp',1000,'$.stars',9,'$.calculationVersion',?1)",[library::ENGINE]).unwrap();
    let raw: String = c
        .query_row("SELECT data FROM scores", [], |r| r.get(0))
        .unwrap();
    let s: Score = serde_json::from_str(&raw).unwrap();
    cache(&c, &s, &mut Map::new(&bytes).unwrap()).unwrap();
    assert_eq!(
        c.query_row::<String, _, _>("SELECT data FROM scores", [], |r| r.get(0))
            .unwrap(),
        raw
    );
    c.execute(
        "INSERT OR REPLACE INTO metadata VALUES('calculator','lazer')",
        [],
    )
    .unwrap();
    let l = library::load(&path).unwrap();
    assert!(l.scores[0].pp.is_some(), "{}", l.scores[0].status);
    assert_ne!(l.scores[0].pp, Some(999.0));
    c.execute(
        "INSERT OR REPLACE INTO metadata VALUES('calculator','rosu')",
        [],
    )
    .unwrap();
    assert_eq!(library::load(&path).unwrap().scores[0].pp, Some(999.0));
    c.execute("UPDATE scores SET data=json_set(data,'$.rawScore',321)", [])
        .unwrap();
    c.execute(
        "INSERT OR REPLACE INTO metadata VALUES('calculator','lazer')",
        [],
    )
    .unwrap();
    assert!(library::load(&path).unwrap().scores[0].pp.is_none());
    let file = songs.join("map.osu");
    std::fs::write(&file, &bytes).unwrap();
    crate::recovery::installed(&mut c, &songs, &[file]).unwrap();
    assert!(library::load(&path).unwrap().scores[0].pp.is_some());
    drop(c);
    std::fs::remove_dir_all(root).unwrap();
}
