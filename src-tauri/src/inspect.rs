use std::path::PathBuf;
fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: inspect-library <app-owned-cache.sqlite> <stable-folder> [songs-folder]");
        std::process::exit(2);
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
