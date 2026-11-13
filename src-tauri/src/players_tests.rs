use super::*;
fn row(name: &str, id: Option<u64>, date: &str) -> Score {
    serde_json::from_value(serde_json::json!({"id":name,"mapHash":"map","player":name,"playerId":id,"date":date,"mode":0,"title":"Map","artist":"","difficulty":"","mapper":"","mods":0,"n300":1,"n100":0,"n50":0,"misses":0,"combo":1,"maxCombo":null,"rawScore":100,"accuracy":100.0,"fc":false,"ss":true,"stars":null,"pp":null,"fcPp":null,"source":"stable","status":"","hasArtwork":false})).unwrap()
}
#[test]
fn stable_and_lazer_names_resolve_to_one_id_across_case_and_renames() {
    let c = Connection::open_in_memory().unwrap();
    c.execute_batch("CREATE TABLE metadata(key TEXT PRIMARY KEY,data TEXT)")
        .unwrap();
    let mut rows = vec![
        row("Old Name", Some(42), "2017"),
        row("newname", Some(42), "2026"),
        row("old name", None, "2018"),
        row("NewName", None, "2020"),
        row("Other", Some(99), "2026"),
    ];
    let original = rows[2].player.clone();
    let accounts = resolve(&c, &mut rows);
    assert_eq!(accounts.len(), 2);
    for s in &rows[..4] {
        assert_eq!(s.player_id, Some(42));
        assert!(matches(s, "id:42"));
        assert!(!matches(s, "id:99"));
    }
    assert_eq!(rows[2].player, original);
    assert!(!matches(&rows[4], "id:42"));
    assert_eq!(
        accounts.iter().find(|a| a.id == 42).unwrap().name,
        "newname"
    );
}
#[test]
fn ambiguous_names_stay_unlinked_and_manual_history_never_overrides_verified_ids() {
    let c = Connection::open_in_memory().unwrap();
    c.execute_batch("CREATE TABLE metadata(key TEXT PRIMARY KEY,data TEXT)")
        .unwrap();
    let mut rows = vec![
        row("shared", Some(42), "2020"),
        row("SHARED", Some(99), "2021"),
        row("Shared", None, "2010"),
        row("Former Name", None, "2015"),
    ];
    save(
        &c,
        Account {
            id: 42,
            name: "Current".into(),
            aliases: vec!["Former Name".into()],
        },
    )
    .unwrap();
    resolve(&c, &mut rows);
    assert_eq!(rows[0].player_id, Some(42));
    assert_eq!(rows[1].player_id, Some(99));
    assert_eq!(rows[2].player_id, None);
    assert_eq!(rows[3].player_id, Some(42));
    assert!(!matches(&rows[2], "id:42"));
    assert!(matches(&rows[3], "former name"));
}

#[test]
fn cached_library_matches_stable_names_to_lazer_ids_without_rewriting_records() {
    let folder = std::env::temp_dir().join(format!(
        "score-explorer-players-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&folder).unwrap();
    let path = folder.join("library.sqlite");
    {
        let c = crate::library::db(&path).unwrap();
        let mut online = row("CurrentName", Some(42), "2026");
        online.source = "lazer".into();
        online.player_aliases = vec!["Former Name".into()];
        let stable = row("currentname", None, "2020");
        let older = row("FORMER NAME", None, "2017");
        let stranger = row("Other player", Some(99), "2026");
        for s in [online, stable, older, stranger] {
            c.execute(
                "INSERT INTO scores(id,data) VALUES(?1,?2)",
                rusqlite::params![s.id, serde_json::to_string(&s).unwrap()],
            )
            .unwrap();
        }
        // Historical names gathered from matching imported copies survive loading.
        c.execute(
            "INSERT INTO metadata VALUES('player_accounts',?1)",
            [serde_json::to_string(&vec![Account {
                id: 42,
                name: "CurrentName".into(),
                aliases: vec!["Former Name".into()],
            }])
            .unwrap()],
        )
        .unwrap();
    }
    let library = crate::library::load(&path).unwrap();
    assert_eq!(library.player_accounts.len(), 2);
    assert_eq!(
        library
            .scores
            .iter()
            .filter(|s| matches(s, "id:42"))
            .count(),
        3
    );
    assert_eq!(
        library
            .scores
            .iter()
            .filter(|s| matches(s, "id:99"))
            .count(),
        1
    );
    let c = Connection::open(&path).unwrap();
    let original: String = c
        .query_row("SELECT data FROM scores WHERE id='currentname'", [], |r| {
            r.get(0)
        })
        .unwrap();
    let original: Score = serde_json::from_str(&original).unwrap();
    assert_eq!(original.player_id, None);
    assert_eq!(original.player, "currentname");
    drop(c);
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn reused_username_does_not_assign_old_scores_to_an_account_created_later() {
    let c = Connection::open_in_memory().unwrap();
    c.execute_batch("CREATE TABLE metadata(key TEXT PRIMARY KEY,data TEXT)")
        .unwrap();
    for p in [
        crate::player_profiles::Profile {
            id: 42,
            name: "Current Name".into(),
            aliases: vec!["Old Name".into()],
            joined: "2017-01-01T00:00:00Z".into(),
            fetched: 0,
        },
        crate::player_profiles::Profile {
            id: 99,
            name: "Old Name".into(),
            aliases: vec![],
            joined: "2020-11-11T00:00:00Z".into(),
            fetched: 0,
        },
    ] {
        crate::player_profiles::store(&c, &p).unwrap();
    }
    let mut imported = row("OLD NAME", Some(99), "2018-06-01T00:00:00Z");
    imported.source = "lazer".into();
    let mut genuine = row("Old Name", Some(99), "2026-01-01T00:00:00Z");
    genuine.source = "lazer".into();
    genuine.gameplay_lazer = true;
    genuine.solo_online_id = Some("123".into());
    let mut rows = vec![
        imported,
        row("old name", None, "2018-05-01T00:00:00Z"),
        row("old name", None, "2026-02-01T00:00:00Z"),
        genuine,
    ];
    resolve(&c, &mut rows);
    assert_eq!(rows[0].player_id, Some(42));
    assert_eq!(rows[1].player_id, Some(42));
    assert_eq!(
        rows[2].player_id, None,
        "Unknown later ownership must remain ambiguous"
    );
    assert_eq!(
        rows[3].player_id,
        Some(99),
        "Keep the new holder's real native score"
    );
    assert_eq!(rows.iter().filter(|s| matches(s, "id:42")).count(), 2);
}

#[test]
fn public_profile_parser_validates_id_and_decodes_name_history() {
    let html = r#"<div data-initial-data="{&quot;user&quot;:{&quot;id&quot;:42,&quot;username&quot;:&quot;New &amp; Name&quot;,&quot;previous_usernames&quot;:[&quot;Old Name&quot;],&quot;join_date&quot;:&quot;2017-01-01T00:00:00Z&quot;}}"></div>"#;
    let p = crate::player_profiles::parse(html, 42).unwrap();
    assert_eq!(p.name, "New & Name");
    assert_eq!(p.aliases, vec!["Old Name"]);
    assert!(crate::player_profiles::parse(html, 99).is_err());
    assert!(crate::player_profiles::parse("<html>Sign in</html>", 42).is_err());
}
