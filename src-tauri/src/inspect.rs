use std::path::PathBuf;
fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: inspect-library <app-owned-cache.sqlite> <stable-folder> [songs-folder]");
        std::process::exit(2);
    }
    if args[1] == "--refresh-player" {
        let identity = score_explorer::player_profiles::refresh(
            &PathBuf::from(&args[2]),
            args[3].parse().expect("Account ID"),
        )
        .expect("Refresh public player history");
        println!(
            "Refreshed {} accounts and {} score identity assignments without rewriting source scores",
            identity.accounts.len(),
            identity.assignments.len()
        );
        return;
    }
    if args[1] == "--players" {
        let library =
            score_explorer::library::load(&PathBuf::from(&args[2])).expect("Load player groups");
        let mut groups = std::collections::BTreeMap::<String, usize>::new();
        for score in &library.scores {
            let key = score
                .player_id
                .map(|id| format!("id:{id}"))
                .unwrap_or_else(|| score.player.trim().to_lowercase());
            *groups.entry(key).or_default() += 1;
        }
        println!(
            "{}",
            serde_json::to_string_pretty(
                &serde_json::json!({"accounts":library.player_accounts,"groups":groups})
            )
            .unwrap()
        );
        return;
    }
    if args[1] == "--link-player" {
        let c = score_explorer::library::db_for_inspection(&PathBuf::from(&args[2]))
            .expect("Open app cache");
        score_explorer::players::save(
            &c,
            score_explorer::players::Account {
                id: args[3].parse().unwrap(),
                name: args[4].clone(),
                aliases: args[5..].to_vec(),
            },
        )
        .unwrap();
        println!("Saved player history association without changing score records");
        return;
    }
    if args[1] == "--recalculate" {
        let path = PathBuf::from(&args[2]);
        let c = score_explorer::library::db_for_inspection(&path).unwrap();
        c.execute(
            "INSERT OR REPLACE INTO metadata VALUES('calculator','lazer')",
            [],
        )
        .unwrap();
        drop(c);
        let ctl = std::sync::Arc::new(score_explorer::calculators::Controller::default());
        ctl.start(path);
        let mut tick = 0;
        loop {
            let s = ctl.status();
            if !s.running {
                println!("{}", s.message);
                break;
            }
            if tick % 50 == 0 {
                println!(
                    "Lazer calculations: {}/{} processed, {} ready",
                    s.processed, s.total, s.calculated
                );
            }
            tick += 1;
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        return;
    }
    if args[1] == "--compare-profile" {
        let rows: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&args[2]).unwrap()).unwrap();
        let c = rusqlite::Connection::open_with_flags(
            &args[3],
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let mut result = Vec::new();
        for row in rows.as_array().unwrap() {
            let hash = row["beatmap"]["checksum"].as_str().unwrap();
            let maps = score_explorer::maps::cached(&c, hash).unwrap();
            let bytes = maps
                .iter()
                .find_map(|m| {
                    score_explorer::maps::safe_file(
                        std::path::Path::new(&m.root),
                        std::path::Path::new(&m.path),
                    )
                    .and_then(|p| std::fs::read(p).ok())
                })
                .expect("Need exact installed map for profile validation");
            assert_eq!(format!("{:x}", md5::compute(&bytes)), hash);
            let stat = |key: &str| row["statistics"][key].as_u64().unwrap_or(0);
            let s:score_explorer::library::Score=serde_json::from_value(serde_json::json!({
                "id":"public-validation","mapHash":hash,"player":"Validation","date":row["ended_at"],"mode":row["ruleset_id"],
                "title":row["beatmapset"]["title"],"artist":"","difficulty":row["beatmap"]["version"],"mapper":"","mods":0,
                "modsJson":row["mods"].to_string(),"gameplayLazer":row["legacy_score_id"].is_null(),
                "n300":stat("great"),"n100":stat("ok"),"n50":stat("meh"),"misses":stat("miss"),"nGeki":stat("perfect"),"nKatu":stat("good"),
                "combo":row["max_combo"],"maxCombo":null,"rawScore":row["legacy_total_score"].as_u64().unwrap_or(0),"accuracy":row["accuracy"].as_f64().unwrap()*100.0,
                "fc":false,"ss":false,"stars":null,"pp":null,"fcPp":null,"source":"lazer","status":"","hasArtwork":false
            })).unwrap();
            let norm = |values: &serde_json::Value| -> serde_json::Value {
                let mut map = serde_json::Map::new();
                for (a, b) in [
                    ("large_tick_hit", "LargeTickHit"),
                    ("small_tick_hit", "SmallTickHit"),
                    ("slider_tail_hit", "SliderTailHit"),
                ] {
                    map.insert(b.into(), values[a].clone());
                }
                serde_json::Value::Object(map)
            };
            let actual = score_explorer::calculators::Map::new(&bytes)
                .unwrap()
                .calculate(
                    &s,
                    Some(&norm(&row["statistics"])),
                    Some(&norm(&row["maximum_statistics"])),
                )
                .unwrap();
            result.push(serde_json::json!({"beatmapId":row["beatmap_id"],"title":s.title,"mods":row["mods"],"websitePp":row["pp"],"calculatedPp":actual.pp,"delta":actual.pp-row["pp"].as_f64().unwrap(),"stars":actual.stars}));
        }
        println!("{}", serde_json::to_string_pretty(&result).unwrap());
        return;
    }
    if args[1] == "--map-audit" {
        let root = PathBuf::from(&args[2]);
        let listing = osu_db::Listing::from_file(root.join("osu!.db")).expect("Read map catalog");
        let maps: std::collections::HashMap<_, _> = listing
            .beatmaps
            .iter()
            .filter_map(|m| m.hash.as_deref().map(|h| (h.to_ascii_lowercase(), m)))
            .collect();
        println!(
            "Catalog entries: {}; unique hashes: {}",
            listing.beatmaps.len(),
            maps.len()
        );
        if let Ok(scores) = osu_db::ScoreList::from_file(root.join("scores.db")) {
            let hashes: std::collections::HashSet<_> = scores
                .beatmaps
                .iter()
                .filter_map(|m| m.hash.as_deref())
                .collect();
            println!(
                "Score map hashes: {}; matched in catalog: {}",
                hashes.len(),
                hashes
                    .iter()
                    .filter(|h| maps.contains_key(&h.to_ascii_lowercase()))
                    .count()
            );
        }
        for hash in args.iter().skip(3) {
            if let Some(m) = maps.get(&hash.to_ascii_lowercase()) {
                println!(
                    "{hash}: {:?} / {:?}; folder {:?}; file {:?}",
                    m.title_ascii, m.difficulty_name, m.folder_name, m.file_name
                );
            } else {
                println!("{hash}: absent from catalog");
            }
        }
        return;
    }
    if args[1] == "--profile" {
        if args.len() < 4 {
            eprintln!("Usage: inspect-library --profile <stable-folder> <app-cache.sqlite>");
            std::process::exit(2);
        }
        let listing = osu_db::Listing::from_file(PathBuf::from(&args[2]).join("osu!.db"))
            .expect("Read map listing profile");
        if let Some(player) = listing.player_name {
            let c = rusqlite::Connection::open(&args[3]).expect("Open app cache");
            c.execute(
                "INSERT OR REPLACE INTO metadata VALUES('default_player',?1)",
                [player],
            )
            .expect("Remember default player");
            println!("Saved the stable profile name to the app cache");
        }
        return;
    }
    if args[1] == "--replay-audit" {
        let path = PathBuf::from(&args[2]);
        let started = std::time::Instant::now();
        score_explorer::replays::refresh_lazer(&path).expect("Index local replay references");
        let l = score_explorer::library::load(&path).expect("Read cache");
        for (label, source, native) in [
            ("stable", "stable", false),
            ("lazer legacy", "lazer", false),
            ("lazer native", "lazer", true),
        ] {
            let mut scores: Vec<_> = l
                .scores
                .iter()
                .filter(|s| s.source == source && s.gameplay_lazer == native)
                .collect();
            scores.sort_by(|a, b| a.date.cmp(&b.date));
            if native {
                scores.reverse();
            }
            let ids: Vec<_> = scores.iter().take(100).map(|s| s.id.clone()).collect();
            let available = score_explorer::replays::availability(&path, &ids, None)
                .expect("Check replay files");
            println!(
                "{label}: {} verified local replays among {} checked",
                available.available.len(),
                ids.len()
            );
            if let (Some(folder), Some(replay)) = (args.get(3), available.available.first()) {
                let saved =
                    score_explorer::replays::export(&path, &replay.id, &PathBuf::from(folder))
                        .expect("Export replay");
                println!(
                    "{label}: exported {} bytes",
                    std::fs::metadata(saved).unwrap().len()
                );
            }
        }
        println!(
            "Replay audit completed in {} ms",
            started.elapsed().as_millis()
        );
        return;
    }
    if args[1] == "--audio-audit" {
        let path = PathBuf::from(&args[2]);
        score_explorer::replays::refresh_lazer(&path).expect("Index local media");
        let l = score_explorer::library::load(&path).unwrap();
        println!("Configured game folders: {}", l.game_sources.len());
        for source in ["stable", "lazer"] {
            let mut found = false;
            for s in l.scores.iter().filter(|s| s.source == source).take(100) {
                if let Some(file) = score_explorer::audio::track(&path, &s.id).unwrap() {
                    println!(
                        "{source}: resolved installed audio ({} bytes)",
                        std::fs::metadata(file).unwrap().len()
                    );
                    found = true;
                    break;
                }
            }
            assert!(found, "Need installed audio from {source}");
        }
        return;
    }
    if args[1] == "--lazer" {
        if args.len() < 4 {
            eprintln!("Usage: inspect-library --lazer <cache.sqlite> <lazer-folder>");
            std::process::exit(2);
        }
        match score_explorer::lazer::import(
            &PathBuf::from(&args[2]),
            &PathBuf::from(&args[3]),
            false,
        ) {
            Ok(l) => println!(
                "Combined: {}; stable: {}; lazer: {}; native lazer: {}; merged: {}; calculations: {}; unchanged: {}; scan {} ms",
                l.scores.len(),
                l.stable_scores,
                l.lazer_scores,
                l.native_lazer_scores,
                l.merged_duplicates,
                l.calculated,
                l.source_unchanged,
                l.scan_ms
            ),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        return;
    }
    if args[1] == "--load" {
        let start = std::time::Instant::now();
        let library = score_explorer::library::load(&PathBuf::from(&args[2])).expect("Read cache");
        let mut modes = [0; 4];
        for s in &library.scores {
            if s.mode < 4 {
                modes[s.mode as usize] += 1;
            }
        }
        println!(
            "Loaded {} cached scores in {} ms; mode counts: {:?}",
            library.scores.len(),
            start.elapsed().as_millis(),
            modes
        );
        return;
    }
    if args[1] == "--audit" {
        let l =
            score_explorer::library::load(&PathBuf::from(&args[2])).expect("Load audit library");
        let player = l.default_player.as_deref().unwrap_or("");
        let personal: Vec<_> = l
            .scores
            .iter()
            .filter(|s| s.player == player || s.player_aliases.iter().any(|p| p == player))
            .collect();
        let mut periods = std::collections::BTreeMap::<String, usize>::new();
        let mut players = std::collections::BTreeMap::<String, usize>::new();
        for s in &personal {
            *players.entry(s.player.clone()).or_default() += 1;
        }
        println!("Selected player: {player}; matched names: {players:?}");
        for s in personal.iter().filter(|s| s.mode == 0 && s.gameplay_lazer) {
            *periods.entry(s.date[..7].to_string()).or_default() += 1;
        }
        println!(
            "Canonical records: {}; raw records: {}; links: {}; personal records: {}; personal native lazer: {}; personal standard lazer periods: {:?}",
            l.scores.len(),
            l.stable_scores + l.lazer_scores,
            l.merged_duplicates,
            personal.len(),
            personal.iter().filter(|s| s.gameplay_lazer).count(),
            periods
        );
        return;
    }
    if args[1] == "--identities" {
        if args.len() < 4 {
            eprintln!("Usage: inspect-library --identities <stable-folder> <output.json>");
            std::process::exit(2);
        }
        let list = osu_db::ScoreList::from_file(PathBuf::from(&args[2]).join("scores.db"))
            .expect("Read stable scores");
        let rows:Vec<_>=list.beatmaps.iter().flat_map(|m|m.scores.iter()).map(|s|serde_json::json!({"hash":s.beatmap_hash,"mode":s.mode as u8,"date":s.timestamp.timestamp_millis(),"legacyId":s.online_score_id.to_string(),"player":s.player_name,"raw":s.score,"mods":s.mods.bits(),"combo":s.max_combo,"n300":s.count_300,"n100":s.count_100,"n50":s.count_50,"geki":s.count_geki,"katsu":s.count_katsu,"miss":s.count_miss})).collect();
        std::fs::write(&args[3], serde_json::to_string(&rows).unwrap())
            .expect("Write research inventory");
        println!(
            "Exported {} identity records for local overlap analysis",
            rows.len()
        );
        return;
    }
    match score_explorer::library::import(
        &PathBuf::from(&args[1]),
        &PathBuf::from(&args[2]),
        args.get(3).map(PathBuf::from).as_deref(),
    ) {
        Ok(l) => {
            let ready = l.scores.iter().filter(|s| s.pp.is_some()).count();
            println!(
                "Scores: {}; calculated: {}; reused: {}; PP available: {}; artwork references: {}; unchanged source: {}; unchanged records: {}; new records: {}; scan: {} ms",
                l.scores.len(),
                l.calculated,
                l.reused,
                ready,
                l.scores.iter().filter(|s| s.has_artwork).count(),
                l.source_unchanged,
                l.unchanged,
                l.new_scores,
                l.scan_ms
            );
            for w in l.warnings {
                println!("Warning: {w}");
            }
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
