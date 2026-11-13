use crate::{library, sources};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LocalMap {
    pub path: String,
    pub root: String,
    pub hash: String,
    pub title: String,
    pub artist: String,
    pub difficulty: String,
    pub mapper: String,
    pub audio: Option<String>,
    pub background: Option<String>,
}
pub struct Index {
    pub maps: HashMap<String, Vec<LocalMap>>,
    pub changed: HashSet<String>,
    pub generation: i64,
    pub warnings: Vec<String>,
}
pub fn schema(c: &Connection) -> Result<(), String> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS beatmap_files(path TEXT PRIMARY KEY,root TEXT NOT NULL,stamp TEXT NOT NULL,hash TEXT NOT NULL,data TEXT NOT NULL,present INTEGER NOT NULL); CREATE INDEX IF NOT EXISTS beatmaps_hash ON beatmap_files(hash,present); CREATE TABLE IF NOT EXISTS beatmap_changes(hash TEXT PRIMARY KEY,generation INTEGER NOT NULL);").map_err(|e|e.to_string())
}
pub fn safe_file(root: &Path, path: &Path) -> Option<PathBuf> {
    let root = root.canonicalize().ok()?;
    let path = path.canonicalize().ok()?;
    (path.starts_with(root) && path.is_file()).then_some(path)
}
fn relative(parent: &Path, value: &str) -> String {
    parent
        .join(value.trim().trim_matches('"').replace('\\', "/"))
        .to_string_lossy()
        .into()
}
pub(crate) fn parse(root: &Path, file: &Path, bytes: &[u8]) -> LocalMap {
    let text = String::from_utf8_lossy(bytes);
    let mut section = "";
    let mut fields = HashMap::new();
    let parent = file.parent().unwrap_or(root);
    let mut image = None;
    for line in text
        .lines()
        .map(|s| s.trim().trim_start_matches('\u{feff}'))
    {
        if line.starts_with('[') {
            section = line;
            continue;
        }
        if matches!(section, "[Metadata]" | "[General]")
            && let Some((key, value)) = line.split_once(':')
        {
            fields.insert(key.trim(), value.trim());
        }
        if section == "[Events]"
            && (line.starts_with("0,") || line.starts_with("Background,"))
            && let Some(name) = line.split('"').nth(1).or_else(|| line.split(',').nth(2))
        {
            image = Some(relative(parent, name));
        }
    }
    let field = |a: &str, b: &str| {
        fields
            .get(a)
            .filter(|v| !v.is_empty())
            .or_else(|| fields.get(b))
            .copied()
            .unwrap_or("")
            .to_string()
    };
    LocalMap {
        path: file.to_string_lossy().into(),
        root: root.to_string_lossy().into(),
        hash: format!("{:x}", md5::compute(bytes)),
        title: field("TitleUnicode", "Title"),
        artist: field("ArtistUnicode", "Artist"),
        difficulty: field("Version", "Version"),
        mapper: field("Creator", "Creator"),
        audio: fields
            .get("AudioFilename")
            .filter(|v| !v.is_empty())
            .map(|v| relative(parent, v)),
        background: image,
    }
}
fn collect(root: &Path, files: &mut HashMap<PathBuf, PathBuf>, warnings: &mut Vec<String>) {
    let Ok(root) = root.canonicalize() else {
        return;
    };
    let mut dirs = vec![root.clone()];
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            warnings.push(format!("Cannot read map folder {}", dir.display()));
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                dirs.push(entry.path());
            } else if kind.is_file()
                && entry
                    .path()
                    .extension()
                    .is_some_and(|v| v.eq_ignore_ascii_case("osu"))
            {
                files.insert(entry.path(), root.clone());
            }
        }
    }
}
// No map bytes, backgrounds or audio are copied. Only hashes, metadata and paths persist.
pub fn refresh(
    c: &mut Connection,
    current_root: &Path,
    current_songs: &Path,
    since: i64,
    force: bool,
) -> Result<Index, String> {
    schema(c)?;
    let mut roots = HashSet::new();
    roots.insert(current_songs.to_path_buf());
    if let Some(root) = sources::value(c, "download_songs") {
        roots.insert(PathBuf::from(root));
    }
    if let Some(paths) = sources::value(c, "download_roots")
        .and_then(|v| serde_json::from_str::<Vec<String>>(&v).ok())
    {
        roots.extend(paths.into_iter().map(PathBuf::from));
    }
    for source in sources::list(c)? {
        if source.kind == "stable" {
            roots.insert(
                source
                    .songs
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from(source.root).join("Songs")),
            );
        }
    }
    // A standalone scores.db folder can point at a different configured map library.
    if current_root.join("Songs").is_dir() {
        roots.insert(current_root.join("Songs"));
    }
    let mut files = HashMap::new();
    let mut media: HashMap<PathBuf, serde_json::Value> = HashMap::new();
    let mut warnings = Vec::new();
    let mut roots: Vec<_> = roots
        .into_iter()
        .filter_map(|p| p.canonicalize().ok())
        .collect();
    roots.sort_by(|a, b| {
        a.components()
            .count()
            .cmp(&b.components().count())
            .then(a.cmp(b))
    });
    roots.dedup();
    for root in roots {
        collect(&root, &mut files, &mut warnings);
    }
    if let Ok(mut q) = c.prepare("SELECT source_key,path,data FROM map_locations") {
        let lazer: HashMap<_, _> = sources::list(c)?
            .into_iter()
            .filter(|s| s.kind == "lazer")
            .map(|s| {
                (
                    sources::key("lazer", Path::new(&s.root)),
                    PathBuf::from(s.root).join("files"),
                )
            })
            .collect();
        if let Ok(rows) = q.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        }) {
            for (key, file, data) in rows.flatten() {
                if let Some(root) = lazer.get(&key)
                    && let Some(file) = safe_file(root, Path::new(&file))
                    && let Ok(root) = root.canonicalize()
                {
                    media.insert(
                        file.clone(),
                        serde_json::from_str(&data).unwrap_or_default(),
                    );
                    files.insert(file, root);
                }
            }
        }
    }
    let old: HashMap<String, (String, String, bool, String)> = {
        let mut q = c
            .prepare("SELECT path,stamp,hash,present,root FROM beatmap_files")
            .map_err(|e| e.to_string())?;
        q.query_map([], |r| {
            Ok((r.get(0)?, (r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?
    };
    let generation = sources::value(c, "map_index_generation")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0)
        + 1;
    let mut touched = HashSet::new();
    let mut seen = HashSet::new();
    let tx = c.transaction().map_err(|e| e.to_string())?;
    for (file, root) in files {
        let path = file.to_string_lossy().to_string();
        seen.insert(path.clone());
        let Ok(stamp) = library::stamp(&file) else {
            continue;
        };
        let fingerprint = format!(
            "maps-1:{stamp}:{}",
            media
                .get(&file)
                .map(serde_json::Value::to_string)
                .unwrap_or_default()
        );
        if !force
            && old.get(&path).is_some_and(|(s, _, present, base)| {
                s == &fingerprint && *present && *base == root.to_string_lossy()
            })
        {
            continue;
        }
        if !fs::metadata(&file).is_ok_and(|m| m.len() <= 20_000_000) {
            continue;
        }
        let Ok(bytes) = fs::read(&file) else { continue };
        if library::stamp(&file).ok().as_deref() != Some(&stamp) {
            continue;
        }
        let mut map = parse(&root, &file, &bytes);
        if let Some(extra) = media.get(&file) {
            map.audio = extra["audio"].as_str().map(String::from);
            map.background = extra["background"].as_str().map(String::from);
        }
        touched.insert(map.hash.clone());
        if let Some((_, hash, _, _)) = old.get(&path) {
            touched.insert(hash.clone());
        }
        tx.execute(
            "INSERT OR REPLACE INTO beatmap_files VALUES(?1,?2,?3,?4,?5,1)",
            params![
                path,
                map.root,
                fingerprint,
                map.hash,
                serde_json::to_string(&map).map_err(|e| e.to_string())?
            ],
        )
        .map_err(|e| e.to_string())?;
    }
    for (path, (_, hash, present, _)) in old {
        if present && !seen.contains(&path) {
            tx.execute("UPDATE beatmap_files SET present=0 WHERE path=?1", [path])
                .map_err(|e| e.to_string())?;
            touched.insert(hash);
        }
    }
    for hash in &touched {
        tx.execute(
            "INSERT OR REPLACE INTO beatmap_changes VALUES(?1,?2)",
            params![hash, generation],
        )
        .map_err(|e| e.to_string())?;
    }
    if !touched.is_empty() {
        tx.execute(
            "INSERT OR REPLACE INTO metadata VALUES('map_index_generation',?1)",
            [generation.to_string()],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    let actual_generation = sources::value(c, "map_index_generation")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let changed = {
        let mut q = c
            .prepare("SELECT hash FROM beatmap_changes WHERE generation>?1")
            .map_err(|e| e.to_string())?;
        q.query_map([since], |r| r.get(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?
    };
    let mut maps: HashMap<String, Vec<LocalMap>> = HashMap::new();
    let mut q = c
        .prepare("SELECT data,present FROM beatmap_files ORDER BY present DESC,path")
        .map_err(|e| e.to_string())?;
    for row in q
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?)))
        .map_err(|e| e.to_string())?
    {
        let (json, present) = row.map_err(|e| e.to_string())?;
        let mut map: LocalMap = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        if !present {
            map.path.clear();
        }
        maps.entry(map.hash.clone()).or_default().push(map);
    }
    Ok(Index {
        maps,
        changed,
        generation: actual_generation,
        warnings,
    })
}
pub fn cached(c: &Connection, hash: &str) -> Result<Vec<LocalMap>, String> {
    let mut q = c
        .prepare("SELECT data FROM beatmap_files WHERE hash=?1 AND present=1 ORDER BY path")
        .map_err(|e| e.to_string())?;
    q.query_map([hash], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .map(|r| serde_json::from_str(&r.map_err(|e| e.to_string())?).map_err(|e| e.to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hashed_lazer_maps_keep_store_media_paths_and_changes_for_each_score_source() {
        let root = std::env::temp_dir().join(format!("sx-store-maps-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let store = root.join("lazer/files");
        fs::create_dir_all(store.join("a/ab")).unwrap();
        let file = store.join("a/ab/map-hash");
        fs::write(&file,"osu file format v14\n[Metadata]\nTitle:Fixture\nTitleUnicode:曲\nArtist:Composer: Remix\nVersion:Test\n[General]\nAudioFilename: original.mp3\n[Events]\n0,0,\"original.jpg\",0,0\n").unwrap();
        let audio = store.join("audio-hash");
        let image = store.join("image-hash");
        fs::write(&audio, b"audio").unwrap();
        fs::write(&image, b"image").unwrap();
        let mut c = library::db(&root.join("cache.sqlite")).unwrap();
        sources::save(
            &c,
            vec![sources::GameSource {
                kind: "lazer".into(),
                root: root.join("lazer").to_string_lossy().into(),
                songs: None,
            }],
        )
        .unwrap();
        c.execute_batch("CREATE TABLE map_locations(source_key TEXT,path TEXT,data TEXT)")
            .unwrap();
        c.execute(
            "INSERT INTO map_locations VALUES(?1,?2,?3)",
            params![
                sources::key("lazer", &root.join("lazer")),
                file.to_string_lossy(),
                serde_json::json!({"audio":audio,"background":image}).to_string()
            ],
        )
        .unwrap();
        let first = refresh(&mut c, &root, &root.join("Songs"), 0, false).unwrap();
        let generation = first.generation;
        let map = first.maps.values().next().unwrap().first().unwrap();
        assert_eq!(map.title, "曲");
        assert_eq!(map.artist, "Composer: Remix");
        assert_eq!(
            safe_file(Path::new(&map.root), Path::new(map.audio.as_ref().unwrap())),
            Some(audio.canonicalize().unwrap())
        );
        assert_eq!(
            safe_file(
                Path::new(&map.root),
                Path::new(map.background.as_ref().unwrap())
            ),
            Some(image.canonicalize().unwrap())
        );
        let hash = map.hash.clone();
        assert!(first.changed.contains(&hash));
        assert!(
            refresh(&mut c, &root, &root.join("Songs"), 0, false)
                .unwrap()
                .changed
                .contains(&hash),
            "another source must still see the cached change"
        );
        assert!(
            refresh(&mut c, &root, &root.join("Songs"), generation, false)
                .unwrap()
                .changed
                .is_empty()
        );
        assert!(safe_file(&store, &root.join("cache.sqlite")).is_none());
        drop(c);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn standalone_scores_heal_from_another_library_without_a_copied_database() {
        let root = std::env::temp_dir().join(format!("sx-shared-maps-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let archive = root.join("friend");
        let install = root.join("game");
        let songs = install.join("Songs");
        fs::create_dir_all(&archive).unwrap();
        fs::create_dir_all(&songs).unwrap();
        let cache = root.join("library.sqlite");
        {
            let c = library::db(&cache).unwrap();
            sources::save(
                &c,
                vec![
                    sources::GameSource {
                        kind: "stable".into(),
                        root: install.to_string_lossy().into(),
                        songs: Some(songs.to_string_lossy().into()),
                    },
                    sources::GameSource {
                        kind: "stable".into(),
                        root: archive.to_string_lossy().into(),
                        songs: None,
                    },
                ],
            )
            .unwrap();
        }
        let objects = (0..16)
            .map(|i| format!("256,192,{},1,0,0:0:0:0:", 1000 + i * 300))
            .collect::<Vec<_>>()
            .join("\n");
        let text = format!(
            "osu file format v14\n[General]\nAudioFilename: audio.mp3\nMode:0\n[Metadata]\nTitle:Shared song\nArtist:Fixture artist\nCreator:Fixture mapper\nVersion:Test difficulty\n[Difficulty]\nHPDrainRate:5\nCircleSize:4\nOverallDifficulty:5\nApproachRate:5\nSliderMultiplier:1.4\nSliderTickRate:1\n[Events]\n0,0,\"background.jpg\",0,0\n[TimingPoints]\n0,500,4,2,0,100,1,0\n[HitObjects]\n{objects}\n"
        );
        let hash = format!("{:x}", md5::compute(text.as_bytes()));
        let replay = osu_db::Replay {
            mode: osu_db::Mode::Standard,
            version: 20171101,
            beatmap_hash: Some(hash.clone()),
            player_name: Some("Friend".into()),
            replay_hash: Some("fixture".into()),
            count_300: 15,
            count_100: 1,
            count_50: 0,
            count_geki: 0,
            count_katsu: 0,
            count_miss: 0,
            score: 123456,
            max_combo: 16,
            perfect_combo: true,
            mods: osu_db::ModSet::from_bits(0),
            life_graph: None,
            timestamp: chrono::DateTime::from_timestamp(1500000000, 0).unwrap(),
            replay_data: None,
            raw_replay_data: None,
            online_score_id: 0,
        };
        osu_db::ScoreList {
            version: 20171101,
            beatmaps: vec![osu_db::score::BeatmapScores {
                hash: Some(hash.clone()),
                scores: vec![replay],
            }],
        }
        .save(archive.join("scores.db"))
        .unwrap();
        let missing = library::import(&cache, &archive, None).unwrap();
        assert_eq!(missing.scores[0].title, "Unmatched beatmap");
        assert!(missing.scores[0].pp.is_none());
        let set = songs.join("renamed mapset folder");
        fs::create_dir_all(&set).unwrap();
        let map = set.join("unusual filename.osu");
        fs::write(&map, &text).unwrap();
        fs::write(set.join("audio.mp3"), b"fixture audio").unwrap();
        fs::write(set.join("background.jpg"), [0xff, 0xd8, 0xff, 1, 2, 3]).unwrap();
        let healed = library::import(&cache, &archive, None).unwrap();
        assert_eq!(healed.scores.len(), 1);
        let score = &healed.scores[0];
        assert_eq!(score.title, "Shared song");
        assert_eq!(score.artist, "Fixture artist");
        assert_eq!(score.mapper, "Fixture mapper");
        assert_eq!(score.max_combo, Some(16));
        assert!(score.pp.is_some());
        assert!(score.stars.is_some());
        assert!(score.fc_pp.is_some());
        assert!(score.has_artwork);
        assert!(
            library::artwork(&cache, &score.id)
                .unwrap()
                .unwrap()
                .starts_with("data:image/jpeg;base64,")
        );
        assert_eq!(
            crate::audio::track(&cache, &score.id).unwrap(),
            Some(set.join("audio.mp3").canonicalize().unwrap())
        );
        assert!(!archive.join("osu!.db").exists());
        let again = library::import(&cache, &archive, None).unwrap();
        assert!(again.source_unchanged);
        assert_eq!(again.calculated, 0);
        let renamed = songs.join("moved again");
        fs::rename(&set, &renamed).unwrap();
        let moved = library::import(&cache, &archive, None).unwrap();
        assert_eq!(moved.calculated, 0);
        assert_eq!(moved.scores[0].pp, score.pp);
        assert_eq!(
            crate::audio::track(&cache, &score.id).unwrap(),
            Some(renamed.join("audio.mp3").canonicalize().unwrap())
        );
        fs::write(
            renamed.join("unusual filename.osu"),
            text + "// a different map revision\n",
        )
        .unwrap();
        let revised = library::import(&cache, &archive, None).unwrap();
        assert_eq!(revised.scores[0].title, "Shared song");
        assert_eq!(
            revised.scores[0].pp, score.pp,
            "keep the cached original result, never calculate it against another revision"
        );
        assert_eq!(revised.calculated, 0);
        let c = library::db(&cache).unwrap();
        assert!(cached(&c, &hash).unwrap().is_empty());
        drop(c);
        fs::remove_dir_all(root).unwrap();
    }
}
