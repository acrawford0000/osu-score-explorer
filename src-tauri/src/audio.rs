use crate::sources;
use rusqlite::{Connection, OpenFlags, params};
use std::path::{Path, PathBuf};

fn local_file(base: &Path, file: &Path) -> Option<PathBuf> {
    let base = base.canonicalize().ok()?;
    let file = file.canonicalize().ok()?;
    (file.starts_with(base) && file.is_file()).then_some(file)
}
pub fn track(path: &Path, id: &str) -> Result<Option<PathBuf>, String> {
    let c = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| e.to_string())?;
    c.busy_timeout(std::time::Duration::from_secs(2))
        .map_err(|e| e.to_string())?;
    let hash: String = c
        .query_row(
            "SELECT json_extract(data,'$.mapHash') FROM scores WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .map_err(|_| "This score is no longer in the library".to_string())?;
    for map in crate::maps::cached(&c, &hash.to_ascii_lowercase())? {
        if let Some(audio) = map.audio
            && let Some(file) = crate::maps::safe_file(Path::new(&map.root), Path::new(&audio))
        {
            return Ok(Some(file));
        }
    }
    let indexed: bool = c
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM beatmap_files WHERE present=1 LIMIT 1)",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    for source in sources::list(&c)? {
        if source.kind == "stable" {
            if indexed {
                continue;
            }
            let root = PathBuf::from(&source.root);
            let songs = source
                .songs
                .as_ref()
                .map(PathBuf::from)
                .unwrap_or_else(|| root.join("Songs"));
            let Ok(list) = osu_db::Listing::from_file(root.join("osu!.db")) else {
                continue;
            };
            for map in list
                .beatmaps
                .iter()
                .filter(|m| m.hash.as_deref() == Some(&hash))
            {
                if let (Some(folder), Some(audio)) = (&map.folder_name, &map.audio)
                    && let Some(file) = local_file(&songs, &songs.join(folder).join(audio))
                {
                    return Ok(Some(file));
                }
            }
        } else if let Ok(mut q) =
            c.prepare("SELECT path FROM audio_files WHERE hash=?1 AND source_key=?2")
        {
            let install = sources::key("lazer", Path::new(&source.root));
            if let Ok(rows) = q.query_map(params![hash, install], |r| r.get::<_, String>(0)) {
                for file in rows.flatten() {
                    if let Some(file) =
                        local_file(&PathBuf::from(&source.root).join("files"), Path::new(&file))
                    {
                        return Ok(Some(file));
                    }
                }
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library;
    #[test]
    fn resolves_audio_from_the_second_install_and_rejects_escaped_paths() {
        let root = std::env::temp_dir().join(format!("sx-audio-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let first = root.join("first");
        let second = root.join("second");
        std::fs::create_dir_all(first.join("files")).unwrap();
        std::fs::create_dir_all(second.join("files")).unwrap();
        let file = second.join("files/audio");
        std::fs::write(&file, b"fixture audio").unwrap();
        let path = root.join("cache.sqlite");
        let c = library::db(&path).unwrap();
        sources::save(
            &c,
            vec![
                sources::GameSource {
                    kind: "lazer".into(),
                    root: first.to_string_lossy().into(),
                    songs: None,
                },
                sources::GameSource {
                    kind: "lazer".into(),
                    root: second.to_string_lossy().into(),
                    songs: None,
                },
            ],
        )
        .unwrap();
        c.execute_batch("CREATE TABLE audio_files(hash TEXT,source_key TEXT,path TEXT)")
            .unwrap();
        c.execute(
            "INSERT INTO scores VALUES('fixture','{\"mapHash\":\"map\"}',NULL)",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO audio_files VALUES('map',?1,?2)",
            params![sources::key("lazer", &second), file.to_string_lossy()],
        )
        .unwrap();
        assert_eq!(
            track(&path, "fixture").unwrap(),
            Some(file.canonicalize().unwrap())
        );
        let outside = root.join("outside");
        std::fs::write(&outside, b"private file").unwrap();
        c.execute(
            "UPDATE audio_files SET path=?1",
            [outside.to_string_lossy()],
        )
        .unwrap();
        assert!(track(&path, "fixture").unwrap().is_none());
        assert!(track(&path, "missing").is_err());
        drop(c);
        std::fs::remove_dir_all(root).unwrap();
    }
}
