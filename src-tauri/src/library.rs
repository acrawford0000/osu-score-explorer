use base64::{Engine, engine::general_purpose::STANDARD};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

pub const ENGINE: &str = "rosu-pp 4.0.1 / stable / policy-2";
const IMPORT_SCHEMA: u8 = 3;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Score {
    pub id: String,
    pub map_hash: String,
    pub player: String,
    pub date: String,
    pub mode: u8,
    pub title: String,
    pub artist: String,
    pub difficulty: String,
    pub mapper: String,
    pub mods: u32,
    pub n300: u32,
    pub n100: u32,
    pub n50: u32,
    pub misses: u32,
    #[serde(default)]
    pub n_geki: u32,
    #[serde(default)]
    pub n_katu: u32,
    #[serde(default)]
    pub calculation_version: String,
    #[serde(default)]
    pub record_version: u8,
    #[serde(default)]
    pub legacy_online_id: Option<String>,
    #[serde(default)]
    pub replay_hash: Option<String>,
    pub combo: u32,
    pub max_combo: Option<u32>,
    pub raw_score: u32,
    pub accuracy: Option<f64>,
    pub fc: bool,
    pub ss: bool,
    pub stars: Option<f64>,
    pub pp: Option<f64>,
    pub fc_pp: Option<f64>,
    pub source: String,
    pub status: String,
    pub has_artwork: bool,
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default)]
    pub player_id: Option<u64>,
    #[serde(default)]
    pub player_aliases: Vec<String>,
    #[serde(default)]
    pub mods_display: String,
    #[serde(default)]
    pub mods_json: String,
    #[serde(default)]
    pub gameplay_lazer: bool,
    #[serde(default)]
    pub solo_online_id: Option<String>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Library {
    pub player_accounts: Vec<crate::players::Account>,
    pub calculator: String,
    pub scores: Vec<Score>,
    pub warnings: Vec<String>,
    pub engine: String,
    pub imported_at: Option<String>,
    pub default_player: Option<String>,
    pub reused: usize,
    pub calculated: usize,
    pub unchanged: usize,
    pub new_scores: usize,
    pub source_unchanged: bool,
    pub scan_ms: u128,
    pub stable_scores: usize,
    pub lazer_scores: usize,
    pub merged_duplicates: usize,
    pub native_lazer_scores: usize,
    pub lazer_root: Option<String>,
    pub game_sources: Vec<crate::sources::GameSource>,
}
pub(crate) fn db(path: &Path) -> Result<Connection, String> {
    let c = Connection::open(path).map_err(|e| e.to_string())?;
    c.busy_timeout(std::time::Duration::from_secs(20))
        .map_err(|e| e.to_string())?;
    c.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS scores(id TEXT PRIMARY KEY, data TEXT NOT NULL, artwork TEXT); CREATE TABLE IF NOT EXISTS calculations(key TEXT PRIMARY KEY, data TEXT NOT NULL); CREATE TABLE IF NOT EXISTS metadata(key TEXT PRIMARY KEY, data TEXT NOT NULL);").map_err(|e|e.to_string())?;
    c.execute_batch("CREATE TABLE IF NOT EXISTS score_links(alias_id TEXT PRIMARY KEY,canonical_id TEXT NOT NULL); CREATE INDEX IF NOT EXISTS links_canonical ON score_links(canonical_id);").map_err(|e|e.to_string())?;
    c.execute_batch("CREATE TABLE IF NOT EXISTS score_artwork(id TEXT NOT NULL,source_key TEXT NOT NULL,path TEXT NOT NULL,PRIMARY KEY(id,source_key));").map_err(|e|e.to_string())?;
    crate::maps::schema(&c)?;
    crate::downloads::schema(&c)?;
    crate::calculators::schema(&c)?;
    c.execute_batch(
        "CREATE INDEX IF NOT EXISTS scores_map_hash_lower ON scores(lower(json_extract(data,'$.mapHash')));",
    )
    .map_err(|e| e.to_string())?;
    if crate::sources::value(&c, "multi_artwork").is_none() {
        c.execute_batch("BEGIN; INSERT OR IGNORE INTO score_artwork SELECT id,'legacy:'||artwork,artwork FROM scores WHERE artwork IS NOT NULL; INSERT OR REPLACE INTO metadata VALUES('multi_artwork','1'); COMMIT;").map_err(|e|e.to_string())?;
    }
    Ok(c)
}
#[doc(hidden)]
pub fn db_for_inspection(path: &Path) -> Result<Connection, String> {
    db(path)
}
pub fn load(path: &Path) -> Result<Library, String> {
    let c = db(path)?;
    let artwork_ids: HashSet<String> = {
        let mut q = c
            .prepare("SELECT DISTINCT id FROM score_artwork")
            .map_err(|e| e.to_string())?;
        q.query_map([], |r| r.get(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?
    };
    let mut q = c
        .prepare("SELECT data FROM scores ORDER BY id")
        .map_err(|e| e.to_string())?;
    let rows = q
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    let mut scores = Vec::new();
    for r in rows {
        let mut score: Score = serde_json::from_str(&r.map_err(|e| e.to_string())?)
            .map_err(|e| format!("Cached score could not be read: {e}"))?;
        // Lazer legacy/guest records can share the placeholder ID 1 across
        // unrelated names. Normalize old caches too, without recalculating PP.
        score.player_id = score.player_id.filter(|id| *id > 1);
        score.has_artwork |= artwork_ids.contains(&score.id);
        if score.calculation_version != ENGINE
            && score.calculation_version != crate::lazer::ENGINE
            && score.pp.is_some()
        {
            score.pp = None;
            score.fc_pp = None;
            score.stars = None;
            score.max_combo = None;
            score.status = "Older calculation version; rescan to rebuild".into();
        }
        scores.push(score);
    }
    crate::calculators::project(&c, &mut scores)?;
    let stable_scores = scores.iter().filter(|s| s.source == "stable").count();
    let lazer_scores = scores.iter().filter(|s| s.source == "lazer").count();
    let native_lazer_scores = scores
        .iter()
        .filter(|s| s.source == "lazer" && s.gameplay_lazer)
        .count();
    let links: HashMap<String, String> = {
        let mut q = c
            .prepare("SELECT alias_id,canonical_id FROM score_links")
            .map_err(|e| e.to_string())?;
        q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?
    };
    let mut by_id: HashMap<String, Score> = scores
        .into_iter()
        .map(|mut s| {
            s.sources = vec![s.source.clone()];
            (s.id.clone(), s)
        })
        .collect();
    let mut aliases: HashMap<u64, HashSet<String>> = HashMap::new();
    for (alias, canonical) in &links {
        if let (Some(a), Some(s)) = (by_id.get(alias), by_id.get(canonical))
            && let Some(id) = a.player_id
        {
            let names = aliases.entry(id).or_default();
            names.insert(a.player.clone());
            names.insert(s.player.clone());
        }
    }
    for (alias, canonical) in &links {
        if !by_id.contains_key(canonical) {
            continue;
        }
        if let Some(a) = by_id.remove(alias)
            && let Some(s) = by_id.get_mut(canonical)
        {
            if !s.has_artwork && a.has_artwork {
                s.has_artwork = true;
            }
            s.sources.push(a.source);
            s.sources.sort();
            s.sources.dedup();
            s.player_id = s.player_id.or(a.player_id);
        }
    }
    let merged_duplicates = links.len();
    let mut scores: Vec<_> = by_id
        .into_values()
        .map(|mut s| {
            s.player_aliases = s
                .player_id
                .and_then(|id| aliases.get(&id))
                .map(|a| a.iter().cloned().collect())
                .unwrap_or_default();
            s.player_aliases.push(s.player.clone());
            s.player_aliases.sort();
            s.player_aliases.dedup();
            s
        })
        .collect();
    scores.sort_by(|a, b| a.id.cmp(&b.id));
    let get = |k: &str| -> Option<String> {
        c.query_row("SELECT data FROM metadata WHERE key=?1", [k], |r| r.get(0))
            .optional()
            .ok()
            .flatten()
    };
    let player_accounts = crate::players::resolve(&c, &mut scores);
    Ok(Library {
        player_accounts,
        game_sources: crate::sources::list(&c)?,
        scores,
        warnings: get("warnings")
            .and_then(|x| serde_json::from_str(&x).ok())
            .unwrap_or_default(),
        calculator: crate::calculators::selected(&c),
        engine: if crate::calculators::selected(&c) == "lazer" {
            "Lazer 2026 rules · Rust port ba174151".into()
        } else if lazer_scores > 0 {
            "rosu-pp 4.0.1 · stable + lazer".into()
        } else {
            get("engine").unwrap_or_else(|| ENGINE.into())
        },
        imported_at: get("imported_at"),
        default_player: get("default_player").filter(|p| !p.is_empty()),
        reused: 0,
        calculated: 0,
        unchanged: 0,
        new_scores: 0,
        source_unchanged: false,
        scan_ms: 0,
        stable_scores,
        lazer_scores,
        merged_duplicates,
        native_lazer_scores,
        lazer_root: get("lazer_root").or_else(|| {
            std::env::var("APPDATA")
                .ok()
                .map(|p| PathBuf::from(p).join("osu").to_string_lossy().to_string())
        }),
    })
}
fn safe_path(root: &Path, relative: &Path) -> Option<PathBuf> {
    let base = root.canonicalize().ok()?;
    let target = base.join(relative).canonicalize().ok()?;
    target.starts_with(&base).then_some(target)
}
fn background(map_path: &Path, text: &str) -> Option<PathBuf> {
    let parent = map_path.parent()?;
    let mut events = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            events = line == "[Events]";
            continue;
        }
        if events && (line.starts_with("0,") || line.starts_with("Background,")) {
            let name = line.split('"').nth(1).or_else(|| line.split(',').nth(2))?;
            return safe_path(parent, Path::new(name));
        }
    }
    None
}
#[derive(Clone, Serialize, Deserialize)]
struct Calculation {
    stars: f64,
    pp: f64,
    fc_pp: f64,
    max_combo: u32,
}
fn accuracy(
    mode: u8,
    n300: u32,
    n100: u32,
    n50: u32,
    geki: u32,
    katu: u32,
    misses: u32,
) -> Option<f64> {
    let (numerator, denominator) = match mode {
        0 => (
            n300 * 300 + n100 * 100 + n50 * 50,
            (n300 + n100 + n50 + misses) * 300,
        ),
        1 => (n300 * 2 + n100, (n300 + n100 + misses) * 2),
        2 => (n300 + n100 + n50, n300 + n100 + n50 + katu + misses),
        3 => (
            (geki + n300) * 300 + katu * 200 + n100 * 100 + n50 * 50,
            (geki + n300 + katu + n100 + n50 + misses) * 300,
        ),
        _ => return None,
    };
    (denominator > 0).then(|| f64::from(numerator) / f64::from(denominator) * 100.)
}
pub(crate) fn difficulty(
    map: &rosu_pp::Beatmap,
    mode: u8,
    mods: u32,
) -> Option<rosu_pp::any::DifficultyAttributes> {
    let d = rosu_pp::Difficulty::new().mods(mods).lazer(false);
    match mode {
        0 => d
            .checked_calculate_for_mode::<rosu_pp::osu::Osu>(map)
            .ok()
            .map(rosu_pp::any::DifficultyAttributes::Osu),
        1 => d
            .checked_calculate_for_mode::<rosu_pp::taiko::Taiko>(map)
            .ok()
            .map(rosu_pp::any::DifficultyAttributes::Taiko),
        2 => d
            .checked_calculate_for_mode::<rosu_pp::catch::Catch>(map)
            .ok()
            .map(rosu_pp::any::DifficultyAttributes::Catch),
        3 => d
            .checked_calculate_for_mode::<rosu_pp::mania::Mania>(map)
            .ok()
            .map(rosu_pp::any::DifficultyAttributes::Mania),
        _ => None,
    }
}
pub(crate) fn complete(attrs: &rosu_pp::any::DifficultyAttributes, s: &Score) -> bool {
    use rosu_pp::any::DifficultyAttributes::*;
    match attrs {
        Osu(a) => a.n_circles + a.n_sliders + a.n_spinners == s.n300 + s.n100 + s.n50 + s.misses,
        Taiko(a) => a.max_combo == s.n300 + s.n100 + s.misses,
        Catch(a) => {
            a.n_fruits + a.n_droplets == s.n300 + s.n100 + s.misses
                && a.n_tiny_droplets == s.n50 + s.n_katu
        }
        Mania(a) => a.n_objects == s.n300 + s.n100 + s.n50 + s.n_geki + s.n_katu + s.misses,
    }
}
pub(crate) fn performance(attrs: rosu_pp::any::DifficultyAttributes, s: &Score) -> f64 {
    attrs
        .performance()
        .mods(s.mods)
        .lazer(false)
        .combo(s.combo)
        .n300(s.n300)
        .n100(s.n100)
        .n50(s.n50)
        .n_geki(s.n_geki)
        .n_katu(s.n_katu)
        .misses(s.misses)
        .legacy_total_score(s.raw_score)
        .calculate()
        .pp()
}
pub(crate) fn stamp(path: &Path) -> Result<String, String> {
    let metadata =
        fs::metadata(path).map_err(|e| format!("Cannot inspect {}: {e}", path.display()))?;
    let time = metadata
        .modified()
        .map_err(|e| e.to_string())?
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    Ok(format!(
        "{}:{}:{time}",
        path.canonicalize().map_err(|e| e.to_string())?.display(),
        metadata.len()
    ))
}
fn score_id(r: &osu_db::Replay, hash: &str) -> String {
    let fingerprint = format!(
        "{hash}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
        r.mode as u8,
        r.player_name.as_deref().unwrap_or(""),
        r.timestamp.to_rfc3339(),
        r.mods.bits(),
        r.score,
        r.max_combo,
        r.count_300,
        r.count_100,
        r.count_50,
        r.count_miss,
        r.count_geki,
        r.count_katsu,
        r.replay_hash.as_deref().unwrap_or("")
    );
    format!("stable:{:x}", md5::compute(fingerprint.as_bytes()))
}
pub fn import(path: &Path, root: &Path, songs: Option<&Path>) -> Result<Library, String> {
    import_scan(path, root, songs, false)
}
pub fn import_scan(
    path: &Path,
    root: &Path,
    songs: Option<&Path>,
    force: bool,
) -> Result<Library, String> {
    let started = std::time::Instant::now();
    let source = root.join("scores.db");
    let before = stamp(&source)?;
    let catalog_path = root.join("osu!.db");
    let catalog_stamp = stamp(&catalog_path).unwrap_or_else(|_| "missing".into());
    let song_root = songs
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("Songs"));
    let source_key = format!(
        "{ENGINE}|import-{IMPORT_SCHEMA}|{before}|catalog:{catalog_stamp}|{}",
        song_root.display()
    );
    let mut c = db(path)?;
    let install = crate::sources::scope(&c, "stable", root)?;
    let previous_generation =
        crate::sources::value(&c, &format!("stable_map_generation:{install}"))
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
    let media_warning = crate::replays::refresh_lazer(path).err();
    let index = crate::maps::refresh(&mut c, root, &song_root, previous_generation, force)?;
    let source_key = format!("{source_key}|map-index-1:{}", index.generation);
    let configuration = format!(
        "{}|{}|catalog:{catalog_stamp}",
        root.canonicalize().map_err(|e| e.to_string())?.display(),
        song_root.display()
    );
    let previous_configuration: Option<String> = c
        .query_row(
            "SELECT data FROM metadata WHERE key=?1",
            [format!("stable_configuration:{install}")],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let force = force
        || previous_configuration
            .as_ref()
            .is_none_or(|previous| previous != &configuration);
    let last: Option<String> = c
        .query_row(
            "SELECT data FROM metadata WHERE key=?1",
            [format!("stable_stamp:{install}")],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if !force && last.as_deref() == Some(source_key.as_str()) {
        let mut result = load(path)?;
        result.reused = result.scores.iter().filter(|s| s.pp.is_some()).count();
        result.unchanged = result.scores.len();
        result.source_unchanged = true;
        result.scan_ms = started.elapsed().as_millis();
        return Ok(result);
    }
    let score_bytes =
        fs::read(&source).map_err(|e| format!("Cannot read {}: {e}", source.display()))?;
    if stamp(&source)? != before {
        return Err(
            "scores.db changed while being read. Retry when the game has finished saving.".into(),
        );
    }
    // The selected parser reads immutable snapshots in memory and never saves to game files.
    let list = osu_db::ScoreList::from_bytes(&score_bytes)
        .map_err(|e| format!("Unsupported scores.db: {e}"))?;
    let mut warnings = index.warnings.clone();
    if let Some(warning) = media_warning {
        warnings.push(warning);
    }
    let (known, existing): (HashSet<String>, HashSet<String>) = {
        let mut q = c.prepare("SELECT id, json_extract(data,'$.calculationVersion')=?1 AND json_extract(data,'$.recordVersion')=?2 FROM scores").map_err(|e|e.to_string())?;
        let rows = q
            .query_map(params![ENGINE, IMPORT_SCHEMA], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, Option<bool>>(1)?))
            })
            .map_err(|e| e.to_string())?;
        let mut known = HashSet::new();
        let mut existing = HashSet::new();
        for row in rows {
            let (id, current) = row.map_err(|e| e.to_string())?;
            if current == Some(true) {
                known.insert(id.clone());
            }
            existing.insert(id);
        }
        (known, existing)
    };
    let needs_maps = force
        || list.beatmaps.iter().any(|group| {
            group.scores.iter().any(|r| {
                !known.contains(&score_id(
                    r,
                    r.beatmap_hash
                        .as_deref()
                        .or(group.hash.as_deref())
                        .unwrap_or(""),
                ))
            })
        });
    let listing = if needs_maps {
        match osu_db::Listing::from_file(&catalog_path) {
            Ok(l) => Some(l),
            Err(e) => {
                if index.maps.is_empty() {
                    warnings.push(format!("No shared beatmaps were found. Configure a Songs folder or another game library. Metadata lookup: {e}"));
                }
                None
            }
        }
    } else {
        None
    };
    if stamp(&catalog_path).unwrap_or_else(|_| "missing".into()) != catalog_stamp {
        return Err(
            "osu!.db changed while being read. Retry after the file has finished copying.".into(),
        );
    }
    let maps: HashMap<_, _> = listing
        .as_ref()
        .map(|l| {
            l.beatmaps
                .iter()
                .filter_map(|m| m.hash.as_ref().map(|h| (h.as_str(), m)))
                .collect()
        })
        .unwrap_or_default();
    let tx = c.transaction().map_err(|e| e.to_string())?;
    if let Some(player) = listing
        .as_ref()
        .and_then(|l| l.player_name.as_ref())
        .filter(|p| !p.is_empty())
    {
        tx.execute(
            "INSERT OR REPLACE INTO metadata VALUES('default_player',?1)",
            [player],
        )
        .map_err(|e| e.to_string())?;
    }
    let mut reused = 0;
    let mut calculated = 0;
    let mut unchanged = 0;
    let mut records = HashMap::new();
    let mut seen = HashSet::new();
    let mut loaded: HashMap<String, Option<(rosu_pp::Beatmap, String, Option<PathBuf>)>> =
        HashMap::new();
    let mut difficulties: HashMap<String, Option<rosu_pp::any::DifficultyAttributes>> =
        HashMap::new();
    for group in &list.beatmaps {
        for r in &group.scores {
            let hash = r
                .beatmap_hash
                .clone()
                .or_else(|| group.hash.clone())
                .unwrap_or_default();
            let mode = r.mode as u8;
            let timestamp = r.timestamp.to_rfc3339();
            let id = score_id(r, &hash);
            if !seen.insert(id.clone()) {
                continue;
            }
            if !force && known.contains(&id) && !index.changed.contains(&hash.to_ascii_lowercase())
            {
                unchanged += 1;
                continue;
            }
            let m = maps.get(hash.as_str()).copied();
            let shared = index.maps.get(&hash.to_ascii_lowercase());
            let metadata = shared.and_then(|maps| maps.first());
            let previous: Option<Score> =
                if metadata.is_none() && m.is_none() && existing.contains(&id) {
                    tx.query_row("SELECT data FROM scores WHERE id=?1", [&id], |r| {
                        r.get::<_, String>(0)
                    })
                    .optional()
                    .map_err(|e| e.to_string())?
                    .and_then(|s| serde_json::from_str(&s).ok())
                } else {
                    None
                };
            let accuracy = accuracy(
                mode,
                r.count_300.into(),
                r.count_100.into(),
                r.count_50.into(),
                r.count_geki.into(),
                r.count_katsu.into(),
                r.count_miss.into(),
            );
            let mut score = Score {
                id: id.clone(),
                map_hash: hash.clone(),
                player: r
                    .player_name
                    .clone()
                    .unwrap_or_else(|| "Unknown player".into()),
                date: timestamp,
                mode,
                title: metadata
                    .filter(|m| !m.title.is_empty())
                    .map(|m| m.title.clone())
                    .or_else(|| {
                        m.and_then(|m| {
                            m.title_unicode
                                .clone()
                                .filter(|s| !s.is_empty())
                                .or_else(|| m.title_ascii.clone())
                        })
                    })
                    .or_else(|| previous.as_ref().map(|s| s.title.clone()))
                    .unwrap_or_else(|| "Unmatched beatmap".into()),
                artist: metadata
                    .filter(|m| !m.artist.is_empty())
                    .map(|m| m.artist.clone())
                    .or_else(|| m.and_then(|m| m.artist_ascii.clone()))
                    .or_else(|| previous.as_ref().map(|s| s.artist.clone()))
                    .unwrap_or_default(),
                difficulty: metadata
                    .filter(|m| !m.difficulty.is_empty())
                    .map(|m| m.difficulty.clone())
                    .or_else(|| m.and_then(|m| m.difficulty_name.clone()))
                    .or_else(|| previous.as_ref().map(|s| s.difficulty.clone()))
                    .unwrap_or_else(|| hash.clone()),
                mapper: metadata
                    .filter(|m| !m.mapper.is_empty())
                    .map(|m| m.mapper.clone())
                    .or_else(|| m.and_then(|m| m.creator.clone()))
                    .or_else(|| previous.as_ref().map(|s| s.mapper.clone()))
                    .unwrap_or_default(),
                mods: r.mods.bits(),
                n300: r.count_300.into(),
                n100: r.count_100.into(),
                n50: r.count_50.into(),
                n_geki: r.count_geki.into(),
                n_katu: r.count_katsu.into(),
                calculation_version: ENGINE.into(),
                record_version: IMPORT_SCHEMA,
                legacy_online_id: (r.online_score_id > 0 && r.online_score_id < i64::MAX as u64)
                    .then(|| r.online_score_id.to_string()),
                replay_hash: r.replay_hash.clone(),
                misses: r.count_miss.into(),
                combo: r.max_combo.into(),
                max_combo: None,
                raw_score: r.score,
                accuracy,
                fc: r.perfect_combo && r.count_miss == 0,
                ss: accuracy == Some(100.),
                stars: None,
                pp: None,
                fc_pp: None,
                source: "stable".into(),
                status: "Exact beatmap revision not found in configured libraries".into(),
                has_artwork: false,
                sources: vec!["stable".into()],
                player_id: None,
                player_aliases: vec![],
                mods_display: String::new(),
                mods_json: String::new(),
                gameplay_lazer: false,
                solo_online_id: None,
            };
            let key = format!("{ENGINE}:{id}");
            let cached: Option<String> = tx
                .query_row(
                    "SELECT data FROM calculations WHERE key=?1",
                    [&key],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            let mut calc: Option<Calculation> = cached.and_then(|x| serde_json::from_str(&x).ok());
            if !loaded.contains_key(&hash) {
                let mut candidates = Vec::new();
                if let Some(maps) = shared {
                    for map in maps {
                        if !map.path.is_empty() {
                            candidates.push((
                                PathBuf::from(&map.root),
                                PathBuf::from(&map.path),
                                map.background.clone(),
                            ));
                        }
                    }
                }
                if let Some(map) = m
                    && let (Some(folder), Some(file)) = (&map.folder_name, &map.file_name)
                {
                    candidates.push((song_root.clone(), song_root.join(folder).join(file), None));
                }
                let entry = candidates.into_iter().find_map(|(base, file, image)| {
                    let file = crate::maps::safe_file(&base, &file)?;
                    let bytes = fs::read(&file).ok()?;
                    if !format!("{:x}", md5::compute(&bytes)).eq_ignore_ascii_case(&hash) {
                        return None;
                    }
                    let text = String::from_utf8_lossy(&bytes);
                    let image = image
                        .and_then(|p| crate::maps::safe_file(&base, Path::new(&p)))
                        .or_else(|| background(&file, &text));
                    let map = rosu_pp::Beatmap::from_bytes(&bytes).ok()?;
                    Some((map, hash.clone(), image))
                });
                loaded.insert(hash.clone(), entry);
            }
            let entry = loaded.get(&hash).and_then(Option::as_ref);
            let image = entry
                .and_then(|(_, _, p)| p.as_ref())
                .map(|p| p.to_string_lossy().to_string());
            score.has_artwork = image.is_some();
            if calc.is_some() {
                reused += 1;
            } else if score.mods & (128 | 2048 | 8192 | 536870912) == 0 {
                if let Some((map, _, _)) = entry {
                    let diff_key = format!("{hash}:{mode}:{}", score.mods);
                    let attrs = difficulties
                        .entry(diff_key)
                        .or_insert_with(|| difficulty(map, mode, score.mods));
                    if let Some(attrs) = attrs {
                        if complete(attrs, &score) {
                            let actual = performance(attrs.clone(), &score);
                            let mut hypothetical = score.clone();
                            hypothetical.misses = 0;
                            hypothetical.combo = attrs.max_combo();
                            match attrs {
                                rosu_pp::any::DifficultyAttributes::Catch(a) => {
                                    hypothetical.n300 = a.n_fruits;
                                    hypothetical.n100 = a.n_droplets;
                                }
                                rosu_pp::any::DifficultyAttributes::Mania(_) => {
                                    hypothetical.n_geki += score.misses
                                }
                                _ => hypothetical.n300 += score.misses,
                            }
                            let fc = performance(attrs.clone(), &hypothetical);
                            if actual.is_finite() && fc.is_finite() && attrs.stars().is_finite() {
                                calc = Some(Calculation {
                                    stars: attrs.stars(),
                                    pp: actual,
                                    fc_pp: fc,
                                    max_combo: attrs.max_combo(),
                                });
                                tx.execute(
                                    "INSERT OR REPLACE INTO calculations VALUES(?1,?2)",
                                    params![
                                        key,
                                        serde_json::to_string(&calc).map_err(|e| e.to_string())?
                                    ],
                                )
                                .map_err(|e| e.to_string())?;
                                calculated += 1;
                            } else {
                                score.status = "Calculation produced invalid values".into();
                            }
                        } else {
                            score.status =
                                "Incomplete play or judgment counts do not match this map".into();
                        }
                    } else {
                        score.status = "Map could not be safely calculated or converted".into();
                    }
                }
            } else {
                score.status = "PP calculation not yet supported for this mode or mod".into();
            }
            if let Some(calc) = calc {
                score.calculation_version = ENGINE.into();
                score.stars = Some(calc.stars);
                score.pp = Some(calc.pp);
                score.fc_pp = Some(calc.fc_pp);
                score.max_combo = Some(calc.max_combo);
                if score.misses == 0 && score.combo == calc.max_combo {
                    score.fc = true;
                }
                score.status = if entry.is_some() {
                    "Ready"
                } else {
                    "Cached calculation; local map missing"
                }
                .into();
            }
            records.insert(id, (score, image));
        }
    }
    for (id, (score, image)) in &records {
        if let Some(image) = image {
            tx.execute(
                "INSERT OR REPLACE INTO score_artwork VALUES(?1,?2,?3)",
                params![id, install, image],
            )
            .map_err(|e| e.to_string())?;
        }
        tx.execute(
            "INSERT OR REPLACE INTO scores VALUES(?1,?2,?3)",
            params![
                id,
                serde_json::to_string(score).map_err(|e| e.to_string())?,
                image
            ],
        )
        .map_err(|e| e.to_string())?;
    }
    let now = chrono::Utc::now().to_rfc3339();
    tx.execute(
        "INSERT OR REPLACE INTO metadata VALUES(?1,?2)",
        params![
            format!("stable_map_generation:{install}"),
            index.generation.to_string()
        ],
    )
    .map_err(|e| e.to_string())?;
    crate::sources::remember(&tx, "stable", root, songs)?;
    for (field, data) in [("stamp", &source_key), ("configuration", &configuration)] {
        tx.execute(
            "INSERT OR REPLACE INTO metadata VALUES(?1,?2)",
            params![format!("stable_{field}:{install}"), data],
        )
        .map_err(|e| e.to_string())?;
    }
    for (key, value) in [
        ("engine", ENGINE.to_string()),
        ("imported_at", now),
        ("warnings", serde_json::to_string(&warnings).unwrap()),
        ("stable_root", root.to_string_lossy().to_string()),
        ("stable_stamp", source_key),
        ("stable_configuration", configuration),
    ] {
        tx.execute(
            "INSERT OR REPLACE INTO metadata VALUES(?1,?2)",
            params![key, value],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    crate::lazer::rebuild_links(path)?;
    let mut result = load(path)?;
    result.reused = reused;
    result.calculated = calculated;
    result.unchanged = unchanged;
    result.new_scores = records.keys().filter(|id| !existing.contains(*id)).count();
    result.scan_ms = started.elapsed().as_millis();
    Ok(result)
}
pub fn artwork(path: &Path, id: &str) -> Result<Option<String>, String> {
    let c = db(path)?;
    let mut q=c.prepare("SELECT artwork,NULL FROM scores WHERE (id=?1 OR id IN (SELECT alias_id FROM score_links WHERE canonical_id=?1)) AND artwork IS NOT NULL UNION SELECT path,NULL FROM score_artwork WHERE id=?1 OR id IN (SELECT alias_id FROM score_links WHERE canonical_id=?1) UNION SELECT json_extract(data,'$.background'),root FROM beatmap_files WHERE present=1 AND json_extract(data,'$.background') IS NOT NULL AND hash IN (SELECT lower(json_extract(data,'$.mapHash')) FROM scores WHERE id=?1 OR id IN (SELECT alias_id FROM score_links WHERE canonical_id=?1))").map_err(|e|e.to_string())?;
    let files = q
        .query_map([id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })
        .map_err(|e| e.to_string())?;
    for file in files {
        let (file, root) = file.map_err(|e| e.to_string())?;
        let p = if let Some(root) = root {
            let Some(p) = crate::maps::safe_file(Path::new(&root), Path::new(&file)) else {
                continue;
            };
            p
        } else {
            PathBuf::from(file)
        };
        if fs::metadata(&p)
            .map(|m| m.len() > 20_000_000)
            .unwrap_or(true)
        {
            continue;
        }
        let Ok(bytes) = fs::read(&p) else {
            continue;
        };
        let mime = if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
            "image/jpeg"
        } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            "image/png"
        } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
            "image/webp"
        } else {
            continue;
        };
        return Ok(Some(format!(
            "data:{mime};base64,{}",
            STANDARD.encode(bytes)
        )));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_library_is_loadable() {
        let path = std::env::temp_dir().join(format!("sx-test-{}.sqlite", std::process::id()));
        let library = load(&path).unwrap();
        assert!(library.scores.is_empty());
        drop(library);
        let _ = fs::remove_file(path);
    }
    #[test]
    fn relative_paths_cannot_escape() {
        assert!(safe_path(Path::new("."), Path::new("../")).is_none());
    }
    #[test]
    fn incremental_import_skips_unchanged_sources_and_preserves_history() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let folder =
            std::env::temp_dir().join(format!("sx-incremental-{}-{unique}", std::process::id()));
        fs::create_dir(&folder).unwrap();
        let cache = folder.join("app.sqlite");
        let source = folder.join("scores.db");
        let first = osu_db::Replay {
            mode: osu_db::Mode::Standard,
            version: 20171101,
            beatmap_hash: Some("test-map".into()),
            player_name: Some("fixture-player".into()),
            replay_hash: Some("first-replay".into()),
            count_300: 100,
            count_100: 0,
            count_50: 0,
            count_geki: 0,
            count_katsu: 0,
            count_miss: 1,
            score: 123456,
            max_combo: 80,
            perfect_combo: false,
            mods: osu_db::ModSet::from_bits(0),
            life_graph: None,
            timestamp: chrono::DateTime::from_timestamp(1500000000, 0).unwrap(),
            replay_data: None,
            raw_replay_data: None,
            online_score_id: 0,
        };
        let mut list = osu_db::ScoreList {
            version: 20171101,
            beatmaps: vec![osu_db::score::BeatmapScores {
                hash: Some("test-map".into()),
                scores: vec![first.clone()],
            }],
        };
        list.save(&source).unwrap();
        let imported = import(&cache, &folder, None).unwrap();
        assert_eq!(imported.new_scores, 1);
        assert_eq!(imported.scores.len(), 1);
        let unchanged = import(&cache, &folder, None).unwrap();
        assert!(unchanged.source_unchanged);
        assert_eq!(unchanged.calculated, 0);
        // Replacing only osu!.db must repair existing rows, even with unchanged scores.db.
        let mut catalog = osu_db::Listing {
            version: 20200101,
            folder_count: 0,
            unban_date: None,
            player_name: None,
            beatmaps: vec![],
            user_permissions: 1,
        };
        catalog.save(folder.join("osu!.db")).unwrap();
        assert!(!import(&cache, &folder, None).unwrap().source_unchanged);
        let time = chrono::DateTime::from_timestamp(1500000000, 0).unwrap();
        catalog.beatmaps.push(osu_db::listing::Beatmap {
            artist_ascii: Some("Catalog artist".into()),
            artist_unicode: None,
            title_ascii: Some("Recovered title".into()),
            title_unicode: None,
            creator: Some("Mapper".into()),
            difficulty_name: Some("Difficulty".into()),
            audio: None,
            hash: Some("test-map".into()),
            file_name: None,
            status: osu_db::listing::RankedStatus::Unknown,
            hitcircle_count: 0,
            slider_count: 0,
            spinner_count: 0,
            last_modified: time,
            approach_rate: 5.0,
            circle_size: 4.0,
            hp_drain: 5.0,
            overall_difficulty: 5.0,
            slider_velocity: 1.4,
            std_ratings: vec![],
            taiko_ratings: vec![],
            ctb_ratings: vec![],
            mania_ratings: vec![],
            drain_time: 0,
            total_time: 0,
            preview_time: 0,
            timing_points: vec![],
            beatmap_id: 1,
            beatmapset_id: 1,
            thread_id: 0,
            std_grade: osu_db::listing::Grade::Unplayed,
            taiko_grade: osu_db::listing::Grade::Unplayed,
            ctb_grade: osu_db::listing::Grade::Unplayed,
            mania_grade: osu_db::listing::Grade::Unplayed,
            local_beatmap_offset: 0,
            stack_leniency: 0.7,
            mode: osu_db::Mode::Standard,
            song_source: None,
            tags: None,
            online_offset: 0,
            title_font: None,
            last_played: None,
            is_osz2: false,
            folder_name: None,
            last_online_check: time,
            ignore_sounds: false,
            ignore_skin: false,
            disable_storyboard: false,
            disable_video: false,
            visual_override: false,
            mysterious_short: None,
            mysterious_last_modified: 0,
            mania_scroll_speed: 0,
        });
        catalog.save(folder.join("osu!.db")).unwrap();
        let repaired = import(&cache, &folder, None).unwrap();
        assert!(!repaired.source_unchanged);
        assert_eq!(repaired.scores[0].title, "Recovered title");
        assert_eq!(repaired.scores[0].artist, "Catalog artist");
        assert!(
            repaired.scores[0].pp.is_none(),
            "Metadata must not invent PP without a map file"
        );
        assert!(import(&cache, &folder, None).unwrap().source_unchanged);
        let mut second = first.clone();
        second.timestamp += chrono::Duration::seconds(5);
        second.replay_hash = Some("second-replay".into());
        list.beatmaps[0].scores.push(second);
        list.save(&source).unwrap();
        let added = import(&cache, &folder, None).unwrap();
        assert_eq!(added.scores.len(), 2);
        assert_eq!(added.new_scores, 1);
        assert_eq!(added.unchanged, 1);
        list.beatmaps[0].scores = vec![first];
        list.save(&source).unwrap();
        let retained = import(&cache, &folder, None).unwrap();
        assert_eq!(retained.scores.len(), 2);
        assert_eq!(retained.new_scores, 0);
        let other = folder.join("second-install");
        fs::create_dir(&other).unwrap();
        list.save(other.join("scores.db")).unwrap();
        let cloned = import(&cache, &other, None).unwrap();
        assert_eq!(
            cloned.scores.len(),
            2,
            "shared scores must not multiply across installs"
        );
        assert_eq!(cloned.new_scores, 0);
        let mut third = list.beatmaps[0].scores[0].clone();
        third.timestamp += chrono::Duration::seconds(20);
        third.replay_hash = Some("third-replay".into());
        list.beatmaps[0].scores.push(third);
        list.save(other.join("scores.db")).unwrap();
        assert_eq!(import(&cache, &other, None).unwrap().scores.len(), 3);
        assert!(
            import(&cache, &folder, None).unwrap().source_unchanged,
            "switching installs must retain independent stamps"
        );
        assert!(import(&cache, &other, None).unwrap().source_unchanged);
        assert_eq!(load(&cache).unwrap().game_sources.len(), 2);
        fs::remove_dir_all(&other).unwrap();
        for name in [
            "scores.db",
            "app.sqlite",
            "app.sqlite-wal",
            "app.sqlite-shm",
        ] {
            let _ = fs::remove_file(folder.join(name));
        }
        let _ = fs::remove_dir(folder);
    }
    #[test]
    fn mode_specific_accuracy() {
        assert_eq!(accuracy(0, 9, 1, 0, 0, 0, 0), Some(2800. / 3000. * 100.));
        assert_eq!(accuracy(1, 9, 1, 0, 0, 0, 0), Some(95.));
        assert_eq!(accuracy(2, 8, 1, 0, 0, 1, 0), Some(90.));
        assert_eq!(accuracy(3, 1, 0, 0, 8, 1, 0), Some(2900. / 3000. * 100.));
        assert_eq!(accuracy(0, 0, 0, 0, 0, 0, 0), None);
    }
    #[test]
    fn all_rulesets_have_finite_difficulty_and_performance() {
        for mode in 0..4 {
            let hitobjects = (0..16)
                .map(|i| format!("256,192,{},1,0,0:0:0:0:", 1000 + i * 300))
                .collect::<Vec<_>>()
                .join("\n");
            let text = format!(
                "osu file format v14\n[General]\nMode:{mode}\n[Metadata]\nTitle:Fixture\nArtist:Test\nCreator:Test\nVersion:Test\n[Difficulty]\nHPDrainRate:5\nCircleSize:4\nOverallDifficulty:5\nApproachRate:5\nSliderMultiplier:1.4\nSliderTickRate:1\n[TimingPoints]\n0,500,4,2,0,100,1,0\n[HitObjects]\n{hitobjects}\n"
            );
            let map = rosu_pp::Beatmap::from_bytes(text.as_bytes()).unwrap();
            let attrs = difficulty(&map, mode, 0).expect("supported mode");
            let result = attrs.performance().lazer(false).accuracy(100.).calculate();
            assert!(result.pp().is_finite(), "mode {mode}");
            assert!(result.stars().is_finite(), "mode {mode}");
        }
    }
}
