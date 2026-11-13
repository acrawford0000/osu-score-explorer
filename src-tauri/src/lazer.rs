use crate::library::{self, Library, Score};
use rusqlite::{OptionalExtension, params};
use serde::Deserialize;
use serde::de::DeserializeSeed;
use serde_json::Value;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub const ENGINE: &str = "rosu-pp 4.0.1 / lazer / policy-1";
const ADAPTER: &str = "realm-adapter-3";
fn same_inputs(saved: &str, current: &str) -> bool {
    let (Ok(mut old), Ok(mut new)) = (
        serde_json::from_str::<Value>(saved),
        serde_json::from_str::<Value>(current),
    ) else {
        return false;
    };
    // Adapter metadata does not change PP when all decoded calculation inputs match.
    if let (Some(a), Some(b)) = (old.as_array_mut(), new.as_array_mut())
        && !a.is_empty()
        && !b.is_empty()
    {
        a[0] = Value::Null;
        b[0] = Value::Null;
    } else {
        return false;
    }
    old == new
}
#[derive(Deserialize)]
struct Export {
    schema: u64,
    rows: Vec<Row>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Row {
    id: String,
    legacy: bool,
    legacy_id: Option<String>,
    online_id: Option<String>,
    player_id: Option<u64>,
    player: String,
    date: String,
    mode: u8,
    map_hash: String,
    map_sha: String,
    map_path: Option<String>,
    artwork: Option<String>,
    title: String,
    artist: String,
    difficulty: String,
    mapper: String,
    stats: HashMap<String, u32>,
    maximum: HashMap<String, u32>,
    mods: Value,
    combo: u32,
    raw_score: u64,
    legacy_score: Option<u64>,
    accuracy: f64,
}
pub(crate) fn runtime() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let candidates = [
        exe.parent().unwrap().join("Score Explorer.runtime"),
        exe.parent().unwrap().join("runtime"),
    ];
    let development = {
        #[cfg(debug_assertions)]
        {
            Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../runtime"))
        }
        #[cfg(not(debug_assertions))]
        {
            None::<PathBuf>
        }
    };
    candidates.into_iter().chain(development).find(|p|p.join("node.exe").is_file()&&p.join("import-lazer.cjs").is_file()).ok_or_else(||"The lazer reader runtime is missing. Keep Score Explorer.runtime next to the application.".into())
}
pub(crate) fn snapshot(source: &Path, target: &Path) -> Result<(), String> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1);
    }
    let mut input = options.open(source).map_err(|e| {
        format!("Cannot obtain a stable read of client.realm. Close osu!lazer and retry: {e}")
    })?;
    let mut output = fs::File::create(target).map_err(|e| e.to_string())?;
    std::io::copy(&mut input, &mut output).map_err(|e| e.to_string())?;
    output.sync_all().map_err(|e| e.to_string())
}
fn mod_bits(mods: &Value) -> u32 {
    let mut bits = 0;
    for m in mods.as_array().into_iter().flatten() {
        bits |= match m["acronym"].as_str().unwrap_or("") {
            "NF" => 1,
            "EZ" => 2,
            "TD" => 4,
            "HD" => 8,
            "HR" => 16,
            "SD" => 32,
            "DT" => 64,
            "RX" => 128,
            "HT" | "DC" => 256,
            "NC" => 576,
            "FL" => 1024,
            "AT" => 2048,
            "SO" => 4096,
            "AP" => 8192,
            "PF" => 16416,
            "4K" | "K4" => 32768,
            "5K" | "K5" => 65536,
            "6K" | "K6" => 131072,
            "7K" | "K7" => 262144,
            "8K" | "K8" => 524288,
            "FI" => 1048576,
            "RD" => 2097152,
            "9K" | "K9" => 16777216,
            "1K" | "K1" => 67108864,
            "3K" | "K3" => 134217728,
            "2K" | "K2" => 268435456,
            "V2" => 536870912,
            "MR" => 1073741824,
            _ => 0,
        };
    }
    bits
}
fn display_mods(mods: &Value) -> String {
    let names: Vec<_> = mods
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m["acronym"].as_str())
        .collect();
    if names.is_empty() {
        "NM".into()
    } else {
        names.join("")
    }
}
fn safe_store_file(root: &Path, file: &Option<String>) -> Option<PathBuf> {
    let file = PathBuf::from(file.as_ref()?).canonicalize().ok()?;
    let base = root.join("files").canonicalize().ok()?;
    file.starts_with(base).then_some(file)
}
fn parsed_mods(row: &Row) -> Result<rosu_mods::GameMods, String> {
    let mode = match row.mode {
        0 => rosu_mods::GameMode::Osu,
        1 => rosu_mods::GameMode::Taiko,
        2 => rosu_mods::GameMode::Catch,
        3 => rosu_mods::GameMode::Mania,
        _ => return Err("Unsupported ruleset".into()),
    };
    let text = row.mods.to_string();
    let mut d = serde_json::Deserializer::from_str(&text);
    rosu_mods::serde::GameModsSeed::Mode {
        mode,
        deny_unknown_fields: true,
    }
    .deserialize(&mut d)
    .map_err(|e| format!("Unsupported mod settings: {e}"))
}
fn attrs(
    map: &rosu_pp::Beatmap,
    row: &Row,
    mods: rosu_mods::GameMods,
) -> Option<rosu_pp::any::DifficultyAttributes> {
    let d = rosu_pp::Difficulty::new().mods(mods).lazer(!row.legacy);
    use rosu_pp::any::DifficultyAttributes as A;
    match row.mode {
        0 => d
            .checked_calculate_for_mode::<rosu_pp::osu::Osu>(map)
            .ok()
            .map(A::Osu),
        1 => d
            .checked_calculate_for_mode::<rosu_pp::taiko::Taiko>(map)
            .ok()
            .map(A::Taiko),
        2 => d
            .checked_calculate_for_mode::<rosu_pp::catch::Catch>(map)
            .ok()
            .map(A::Catch),
        3 => d
            .checked_calculate_for_mode::<rosu_pp::mania::Mania>(map)
            .ok()
            .map(A::Mania),
        _ => None,
    }
}
fn perf(
    attrs: rosu_pp::any::DifficultyAttributes,
    s: &Score,
    row: &Row,
    mods: rosu_mods::GameMods,
    hypothetical: bool,
) -> f64 {
    let count = |key: &str| {
        if hypothetical {
            row.maximum
                .get(key)
                .copied()
                .unwrap_or_else(|| row.stats.get(key).copied().unwrap_or(0))
        } else {
            row.stats.get(key).copied().unwrap_or(0)
        }
    };
    let mut p = attrs
        .performance()
        .mods(mods)
        .lazer(!row.legacy)
        .combo(s.combo)
        .n300(s.n300)
        .n100(s.n100)
        .n50(s.n50)
        .n_geki(s.n_geki)
        .n_katu(s.n_katu)
        .misses(s.misses);
    if !row.legacy {
        p = p
            .large_tick_hits(count("LargeTickHit"))
            .small_tick_hits(count("SmallTickHit"))
            .slider_end_hits(count("SliderTailHit"));
    } else if !hypothetical {
        p = p.legacy_total_score(s.raw_score);
    }
    p.calculate().pp()
}
pub(crate) fn recover(
    c: &rusqlite::Connection,
    score: &mut Score,
    map: &rosu_pp::Beatmap,
) -> Result<(), String> {
    let input = crate::sources::value(c, &format!("lazer-input:{}", score.id))
        .and_then(|v| serde_json::from_str::<Value>(&v).ok());
    let Some(input) = input.filter(|v| v.as_array().is_some_and(|a| a.len() >= 11)) else {
        score.status = "Rescan lazer to recover the original calculation inputs".into();
        return Ok(());
    };
    let row: Row=serde_json::from_value(serde_json::json!({
        "id":score.id,"legacy":input[9],"legacyId":null,"onlineId":null,"playerId":score.player_id,
        "player":score.player,"date":score.date,"mode":score.mode,"mapHash":score.map_hash,
        "mapSha":input[7],"mapPath":null,"artwork":null,"title":score.title,"artist":score.artist,
        "difficulty":score.difficulty,"mapper":score.mapper,"stats":input[3],"maximum":input[4],
        "mods":input[5],"combo":score.combo,"rawScore":score.raw_score,"legacyScore":input[10],"accuracy":score.accuracy.unwrap_or(0.0)
    })).map_err(|_|"Cached lazer inputs could not be read")?;
    if score.mods & (128 | 2048 | 8192) != 0 {
        score.status = "Unranked automation mod; PP unavailable".into();
        return Ok(());
    }
    let mods = match parsed_mods(&row) {
        Ok(m) => m,
        Err(e) => {
            score.status = e;
            return Ok(());
        }
    };
    let Some(a) = attrs(map, &row, mods.clone()) else {
        score.status = "Map calculation or conversion unavailable".into();
        return Ok(());
    };
    score.max_combo = Some(a.max_combo());
    score.stars = Some(a.stars());
    if !library::complete(&a, score) {
        score.status = "Incomplete play or judgments do not match the map".into();
        return Ok(());
    }
    let actual = perf(a.clone(), score, &row, mods.clone(), false);
    let mut fc = score.clone();
    fc.misses = 0;
    fc.combo = a.max_combo();
    match &a {
        rosu_pp::any::DifficultyAttributes::Catch(attrs) => {
            fc.n300 = attrs.n_fruits;
            fc.n100 = attrs.n_droplets;
        }
        rosu_pp::any::DifficultyAttributes::Mania(_) => fc.n_geki += score.misses,
        _ => fc.n300 += score.misses,
    }
    let estimate = perf(a.clone(), &fc, &row, mods, true);
    if actual.is_finite() && estimate.is_finite() && a.stars().is_finite() {
        score.pp = Some(actual);
        score.fc_pp = Some(estimate);
        score.fc = score.misses == 0 && score.combo == a.max_combo();
        score.calculation_version = ENGINE.into();
        score.status = "Ready".into();
    }
    Ok(())
}
pub fn import(path: &Path, root: &Path, force: bool) -> Result<Library, String> {
    let started = std::time::Instant::now();
    let source = root.join("client.realm");
    let fingerprint = format!("{ENGINE}|{ADAPTER}|{}", library::stamp(&source)?);
    let mut c = library::db(path)?;
    let install = crate::sources::scope(&c, "lazer", root)?;
    let previous: Option<String> = c
        .query_row(
            "SELECT data FROM metadata WHERE key=?1",
            [format!("lazer_stamp:{install}")],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if !force && previous.as_deref() == Some(&fingerprint) {
        let mut l = library::load(path)?;
        l.source_unchanged = true;
        l.unchanged = l.scores.len();
        l.scan_ms = started.elapsed().as_millis();
        return Ok(l);
    }
    let runtime = runtime()?;
    let scratch = path.parent().unwrap().join("lazer-import");
    fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;
    let snapshot_path = scratch.join("snapshot.realm");
    let export_path = scratch.join("scores.json");
    snapshot(&source, &snapshot_path)?;
    let mut command = Command::new(runtime.join("node.exe"));
    command
        .arg(runtime.join("import-lazer.cjs"))
        .arg(&snapshot_path)
        .arg(root)
        .arg(&export_path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let out = command
        .output()
        .map_err(|e| format!("Cannot start lazer reader: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "Cannot read lazer snapshot: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let exported: Export =
        serde_json::from_slice(&fs::read(&export_path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let known: HashMap<String, String> = {
        let mut q = c
            .prepare("SELECT id,data FROM scores WHERE id LIKE 'lazer:%'")
            .map_err(|e| e.to_string())?;
        q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?
    };
    let tx = c.transaction().map_err(|e| e.to_string())?;
    let mut calculated = 0;
    let mut unchanged = 0;
    let mut new_scores = 0;
    let mut maps: HashMap<String, Option<rosu_pp::Beatmap>> = HashMap::new();
    let mut diffs: HashMap<String, Option<rosu_pp::any::DifficultyAttributes>> = HashMap::new();
    for row in &exported.rows {
        if row.mode > 3 {
            continue;
        }
        let id = format!("lazer:{}", row.id);
        let input = serde_json::to_string(&serde_json::json!([
            ADAPTER,
            ENGINE,
            row.date,
            row.stats,
            row.maximum,
            row.mods,
            row.combo,
            row.map_sha,
            row.map_hash,
            row.legacy,
            row.legacy_score
        ]))
        .unwrap();
        let input_key = format!("lazer-input:{id}");
        let saved: Option<String> = tx
            .query_row(
                "SELECT data FROM metadata WHERE key=?1",
                [&input_key],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if !force
            && known.contains_key(&id)
            && saved.as_deref().is_some_and(|old| same_inputs(old, &input))
        {
            tx.execute(
                "INSERT OR REPLACE INTO metadata VALUES(?1,?2)",
                params![input_key, input],
            )
            .map_err(|e| e.to_string())?;
            let mut old: Score = serde_json::from_str(&known[&id]).map_err(|e| e.to_string())?;
            let image =
                safe_store_file(root, &row.artwork).map(|p| p.to_string_lossy().to_string());
            if let Some(image) = &image {
                tx.execute(
                    "INSERT OR REPLACE INTO score_artwork VALUES(?1,?2,?3)",
                    params![id, install, image],
                )
                .map_err(|e| e.to_string())?;
            }
            if old.player != row.player
                || old.player_id != row.player_id
                || old.title != row.title
                || old.artist != row.artist
                || old.difficulty != row.difficulty
                || old.mapper != row.mapper
                || old.solo_online_id != row.online_id
                || old.has_artwork != image.is_some()
            {
                old.player = row.player.clone();
                old.player_id = row.player_id;
                old.title = row.title.clone();
                old.artist = row.artist.clone();
                old.difficulty = row.difficulty.clone();
                old.mapper = row.mapper.clone();
                old.solo_online_id = row.online_id.clone();
                old.has_artwork = image.is_some();
                tx.execute(
                    "INSERT OR REPLACE INTO scores VALUES(?1,?2,?3)",
                    params![id, serde_json::to_string(&old).unwrap(), image],
                )
                .map_err(|e| e.to_string())?;
            }
            unchanged += 1;
            continue;
        }
        if !known.contains_key(&id) {
            new_scores += 1;
        }
        let count = |key: &str| row.stats.get(key).copied().unwrap_or(0);
        let mut score = Score {
            id: id.clone(),
            map_hash: if row.map_hash.is_empty() {
                row.map_sha.clone()
            } else {
                row.map_hash.clone()
            },
            player: row.player.clone(),
            date: row.date.clone(),
            mode: row.mode,
            title: row.title.clone(),
            artist: row.artist.clone(),
            difficulty: row.difficulty.clone(),
            mapper: row.mapper.clone(),
            mods: mod_bits(&row.mods),
            n300: count("Great"),
            n100: if row.mode == 0 {
                row.stats
                    .get("Ok")
                    .copied()
                    .unwrap_or_else(|| count("Good"))
            } else {
                count(if row.mode == 2 { "LargeTickHit" } else { "Ok" })
            },
            n50: count(if row.mode == 2 { "SmallTickHit" } else { "Meh" }),
            misses: count("Miss")
                + if row.mode == 2 {
                    count("LargeTickMiss")
                } else {
                    0
                },
            n_geki: if row.mode == 3 { count("Perfect") } else { 0 },
            n_katu: if row.mode == 3 {
                count("Good")
            } else if row.mode == 2 {
                count("SmallTickMiss")
            } else {
                0
            },
            calculation_version: ENGINE.into(),
            record_version: 3,
            legacy_online_id: row.legacy_id.clone(),
            replay_hash: None,
            combo: row.combo,
            max_combo: None,
            raw_score: row
                .legacy_score
                .unwrap_or(row.raw_score)
                .min(u32::MAX as u64) as u32,
            accuracy: Some(row.accuracy),
            fc: false,
            ss: row.accuracy == 100.,
            stars: None,
            pp: None,
            fc_pp: None,
            source: "lazer".into(),
            status: "Exact map file unavailable".into(),
            has_artwork: false,
            sources: vec!["lazer".into()],
            player_id: row.player_id,
            player_aliases: vec![],
            mods_display: display_mods(&row.mods),
            mods_json: row.mods.to_string(),
            gameplay_lazer: !row.legacy,
            solo_online_id: row.online_id.clone(),
        };
        let image = safe_store_file(root, &row.artwork).map(|p| p.to_string_lossy().to_string());
        if let Some(image) = &image {
            tx.execute(
                "INSERT OR REPLACE INTO score_artwork VALUES(?1,?2,?3)",
                params![id, install, image],
            )
            .map_err(|e| e.to_string())?;
        }
        score.has_artwork = image.is_some();
        let mod_result = parsed_mods(row);
        if !maps.contains_key(&row.map_sha) {
            let map = safe_store_file(root, &row.map_path)
                .and_then(|p| fs::read(p).ok())
                .and_then(|bytes| {
                    if !row.map_hash.is_empty()
                        && format!("{:x}", md5::compute(&bytes)) != row.map_hash
                    {
                        return None;
                    }
                    rosu_pp::Beatmap::from_bytes(&bytes).ok()
                });
            maps.insert(row.map_sha.clone(), map);
        }
        if let Err(e) = &mod_result {
            score.status = e.clone();
        } else if score.mods & (128 | 2048 | 8192) != 0 {
            score.status = "Unranked automation mod; PP unavailable".into();
        } else if let (Some(map), Ok(mods)) =
            (maps.get(&row.map_sha).and_then(Option::as_ref), mod_result)
        {
            let diff_key = format!("{}:{}:{}:{}", row.map_sha, row.mode, row.legacy, row.mods);
            let a = diffs
                .entry(diff_key)
                .or_insert_with(|| attrs(map, row, mods.clone()));
            if let Some(a) = a {
                score.max_combo = Some(a.max_combo());
                score.stars = Some(a.stars());
                if library::complete(a, &score) {
                    score.fc = score.misses == 0 && score.combo == a.max_combo();
                    let actual = perf(a.clone(), &score, row, mods.clone(), false);
                    let mut fc = score.clone();
                    fc.misses = 0;
                    fc.combo = a.max_combo();
                    match a {
                        rosu_pp::any::DifficultyAttributes::Catch(attrs) => {
                            fc.n300 = attrs.n_fruits;
                            fc.n100 = attrs.n_droplets;
                        }
                        rosu_pp::any::DifficultyAttributes::Mania(_) => fc.n_geki += score.misses,
                        _ => fc.n300 += score.misses,
                    }
                    let estimate = perf(a.clone(), &fc, row, mods, true);
                    if actual.is_finite() && estimate.is_finite() {
                        score.pp = Some(actual);
                        score.fc_pp = Some(estimate);
                        score.status = "Ready".into();
                        calculated += 1;
                    }
                } else {
                    score.status = "Incomplete play or judgments do not match the map".into();
                }
            } else {
                score.status = "Map calculation or conversion unavailable".into();
            }
        }
        tx.execute(
            "INSERT OR REPLACE INTO scores VALUES(?1,?2,?3)",
            params![id, serde_json::to_string(&score).unwrap(), image],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT OR REPLACE INTO metadata VALUES(?1,?2)",
            params![input_key, input],
        )
        .map_err(|e| e.to_string())?;
    }
    crate::sources::remember(&tx, "lazer", root, None)?;
    tx.execute(
        "INSERT OR REPLACE INTO metadata VALUES(?1,?2)",
        params![format!("lazer_stamp:{install}"), fingerprint],
    )
    .map_err(|e| e.to_string())?;
    for (key, value) in [
        ("lazer_stamp", fingerprint),
        ("lazer_root", root.to_string_lossy().to_string()),
        ("lazer_schema", exported.schema.to_string()),
        ("imported_at", chrono::Utc::now().to_rfc3339()),
    ] {
        tx.execute(
            "INSERT OR REPLACE INTO metadata VALUES(?1,?2)",
            params![key, value],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    rebuild_links(path)?;
    let mut l = library::load(path)?;
    l.calculated = calculated;
    l.unchanged = unchanged;
    l.new_scores = new_scores;
    l.scan_ms = started.elapsed().as_millis();
    let _ = fs::remove_file(snapshot_path);
    let _ = fs::remove_file(export_path);
    Ok(l)
}
fn date_ms(s: &Score) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(&s.date)
        .ok()
        .map(|d| d.timestamp_millis())
}
fn comparable_mods(s: &Score) -> bool {
    if !s.mods_json.is_empty() {
        let v: Value = serde_json::from_str(&s.mods_json).unwrap_or(Value::Null);
        v.as_array().is_some_and(|a| {
            a.iter().all(|m| {
                m["acronym"] == "CL" || m["settings"].as_object().is_none_or(|s| s.is_empty())
            })
        })
    } else {
        true
    }
}
pub fn same_play(a: &Score, b: &Score) -> bool {
    if a.mode != b.mode
        || a.map_hash.is_empty()
        || a.map_hash != b.map_hash
        || b.gameplay_lazer
        || date_ms(a) != date_ms(b)
        || a.raw_score != b.raw_score
        || a.combo != b.combo
        || a.mods != b.mods
        || !comparable_mods(b)
    {
        return false;
    }
    let same = a.n300 == b.n300 && a.n100 == b.n100 && a.misses == b.misses;
    same && match a.mode {
        0 => a.n50 == b.n50,
        1 => true,
        2 => a.n50 == b.n50 && a.n_katu == b.n_katu,
        3 => a.n50 == b.n50 && a.n_geki == b.n_geki && a.n_katu == b.n_katu,
        _ => false,
    }
}
pub fn rebuild_links(path: &Path) -> Result<(), String> {
    let mut c = library::db(path)?;
    let scores: Vec<Score> = {
        let mut q = c
            .prepare("SELECT data FROM scores")
            .map_err(|e| e.to_string())?;
        let rows = q
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        rows.map(|r| {
            serde_json::from_str(&r.map_err(|e| e.to_string())?).map_err(|e| e.to_string())
        })
        .collect::<Result<_, _>>()?
    };
    let mut strong: HashMap<String, Vec<&Score>> = HashMap::new();
    let mut fallback: HashMap<String, Vec<&Score>> = HashMap::new();
    let signature = |s: &Score| {
        format!(
            "{}:{}:{:?}:{}:{}:{}",
            s.mode,
            s.map_hash,
            date_ms(s),
            s.raw_score,
            s.combo,
            s.mods
        )
    };
    for s in scores.iter().filter(|s| s.source == "stable") {
        if let Some(id) = &s.legacy_online_id {
            strong
                .entry(format!("{}:{id}", s.mode))
                .or_default()
                .push(s);
        }
        fallback.entry(signature(s)).or_default().push(s);
    }
    let tx = c.transaction().map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM score_links", [])
        .map_err(|e| e.to_string())?;
    for s in scores
        .iter()
        .filter(|s| s.source == "lazer" && !s.gameplay_lazer)
    {
        let strong_matches: Vec<_> = s
            .legacy_online_id
            .as_ref()
            .and_then(|id| strong.get(&format!("{}:{id}", s.mode)))
            .into_iter()
            .flatten()
            .copied()
            .filter(|candidate| same_play(candidate, s))
            .collect();
        let candidates = if strong_matches.len() == 1 {
            strong_matches
        } else {
            fallback
                .get(&signature(s))
                .into_iter()
                .flatten()
                .copied()
                .filter(|candidate| {
                    candidate.player.eq_ignore_ascii_case(&s.player) && same_play(candidate, s)
                })
                .collect()
        };
        if candidates.len() == 1 {
            tx.execute(
                "INSERT INTO score_links VALUES(?1,?2)",
                params![s.id, candidates[0].id],
            )
            .map_err(|e| e.to_string())?;
        }
    }
    // Importing a replay into another lazer install may assign a new GUID.
    // Merge only exact results for the same known player and gameplay rules.
    let existing: HashMap<String, String> = {
        let mut q = tx
            .prepare("SELECT alias_id,canonical_id FROM score_links")
            .map_err(|e| e.to_string())?;
        q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?
    };
    let mut lazer: Vec<_> = scores.iter().filter(|s| s.source == "lazer").collect();
    lazer.sort_by_key(|s| (!existing.contains_key(&s.id), &s.id));
    let mut plays = HashMap::<String, String>::new();
    for s in lazer {
        if date_ms(s).is_none() || s.map_hash.is_empty() {
            continue;
        }
        let owner = if let Some(id) = s.player_id.filter(|id| *id > 1) {
            format!("id:{id}")
        } else {
            if matches!(
                s.player.to_lowercase().as_str(),
                "guest" | "unknown player" | ""
            ) {
                continue;
            }
            format!("name:{}", s.player.to_lowercase())
        };
        let identity = serde_json::to_string(&serde_json::json!([
            owner,
            s.mode,
            s.map_hash,
            date_ms(s),
            s.gameplay_lazer,
            s.raw_score,
            s.combo,
            s.n300,
            s.n100,
            s.n50,
            s.n_geki,
            s.n_katu,
            s.misses,
            s.mods,
            s.mods_json
        ]))
        .unwrap();
        let canonical = existing.get(&s.id).unwrap_or(&s.id);
        if let Some(first) = plays.get(&identity) {
            if s.id != *first {
                tx.execute(
                    "INSERT OR REPLACE INTO score_links VALUES(?1,?2)",
                    params![s.id, first],
                )
                .map_err(|e| e.to_string())?;
            }
        } else {
            plays.insert(identity, canonical.clone());
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn caches_survive_adapter_metadata_changes_but_not_calculation_input_changes() {
        assert!(same_inputs(
            "[\"adapter-2\",\"engine-1\",{\"Ok\":3}]",
            "[\"adapter-3\",\"engine-1\",{\"Ok\":3}]"
        ));
        assert!(!same_inputs(
            "[\"adapter-2\",\"engine-1\",{\"Ok\":3}]",
            "[\"adapter-3\",\"engine-1\",{\"Ok\":4}]"
        ));
        assert!(!same_inputs(
            "[\"adapter-2\",\"engine-1\"]",
            "[\"adapter-3\",\"engine-2\"]"
        ));
    }
    fn score(id: &str, source: &str) -> Score {
        serde_json::from_value(serde_json::json!({"id":id,"mapHash":"same-map-md5","player":"fixture","date":"2020-01-01T12:00:00.123456Z","mode":0,"title":"Fixture","artist":"Test","difficulty":"Test","mapper":"Test","mods":0,"n300":99,"n100":1,"n50":0,"misses":0,"combo":100,"maxCombo":100,"rawScore":12345,"accuracy":99.0,"fc":true,"ss":false,"stars":null,"pp":null,"fcPp":null,"source":source,"status":"Fixture","hasArtwork":false,"legacyOnlineId":"12345"})).unwrap()
    }
    #[test]
    fn recognizes_legacy_play_across_precision_but_not_native_or_changed_results() {
        let a = score("stable:a", "stable");
        let mut b = score("lazer:b", "lazer");
        b.date = "2020-01-01T12:00:00.123Z".into();
        assert!(same_play(&a, &b));
        b.gameplay_lazer = true;
        assert!(!same_play(&a, &b));
        b.gameplay_lazer = false;
        b.n100 = 2;
        assert!(!same_play(&a, &b));
        b.n100 = 1;
        b.map_hash = "different".into();
        assert!(!same_play(&a, &b));
    }
    #[test]
    fn cached_placeholder_ids_never_join_unrelated_player_histories() {
        let path = std::env::temp_dir().join(format!("sx-players-{}.sqlite", std::process::id()));
        let _ = fs::remove_file(&path);
        {
            let c = library::db(&path).unwrap();
            let mut personal = score("stable:personal", "stable");
            personal.player = "Local player".into();
            let mut imported = personal.clone();
            imported.id = "lazer:imported".into();
            imported.source = "lazer".into();
            imported.player_id = Some(1);
            let mut stranger = score("lazer:stranger", "lazer");
            stranger.player = "WhiteCat".into();
            stranger.player_id = Some(1);
            let mut renamed = score("stable:renamed", "stable");
            renamed.player = "Old name".into();
            let mut online = renamed.clone();
            online.id = "lazer:online".into();
            online.source = "lazer".into();
            online.player = "New name".into();
            online.player_id = Some(42);
            let mut native = online.clone();
            native.id = "lazer:native".into();
            native.gameplay_lazer = true;
            for s in [personal, imported, stranger, renamed, online, native] {
                c.execute(
                    "INSERT INTO scores VALUES(?1,?2,NULL)",
                    params![s.id, serde_json::to_string(&s).unwrap()],
                )
                .unwrap();
            }
            for (alias, canonical) in [
                ("lazer:imported", "stable:personal"),
                ("lazer:online", "stable:renamed"),
            ] {
                c.execute(
                    "INSERT INTO score_links VALUES(?1,?2)",
                    params![alias, canonical],
                )
                .unwrap();
            }
        }
        let l = library::load(&path).unwrap();
        let stranger = l.scores.iter().find(|s| s.player == "WhiteCat").unwrap();
        assert_eq!(stranger.player_id, None);
        assert_eq!(stranger.player_aliases, vec!["WhiteCat"]);
        let personal = l
            .scores
            .iter()
            .find(|s| s.player == "Local player")
            .unwrap();
        assert_eq!(personal.player_aliases, vec!["Local player"]);
        assert_eq!(personal.sources, vec!["lazer", "stable"]);
        let native = l.scores.iter().find(|s| s.id == "lazer:native").unwrap();
        assert_eq!(native.player_aliases, vec!["New name", "Old name"]);
        for suffix in ["", "-wal", "-shm"] {
            let _ = fs::remove_file(format!("{}{suffix}", path.display()));
        }
    }
    #[test]
    fn recognizes_reimported_lazer_copies_without_merging_distinct_players_or_rules() {
        let path =
            std::env::temp_dir().join(format!("sx-lazer-copies-{}.sqlite", std::process::id()));
        let _ = fs::remove_file(&path);
        {
            let c = library::db(&path).unwrap();
            let mut a = score("lazer:a", "lazer");
            a.gameplay_lazer = true;
            a.player_id = Some(42);
            let mut copy = a.clone();
            copy.id = "lazer:b".into();
            let mut other = a.clone();
            other.id = "lazer:c".into();
            other.player_id = Some(43);
            other.player = "Other".into();
            let mut legacy = a.clone();
            legacy.id = "lazer:d".into();
            legacy.gameplay_lazer = false;
            let mut setting = a.clone();
            setting.id = "lazer:e".into();
            setting.mods_json = "[{\"acronym\":\"DA\",\"settings\":{\"circle_size\":4}}]".into();
            for s in [a, copy, other, legacy, setting] {
                c.execute(
                    "INSERT INTO scores VALUES(?1,?2,NULL)",
                    params![s.id, serde_json::to_string(&s).unwrap()],
                )
                .unwrap();
            }
        }
        rebuild_links(&path).unwrap();
        rebuild_links(&path).unwrap();
        let l = library::load(&path).unwrap();
        assert_eq!(l.scores.len(), 4);
        assert_eq!(l.merged_duplicates, 1);
        for suffix in ["", "-wal", "-shm"] {
            let _ = fs::remove_file(format!("{}{suffix}", path.display()));
        }
    }
    #[test]
    fn combined_load_counts_a_duplicate_once_and_keeps_raw_records() {
        let path = std::env::temp_dir().join(format!("sx-merge-{}.sqlite", std::process::id()));
        let _ = fs::remove_file(&path);
        {
            let c = library::db(&path).unwrap();
            for s in [score("stable:a", "stable"), score("lazer:b", "lazer")] {
                c.execute(
                    "INSERT INTO scores VALUES(?1,?2,NULL)",
                    params![s.id, serde_json::to_string(&s).unwrap()],
                )
                .unwrap();
            }
        }
        rebuild_links(&path).unwrap();
        rebuild_links(&path).unwrap();
        let l = library::load(&path).unwrap();
        assert_eq!(l.scores.len(), 1);
        assert_eq!(l.merged_duplicates, 1);
        assert_eq!(l.scores[0].sources, vec!["lazer", "stable"]);
        {
            let c = library::db(&path).unwrap();
            assert_eq!(
                c.query_row("SELECT COUNT(*) FROM scores", [], |r| r.get::<_, i64>(0))
                    .unwrap(),
                2
            );
        }
        for suffix in ["", "-wal", "-shm"] {
            let _ = fs::remove_file(format!("{}{suffix}", path.display()));
        }
    }
}
