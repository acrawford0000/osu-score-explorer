use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameSource {
    pub kind: String,
    pub root: String,
    pub songs: Option<String>,
}
pub fn key(kind: &str, root: &Path) -> String {
    let full = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let name = full
        .to_string_lossy()
        .replace("\\\\?\\", "")
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string();
    #[cfg(windows)]
    let name = name.to_lowercase();
    format!("{kind}-{:x}", md5::compute(name))
}
pub fn value(c: &Connection, key: &str) -> Option<String> {
    c.query_row("SELECT data FROM metadata WHERE key=?1", [key], |r| {
        r.get(0)
    })
    .optional()
    .ok()
    .flatten()
}
pub fn list(c: &Connection) -> Result<Vec<GameSource>, String> {
    if let Some(json) = value(c, "game_sources") {
        return serde_json::from_str(&json).map_err(|e| e.to_string());
    }
    let mut sources = Vec::new();
    for kind in ["stable", "lazer"] {
        if let Some(root) = value(c, &format!("{kind}_root")) {
            let songs = if kind == "stable" {
                value(c, "stable_configuration")
                    .and_then(|s| s.split_once('|').map(|(_, p)| p.to_string()))
            } else {
                None
            };
            sources.push(GameSource {
                kind: kind.into(),
                root,
                songs,
            });
        }
    }
    Ok(sources)
}
pub fn save(c: &Connection, sources: Vec<GameSource>) -> Result<Vec<GameSource>, String> {
    let mut normalized: Vec<GameSource> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for mut s in sources {
        s.root = s.root.trim().to_string();
        if !matches!(s.kind.as_str(), "stable" | "lazer") || !Path::new(&s.root).is_absolute() {
            return Err("Each game source needs a client type and an absolute folder path".into());
        }
        s.songs = s.songs.filter(|p| !p.trim().is_empty());
        if s.kind == "lazer" {
            s.songs = None;
        }
        if let Some(p) = &s.songs
            && !Path::new(p).is_absolute()
        {
            return Err("Songs folders must be absolute paths".into());
        }
        if seen.insert(key(&s.kind, Path::new(&s.root))) {
            normalized.push(s);
        }
    }
    c.execute(
        "INSERT OR REPLACE INTO metadata VALUES('game_sources',?1)",
        [serde_json::to_string(&normalized).map_err(|e| e.to_string())?],
    )
    .map_err(|e| e.to_string())?;
    Ok(normalized)
}
pub fn remember(
    c: &Connection,
    kind: &str,
    root: &Path,
    songs: Option<&Path>,
) -> Result<(), String> {
    let mut sources = list(c)?;
    let source = GameSource {
        kind: kind.into(),
        root: root.to_string_lossy().to_string(),
        songs: songs.map(|p| p.to_string_lossy().to_string()),
    };
    if let Some(old) = sources
        .iter_mut()
        .find(|s| key(&s.kind, Path::new(&s.root)) == key(kind, root))
    {
        *old = source;
    } else {
        sources.push(source);
    }
    save(c, sources)?;
    Ok(())
}
// Migrate the previous single-install fingerprints, then keep one set per install.
pub fn scope(c: &Connection, kind: &str, root: &Path) -> Result<String, String> {
    let id = key(kind, root);
    if value(c, &format!("{kind}_root")).is_some_and(|p| key(kind, Path::new(&p)) == id) {
        for field in ["stamp", "configuration"] {
            let old = format!("{kind}_{field}");
            if let Some(data) = value(c, &old) {
                c.execute(
                    "INSERT OR IGNORE INTO metadata VALUES(?1,?2)",
                    params![format!("{old}:{id}"), data],
                )
                .map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn migrates_old_folders_and_saves_multiple_distinct_installs() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE metadata(key TEXT PRIMARY KEY,data TEXT NOT NULL)")
            .unwrap();
        let a = std::env::temp_dir().join("fixture-stable");
        let b = std::env::temp_dir().join("fixture-lazer");
        let extra = std::env::temp_dir().join("fixture-stable-two");
        for (key, root) in [("stable_root", &a), ("lazer_root", &b)] {
            c.execute(
                "INSERT INTO metadata VALUES(?1,?2)",
                params![key, root.to_string_lossy()],
            )
            .unwrap();
        }
        c.execute(
            "INSERT INTO metadata VALUES('stable_stamp','old-fingerprint')",
            [],
        )
        .unwrap();
        let id = scope(&c, "stable", &a).unwrap();
        assert_eq!(
            value(&c, &format!("stable_stamp:{id}")).as_deref(),
            Some("old-fingerprint")
        );
        assert_eq!(list(&c).unwrap().len(), 2);
        remember(&c, "stable", &extra, None).unwrap();
        remember(&c, "stable", &extra.join(""), None).unwrap();
        assert_eq!(list(&c).unwrap().len(), 3);
        assert_ne!(key("stable", &a), key("stable", &extra));
        assert_ne!(key("stable", &a), key("lazer", &a));
        assert!(
            save(
                &c,
                vec![GameSource {
                    kind: "stable".into(),
                    root: "relative/path".into(),
                    songs: None
                }]
            )
            .is_err()
        );
        assert_eq!(
            list(&c).unwrap().len(),
            3,
            "invalid edits must not overwrite the saved list"
        );
        save(&c, vec![]).unwrap();
        assert!(
            list(&c).unwrap().is_empty(),
            "removed paths must not reappear from old settings"
        );
    }
}
