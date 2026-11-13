use crate::{
    lazer,
    library::{self, Score},
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplayInfo {
    pub id: String,
    pub filename: String,
}
#[derive(Serialize)]
pub struct Availability {
    pub available: Vec<ReplayInfo>,
    pub warning: Option<String>,
}

fn metadata(c: &Connection, key: &str) -> Option<String> {
    c.query_row("SELECT data FROM metadata WHERE key=?1", [key], |r| {
        r.get(0)
    })
    .optional()
    .ok()
    .flatten()
}
fn table(c: &Connection) -> Result<(), String> {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS audio_files(hash TEXT NOT NULL,source_key TEXT NOT NULL,path TEXT NOT NULL,PRIMARY KEY(hash,source_key)); CREATE TABLE IF NOT EXISTS replay_files(id TEXT PRIMARY KEY,path TEXT NOT NULL); CREATE TABLE IF NOT EXISTS replay_locations(id TEXT NOT NULL,source_key TEXT NOT NULL,path TEXT NOT NULL,PRIMARY KEY(id,source_key));",
    )
    .map_err(|e| e.to_string())
}
// Index references from an immutable snapshot, without revisiting PP calculations.
pub fn refresh_lazer(path: &Path) -> Result<(), String> {
    let c = library::db(path)?;
    let mut errors = Vec::new();
    for source in crate::sources::list(&c)?
        .into_iter()
        .filter(|s| s.kind == "lazer")
    {
        if let Err(e) = refresh_one(path, Path::new(&source.root)) {
            errors.push(format!("{}: {e}", source.root));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}
fn refresh_one(path: &Path, root: &Path) -> Result<(), String> {
    let mut c = library::db(path)?;
    table(&c)?;
    let install = crate::sources::key("lazer", root);
    let stamp_key = format!("replay_stamp:{install}");
    let source = root.join("client.realm");
    let fingerprint = format!("media-2:{}:{}", root.display(), library::stamp(&source)?);
    if metadata(&c, &stamp_key).as_deref() == Some(&fingerprint) {
        return Ok(());
    }
    let scratch = path
        .parent()
        .ok_or("Missing cache directory")?
        .join("replay-index")
        .join(&install);
    fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;
    let snapshot = scratch.join("snapshot.realm");
    let output = scratch.join("replays.json");
    lazer::snapshot(&source, &snapshot)?;
    let runtime = lazer::runtime()?;
    let mut command = Command::new(runtime.join("node.exe"));
    command
        .arg(runtime.join("import-lazer.cjs"))
        .arg(&snapshot)
        .arg(root)
        .arg(&output)
        .arg("--replays");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let result = command.output().map_err(|e| e.to_string())?;
    if !result.status.success() {
        return Err(format!(
            "Cannot index lazer replays: {}",
            String::from_utf8_lossy(&result.stderr)
        ));
    }
    #[derive(Deserialize)]
    struct Row {
        id: String,
        path: Option<String>,
    }
    #[derive(Deserialize)]
    struct Export {
        rows: Vec<Row>,
        #[serde(default)]
        audio: Vec<Audio>,
    }
    #[derive(Deserialize)]
    struct Audio {
        hash: String,
        path: String,
    }
    let export: Export = serde_json::from_slice(&fs::read(&output).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let tx = c.transaction().map_err(|e| e.to_string())?;
    tx.execute(
        "DELETE FROM replay_locations WHERE source_key=?1",
        [&install],
    )
    .map_err(|e| e.to_string())?;
    for row in export.rows {
        if let Some(file) = row.path {
            tx.execute(
                "INSERT OR REPLACE INTO replay_locations VALUES(?1,?2,?3)",
                params![row.id, install, file],
            )
            .map_err(|e| e.to_string())?;
        }
    }
    tx.execute("DELETE FROM audio_files WHERE source_key=?1", [&install])
        .map_err(|e| e.to_string())?;
    for audio in export.audio {
        tx.execute(
            "INSERT OR REPLACE INTO audio_files VALUES(?1,?2,?3)",
            params![audio.hash, install, audio.path],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.execute(
        "INSERT OR REPLACE INTO metadata VALUES(?1,?2)",
        [&stamp_key, &fingerprint],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    let _ = fs::remove_file(snapshot);
    let _ = fs::remove_file(output);
    Ok(())
}

fn within(base: &Path, candidate: &Path) -> Option<PathBuf> {
    let base = base.canonicalize().ok()?;
    let file = candidate.canonicalize().ok()?;
    (file.starts_with(base) && file.is_file()).then_some(file)
}
fn stable_filename(s: &Score) -> Option<String> {
    if s.map_hash.len() != 32 || !s.map_hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let date = chrono::DateTime::parse_from_rfc3339(&s.date).ok()?;
    let ticks = i128::from(date.timestamp()) * 10_000_000
        + i128::from(date.timestamp_subsec_nanos() / 100)
        + 116_444_736_000_000_000;
    Some(format!("{}-{ticks}.osr", s.map_hash))
}
fn records(c: &Connection, id: &str) -> Result<Vec<Score>, String> {
    let mut q = c.prepare("SELECT data FROM scores WHERE id=?1 OR id IN (SELECT alias_id FROM score_links WHERE canonical_id=?1) ORDER BY id DESC").map_err(|e|e.to_string())?;
    q.query_map([id], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .map(|r| serde_json::from_str(&r.map_err(|e| e.to_string())?).map_err(|e| e.to_string()))
        .collect()
}
fn candidates(c: &Connection, score: &Score) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for source in crate::sources::list(c)
        .unwrap_or_default()
        .into_iter()
        .filter(|s| s.kind == score.source)
    {
        if score.source == "stable" {
          if let Some(name) = stable_filename(score) {
            let base = PathBuf::from(&source.root).join("Data/r");
            if let Some(file) = within(&base, &base.join(name)) {
                files.push(file);
            }
          }
        } else if let Ok(mut q)=c.prepare("SELECT path FROM replay_locations WHERE id=?1 AND source_key=?2 UNION SELECT path FROM replay_files WHERE id=?1") {
            let install=crate::sources::key("lazer",Path::new(&source.root));
            if let Ok(rows)=q.query_map(params![score.id,install],|r|r.get::<_,String>(0)) {
                for file in rows.flatten() {if let Some(file)=within(&PathBuf::from(&source.root).join("files"),Path::new(&file)) {files.push(file);}}
            }
        }
    }
    files
}
fn verified(file: &Path, score: &Score) -> Result<Vec<u8>, String> {
    let len = fs::metadata(file).map_err(|e| e.to_string())?.len();
    if len > 64 * 1024 * 1024 {
        return Err("Replay exceeds the supported 64 MB limit".into());
    }
    let bytes = fs::read(file).map_err(|e| e.to_string())?;
    let replay =
        osu_db::Replay::from_bytes(&bytes).map_err(|e| format!("Cannot read replay: {e}"))?;
    let date = chrono::DateTime::parse_from_rfc3339(&score.date).map_err(|e| e.to_string())?;
    if replay.mode as u8 != score.mode
        || (replay.timestamp.timestamp_millis() - date.timestamp_millis()).abs() > 1
        || (score.map_hash.len() == 32 && replay.beatmap_hash.as_deref() != Some(&score.map_hash))
        || (score.source == "stable"
            && (replay.player_name.as_deref() != Some(&score.player)
                || replay.score != score.raw_score))
        || replay
            .raw_replay_data
            .as_ref()
            .is_none_or(|data| data.len() < 13)
    {
        return Err("Replay is incomplete or does not match this score".into());
    }
    Ok(bytes)
}
pub fn filename(score: &Score) -> String {
    let name = format!(
        "{} - {} - {} [{}] ({})",
        score.player,
        score.artist,
        score.title,
        score.difficulty,
        score.date.get(..10).unwrap_or("unknown-date")
    );
    let clean: String = name
        .chars()
        .filter(|ch| !ch.is_control() && !"<>:\"/\\|?*".contains(*ch))
        .take(160)
        .collect();
    format!("{}.osr", clean.trim().trim_end_matches('.'))
}
pub fn availability(
    path: &Path,
    ids: &[String],
    warning: Option<String>,
) -> Result<Availability, String> {
    if ids.len() > 100 {
        return Err("Check at most 100 scores at a time".into());
    }
    let c = library::db(path)?;
    table(&c)?;
    let mut available = Vec::new();
    for id in ids {
        let rows = records(&c, id)?;
        if rows
            .iter()
            .any(|s| candidates(&c, s).iter().any(|f| verified(f, s).is_ok()))
        {
            available.push(ReplayInfo {
                id: id.clone(),
                filename: filename(&rows[0]),
            });
        }
    }
    Ok(Availability { available, warning })
}
pub fn export(path: &Path, id: &str, directory: &Path) -> Result<PathBuf, String> {
    let directory = directory
        .canonicalize()
        .map_err(|_| "Choose an existing replay export folder".to_string())?;
    if !directory.is_dir() {
        return Err("Replay export destination must be a folder".into());
    }
    let c = library::db(path)?;
    table(&c)?;
    for source in crate::sources::list(&c)? {
        if let Ok(base) = PathBuf::from(&source.root)
            .join(if source.kind == "stable" {
                "Data"
            } else {
                "files"
            })
            .canonicalize()
            && directory.starts_with(base)
        {
            return Err("Choose a folder outside the game's internal data storage".into());
        }
    }
    let rows = records(&c, id)?;
    let mut found = None;
    for score in &rows {
        for file in candidates(&c, score) {
            if let Ok(bytes) = verified(&file, score) {
                found = Some(bytes);
                break;
            }
        }
        if found.is_some() {
            break;
        }
    }
    let bytes = found.ok_or(
        "A matching local replay is no longer available. Check your game folders and rescan.",
    )?;
    let name = filename(rows.first().ok_or("Unknown score")?);
    let stem = name.trim_end_matches(".osr");
    for index in 0..10_000 {
        let destination = directory.join(if index == 0 {
            name.clone()
        } else {
            format!("{stem} ({index}).osr")
        });
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)
        {
            Ok(mut output) => {
                if let Err(e) = output.write_all(&bytes).and_then(|_| output.sync_all()) {
                    drop(output);
                    let _ = fs::remove_file(&destination);
                    return Err(e.to_string());
                }
                return Ok(destination);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("Too many replay files with the same name in this folder".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn score(source: &str) -> Score {
        serde_json::from_value(serde_json::json!({"id":format!("{source}:fixture"),"mapHash":"0123456789abcdef0123456789abcdef","player":"Fixture","date":"2017-01-01T00:00:00Z","mode":0,"title":"Title?","artist":"Artist","difficulty":"Test","mapper":"Test","mods":0,"n300":99,"n100":1,"n50":0,"misses":0,"combo":100,"maxCombo":100,"rawScore":12345,"accuracy":99.0,"fc":true,"ss":false,"stars":null,"pp":null,"fcPp":null,"source":source,"status":"Fixture","hasArtwork":false})).unwrap()
    }
    fn replay_bytes(score: &Score, version: u32) -> Vec<u8> {
        let replay = osu_db::Replay {
            mode: osu_db::Mode::Standard,
            version,
            beatmap_hash: Some(score.map_hash.clone()),
            player_name: Some(score.player.clone()),
            replay_hash: None,
            count_300: 99,
            count_100: 1,
            count_50: 0,
            count_geki: 0,
            count_katsu: 0,
            count_miss: 0,
            score: 12345,
            max_combo: 100,
            perfect_combo: true,
            mods: osu_db::ModSet::from_bits(0),
            life_graph: None,
            timestamp: chrono::DateTime::parse_from_rfc3339(&score.date)
                .unwrap()
                .to_utc(),
            replay_data: None,
            raw_replay_data: Some(vec![
                93, 0, 0, 128, 0, 255, 255, 255, 255, 255, 255, 255, 255, 0, 24, 31, 2, 67, 81, 3,
                180, 14, 134, 195, 201, 57, 249, 30, 119, 32, 159, 24, 94, 214, 198, 32, 134, 3,
                161, 140, 121, 56, 162, 255, 238, 252, 0, 0,
            ]),
            online_score_id: 1,
        };
        let mut bytes = Vec::new();
        replay.to_writer(&mut bytes, None).unwrap();
        if version >= 30_000_001 {
            bytes.extend_from_slice(b"preserve-lazer-extension-exactly");
        }
        bytes
    }
    #[test]
    fn exports_both_clients_preserves_extensions_and_handles_merged_missing_copies() {
        let root = std::env::temp_dir().join(format!("sx-replay-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let stable = root.join("stable");
        let lazer = root.join("lazer");
        let output = root.join("exports");
        fs::create_dir_all(stable.join("Data/r")).unwrap();
        fs::create_dir_all(lazer.join("files/a")).unwrap();
        fs::create_dir_all(&output).unwrap();
        let path = root.join("library.sqlite");
        let a = score("stable");
        let b = score("lazer");
        let stable_bytes = replay_bytes(&a, 20_170_101);
        let lazer_bytes = replay_bytes(&b, 30_000_019);
        let stable_file = stable.join("Data/r").join(stable_filename(&a).unwrap());
        let lazer_file = lazer.join("files/a/fixture");
        fs::write(&stable_file, &stable_bytes).unwrap();
        fs::write(&lazer_file, &lazer_bytes).unwrap();
        {
            let c = library::db(&path).unwrap();
            table(&c).unwrap();
            for s in [&a, &b] {
                c.execute(
                    "INSERT INTO scores VALUES(?1,?2,NULL)",
                    params![s.id, serde_json::to_string(s).unwrap()],
                )
                .unwrap();
            }
            for (key, value) in [("stable_root", &stable), ("lazer_root", &lazer)] {
                c.execute(
                    "INSERT INTO metadata VALUES(?1,?2)",
                    params![key, value.to_string_lossy()],
                )
                .unwrap();
            }
            c.execute(
                "INSERT INTO replay_files VALUES(?1,?2)",
                params![b.id, lazer_file.to_string_lossy()],
            )
            .unwrap();
            c.execute("INSERT INTO score_links VALUES(?1,?2)", params![b.id, a.id])
                .unwrap();
        }
        assert_eq!(
            availability(&path, &[a.id.clone(), b.id.clone()], None)
                .unwrap()
                .available
                .len(),
            2
        );
        let first = export(&path, &a.id, &output).unwrap();
        assert_eq!(fs::read(&first).unwrap(), stable_bytes);
        let second = export(&path, &a.id, &output).unwrap();
        assert_ne!(first, second);
        assert_eq!(fs::read(&first).unwrap(), stable_bytes);
        assert!(export(&path, &a.id, &stable.join("Data/r")).is_err());
        let other_stable = root.join("other-stable");
        fs::create_dir_all(other_stable.join("Data/r")).unwrap();
        let other_file = other_stable
            .join("Data/r")
            .join(stable_filename(&a).unwrap());
        fs::write(&other_file, &stable_bytes).unwrap();
        {
            let c = library::db(&path).unwrap();
            crate::sources::remember(&c, "stable", &other_stable, None).unwrap();
        }
        // Merged cards can still export from lazer after the stable copy disappears.
        fs::remove_file(&stable_file).unwrap();
        assert_eq!(
            fs::read(export(&path, &a.id, &output).unwrap()).unwrap(),
            stable_bytes,
            "another stable install remains usable"
        );
        fs::remove_file(other_file).unwrap();
        let fallback = export(&path, &a.id, &output).unwrap();
        assert_eq!(fs::read(fallback).unwrap(), lazer_bytes);
        assert_eq!(fs::read(&lazer_file).unwrap(), lazer_bytes);
        let other_lazer = root.join("other-lazer");
        fs::create_dir_all(other_lazer.join("files")).unwrap();
        let other_replay = other_lazer.join("files/copy");
        fs::write(&other_replay, &lazer_bytes).unwrap();
        {
            let c = library::db(&path).unwrap();
            crate::sources::remember(&c, "lazer", &other_lazer, None).unwrap();
            c.execute(
                "INSERT INTO replay_locations VALUES(?1,?2,?3)",
                params![
                    b.id,
                    crate::sources::key("lazer", &other_lazer),
                    other_replay.to_string_lossy()
                ],
            )
            .unwrap();
        }
        fs::remove_file(&lazer_file).unwrap();
        assert_eq!(
            fs::read(export(&path, &a.id, &output).unwrap()).unwrap(),
            lazer_bytes,
            "another lazer install remains usable"
        );
        fs::remove_file(other_replay).unwrap();
        // Wrong-score files and references outside the source storage are rejected.
        let mut wrong = b.clone();
        wrong.map_hash = "ffffffffffffffffffffffffffffffff".into();
        fs::write(&lazer_file, replay_bytes(&wrong, 30_000_019)).unwrap();
        assert!(
            availability(&path, std::slice::from_ref(&a.id), None)
                .unwrap()
                .available
                .is_empty()
        );
        assert!(export(&path, &a.id, &output).is_err());
        assert!(within(&lazer.join("files"), &first).is_none());
        fs::remove_file(lazer_file).unwrap();
        assert!(
            availability(&path, std::slice::from_ref(&b.id), None)
                .unwrap()
                .available
                .is_empty()
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn stable_filetime_and_export_names_are_safe() {
        let mut s = score("stable");
        assert_eq!(
            stable_filename(&s).unwrap(),
            "0123456789abcdef0123456789abcdef-131277024000000000.osr"
        );
        s.player = "../illegal:player".into();
        let name = filename(&s);
        assert!(!name.contains('/'));
        assert!(!name.contains(':'));
        assert!(!name.contains('?'));
        s.map_hash = "../invalid".into();
        assert!(stable_filename(&s).is_none());
    }
}
