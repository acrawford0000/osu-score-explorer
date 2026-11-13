//! Persistent missing-map recovery with concurrent downloads and one installation writer.
use crate::{
    library::{self, Score},
    sources,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const MAX_ARCHIVE: u64 = 300 * 1024 * 1024;
#[cfg(test)]
use std::{collections::HashMap, io::Write};
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Options {
    pub songs: String,
    pub player: Option<String>,
    #[serde(default)]
    pub client_id: String,
    #[serde(default)]
    pub client_secret: String,
}
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub running: bool,
    pub phase: String,
    pub message: String,
    pub total: usize,
    pub completed: usize,
    pub unavailable: usize,
    pub pending: usize,
    pub bytes: u64,
    pub total_bytes: Option<u64>,
    pub installed_sets: usize,
    pub updates: Vec<Score>,
    pub issues: Vec<String>,
}
#[derive(Default)]
pub struct Controller {
    pub running: AtomicBool,
    pub pause: AtomicBool,
    state: Mutex<Status>,
}
impl Controller {
    pub fn restore(&self, path: &Path) {
        let Ok(c) = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        else {
            return;
        };
        if let Some(root) = sources::value(&c, "download_songs")
            && totals(&c, Path::new(&root), self, None).is_ok()
        {
            self.change(|s| {
                if s.pending > 0 {
                    s.phase = "paused".into();
                    s.message = "Saved download queue ready to resume.".into();
                }
            });
        }
    }
    pub fn status(&self) -> Status {
        let mut s = self.state.lock().unwrap();
        s.running = self.running.load(Ordering::Acquire);
        let mut updates = std::mem::take(&mut s.updates);
        let remaining = updates.split_off(updates.len().min(1000));
        let mut result = s.clone();
        result.updates = updates;
        s.updates = remaining;
        result
    }
    fn change(&self, f: impl FnOnce(&mut Status)) {
        f(&mut self.state.lock().unwrap());
    }
    pub fn pause(&self) {
        self.pause.store(true, Ordering::Release);
        self.change(|s| s.message = "Pausing the download queue…".into());
    }
}
pub fn schema(c: &Connection) -> Result<(), String> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS map_ids(hash TEXT PRIMARY KEY,beatmap_id INTEGER NOT NULL,set_id INTEGER NOT NULL,origin TEXT NOT NULL); CREATE INDEX IF NOT EXISTS map_ids_set ON map_ids(set_id); CREATE TABLE IF NOT EXISTS download_jobs(hash TEXT NOT NULL,target TEXT NOT NULL,state TEXT NOT NULL,error TEXT NOT NULL DEFAULT '',PRIMARY KEY(hash,target)); CREATE TABLE IF NOT EXISTS download_installs(target TEXT NOT NULL,set_id INTEGER NOT NULL,path TEXT NOT NULL,PRIMARY KEY(target,set_id));").map_err(|e|e.to_string())
}
fn valid_hash(h: &str) -> bool {
    h.len() == 32 && h.bytes().all(|b| b.is_ascii_hexdigit())
}
fn catalogs(c: &mut Connection) -> Result<(), String> {
    for source in sources::list(c)?.into_iter().filter(|s| s.kind == "stable") {
        let path = Path::new(&source.root).join("osu!.db");
        let Ok(stamp) = library::stamp(&path) else {
            continue;
        };
        let key = format!(
            "download-catalog:{}",
            sources::key("stable", Path::new(&source.root))
        );
        if sources::value(c, &key).as_deref() == Some(&stamp) {
            continue;
        }
        let Ok(list) = osu_db::Listing::from_file(&path) else {
            continue;
        };
        if library::stamp(&path).ok().as_deref() != Some(&stamp) {
            return Err(
                "A beatmap catalog changed while being read; retry after copying finishes".into(),
            );
        }
        let tx = c.transaction().map_err(|e| e.to_string())?;
        for m in list.beatmaps {
            if let Some(hash) = m.hash
                && valid_hash(&hash)
                && m.beatmap_id > 0
                && m.beatmapset_id > 0
            {
                tx.execute(
                    "INSERT OR REPLACE INTO map_ids VALUES(?1,?2,?3,?4)",
                    params![
                        hash.to_ascii_lowercase(),
                        m.beatmap_id,
                        m.beatmapset_id,
                        "catalog"
                    ],
                )
                .map_err(|e| e.to_string())?;
            }
        }
        tx.execute(
            "INSERT OR REPLACE INTO metadata VALUES(?1,?2)",
            params![key, stamp],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn target(path: &str) -> Result<PathBuf, String> {
    let p = Path::new(path);
    if !p.is_absolute() || !p.is_dir() {
        return Err("Choose an existing Songs folder before downloading beatmaps".into());
    }
    p.canonicalize().map_err(|e| e.to_string())
}
fn prepare(
    path: &Path,
    options: &Options,
) -> Result<(Connection, PathBuf, HashSet<String>), String> {
    let root = target(&options.songs)?;
    let mut c = library::db(path)?;
    catalogs(&mut c)?;
    let mut roots: Vec<String> = sources::value(&c, "download_roots")
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_default();
    let target_name = root.to_string_lossy().to_string();
    if !roots.contains(&target_name) {
        roots.push(target_name);
    }
    c.execute(
        "INSERT OR REPLACE INTO metadata VALUES('download_roots',?1)",
        [serde_json::to_string(&roots).unwrap()],
    )
    .map_err(|e| e.to_string())?;
    // A separate media library survives source changes without inventing a scores.db.
    c.execute(
        "INSERT OR REPLACE INTO metadata VALUES('download_songs',?1)",
        [root.to_string_lossy()],
    )
    .map_err(|e| e.to_string())?;
    let l = library::load(path)?;
    let mut hashes = HashSet::new();
    for s in l.scores {
        if options
            .player
            .as_ref()
            .filter(|v| !v.is_empty())
            .is_some_and(|p| !crate::players::matches(&s, p))
        {
            continue;
        }
        if valid_hash(&s.map_hash) {
            hashes.insert(s.map_hash.to_ascii_lowercase());
        }
    }
    let tx = c.transaction().map_err(|e| e.to_string())?;
    for hash in &hashes {
        let available = crate::maps::cached(&tx, hash)?
            .iter()
            .any(|m| crate::maps::safe_file(Path::new(&m.root), Path::new(&m.path)).is_some());
        if available {
            tx.execute(
                "DELETE FROM download_jobs WHERE hash=?1 AND target=?2",
                params![hash, root.to_string_lossy()],
            )
            .map_err(|e| e.to_string())?;
            continue;
        }
        tx.execute("INSERT INTO download_jobs(hash,target,state) VALUES(?1,?2,'pending') ON CONFLICT(hash,target) DO UPDATE SET state=CASE WHEN download_jobs.state IN ('needs_api','failed','working') THEN 'pending' ELSE download_jobs.state END,error=CASE WHEN download_jobs.state IN ('needs_api','failed','working') THEN '' ELSE download_jobs.error END",params![hash,root.to_string_lossy()]).map_err(|e|e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok((c, root, hashes))
}
fn totals(
    c: &Connection,
    root: &Path,
    ctl: &Controller,
    selected: Option<&HashSet<String>>,
) -> Result<(), String> {
    let mut total = 0;
    let mut pending = 0;
    let mut completed = 0;
    let mut unavailable = 0;
    let mut q = c
        .prepare("SELECT hash,state FROM download_jobs WHERE target=?1")
        .map_err(|e| e.to_string())?;
    for row in q
        .query_map([root.to_string_lossy()], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(|e| e.to_string())?
    {
        let (hash, state) = row.map_err(|e| e.to_string())?;
        if selected.is_some_and(|hashes| !hashes.contains(&hash)) {
            continue;
        }
        let n = 1;
        total += n;
        match state.as_str() {
            "pending" | "working" => pending += n,
            "installed" => completed += n,
            _ => unavailable += n,
        }
    }
    ctl.change(|s| {
        s.total = total;
        s.pending = pending;
        s.completed = completed;
        s.unavailable = unavailable;
    });
    Ok(())
}
fn job(c: &Connection, root: &Path, hash: &str, state: &str, error: &str) -> Result<(), String> {
    c.execute(
        "UPDATE download_jobs SET state=?3,error=?4 WHERE hash=?1 AND target=?2",
        params![hash, root.to_string_lossy(), state, error],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
fn wait(ctl: &Controller, d: Duration) -> Result<(), String> {
    let until = Instant::now() + d;
    while Instant::now() < until {
        if ctl.pause.load(Ordering::Acquire) {
            return Err("Paused".into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}
struct Http {
    client: reqwest::blocking::Client,
    client_id: String,
    secret: String,
    token: Option<String>,
    last_api: Option<Instant>,
    oauth: String,
    api: String,
    #[cfg(test)]
    mirror: String,
}
impl Http {
    fn new(o: &Options) -> Result<Self, String> {
        Ok(Self {
            client: reqwest::blocking::Client::builder()
                .user_agent(concat!("osu-score-explorer/", env!("CARGO_PKG_VERSION")))
                .connect_timeout(Duration::from_secs(15))
                .timeout(Duration::from_secs(180))
                .https_only(true)
                .build()
                .map_err(|_| "Could not initialize HTTPS downloads")?,
            client_id: o.client_id.clone(),
            secret: o.client_secret.clone(),
            token: None,
            last_api: None,
            oauth: "https://osu.ppy.sh/oauth/token".into(),
            api: "https://osu.ppy.sh/api/v2/beatmaps/lookup".into(),
            #[cfg(test)]
            mirror: "https://mirror.nekoha.moe/api/download".into(),
        })
    }
    fn response(
        &self,
        make: impl Fn() -> reqwest::blocking::RequestBuilder,
        ctl: &Controller,
    ) -> Result<reqwest::blocking::Response, String> {
        for attempt in 0..3 {
            if ctl.pause.load(Ordering::Acquire) {
                return Err("Paused".into());
            }
            match make().send() {
                Ok(r) if r.status().is_success() || r.status().as_u16() == 404 => return Ok(r),
                Ok(r) if r.status().as_u16() == 429 || r.status().is_server_error() => {
                    let delay = r
                        .headers()
                        .get("retry-after")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.parse::<u64>().ok())
                        .unwrap_or(2u64.pow(attempt + 1))
                        .min(300);
                    if attempt == 2 {
                        return Err(format!(
                            "Service temporarily unavailable (HTTP {}). Resume to retry.",
                            r.status().as_u16()
                        ));
                    }
                    ctl.change(|s| s.message = format!("Service is busy; retrying in {delay}s…"));
                    wait(ctl, Duration::from_secs(delay))?;
                }
                Ok(r) => return Err(format!("Request failed (HTTP {})", r.status().as_u16())),
                Err(_) => {
                    if attempt == 2 {
                        return Err("Network request failed or timed out. Resume to retry.".into());
                    }
                    wait(ctl, Duration::from_secs(2u64.pow(attempt + 1)))?;
                }
            }
        }
        Err("Network request failed".into())
    }
    fn resolve(&mut self, hash: &str, ctl: &Controller) -> Result<Option<(i64, i64)>, String> {
        if self.token.is_none() {
            let r = self.response(
                || {
                    self.client.post(&self.oauth).form(&[
                        ("client_id", self.client_id.as_str()),
                        ("client_secret", self.secret.as_str()),
                        ("grant_type", "client_credentials"),
                        ("scope", "public"),
                    ])
                },
                ctl,
            )?;
            let v: serde_json::Value = r
                .json()
                .map_err(|_| "osu! API authentication returned an invalid response")?;
            self.token = Some(
                v["access_token"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or("osu! API authentication failed. Check your client ID and secret.")?
                    .into(),
            );
        }
        if let Some(last) = self.last_api {
            let elapsed = last.elapsed();
            if elapsed < Duration::from_millis(1100) {
                wait(ctl, Duration::from_millis(1100) - elapsed)?;
            }
        }
        self.last_api = Some(Instant::now());
        let r = self.response(
            || {
                self.client
                    .get(&self.api)
                    .query(&[("checksum", hash)])
                    .bearer_auth(self.token.as_ref().unwrap())
            },
            ctl,
        )?;
        if r.status().as_u16() == 404 {
            return Ok(None);
        }
        let v: serde_json::Value = r
            .json()
            .map_err(|_| "osu! API returned an invalid beatmap response")?;
        if v["checksum"]
            .as_str()
            .is_none_or(|v| !v.eq_ignore_ascii_case(hash))
        {
            return Ok(None);
        }
        Ok(v["id"]
            .as_i64()
            .zip(v["beatmapset_id"].as_i64())
            .filter(|(a, b)| *a > 0 && *b > 0))
    }
    #[cfg(test)]
    fn download(&self, id: i64, path: &Path, ctl: &Controller) -> Result<(), String> {
        let url = format!("{}/{id}?noVideo=1", self.mirror);
        let mut r = self.response(|| self.client.get(&url), ctl)?;
        if r.status().as_u16() == 404 {
            return Err("This beatmapset is unavailable on the mirror".into());
        }
        let size = r.content_length();
        if size.is_some_and(|n| n > MAX_ARCHIVE) {
            return Err("Beatmap archive exceeds the 300 MiB download limit".into());
        }
        ctl.change(|s| {
            s.bytes = 0;
            s.total_bytes = size;
        });
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|e| e.to_string())?;
        let mut buffer = [0; 65536];
        let mut total = 0;
        loop {
            if ctl.pause.load(Ordering::Acquire) {
                return Err("Paused".into());
            }
            let n = r
                .read(&mut buffer)
                .map_err(|_| "Download interrupted. Resume to retry.")?;
            if n == 0 {
                break;
            }
            total += n as u64;
            if total > MAX_ARCHIVE {
                return Err("Beatmap archive exceeds the download limit".into());
            }
            f.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
            ctl.change(|s| s.bytes = total);
        }
        f.sync_all().map_err(|e| e.to_string())?;
        Ok(())
    }
}
fn safe_entry(name: &str) -> Result<PathBuf, String> {
    let normalized = name.replace('\\', "/");
    if normalized.starts_with('/') || normalized.split('/').any(|p| p == ".." || p.contains(':')) {
        return Err("Archive contains an unsafe path".into());
    }
    let p = PathBuf::from(normalized);
    if p.is_absolute()
        || p.components().any(|v| {
            matches!(
                v,
                std::path::Component::ParentDir
                    | std::path::Component::Prefix(_)
                    | std::path::Component::RootDir
            )
        })
    {
        return Err("Archive contains an unsafe path".into());
    }
    Ok(p)
}
fn video(path: &Path) -> bool {
    path.extension().and_then(|v| v.to_str()).is_some_and(|v| {
        matches!(
            v.to_ascii_lowercase().as_str(),
            "mp4" | "avi" | "mkv" | "webm" | "flv" | "wmv" | "m4v" | "mov" | "mpg" | "mpeg" | "3gp"
        )
    })
}
fn install(
    archive: &Path,
    root: &Path,
    set: i64,
    ctl: &Controller,
) -> Result<Vec<PathBuf>, String> {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let stage = root.join(format!(".score-explorer-{set}-{nonce}"));
    fs::create_dir(&stage).map_err(|e| e.to_string())?;
    let result = (|| {
        let mut zip = zip::ZipArchive::new(File::open(archive).map_err(|e| e.to_string())?)
            .map_err(|_| "The mirror did not return a valid .osz archive")?;
        if zip.len() > 10000 {
            return Err("Beatmap archive has too many files".into());
        }
        let mut expanded = 0u64;
        let mut maps = Vec::new();
        for i in 0..zip.len() {
            if ctl.pause.load(Ordering::Acquire) {
                return Err("Paused".into());
            }
            let mut file = zip.by_index(i).map_err(|e| e.to_string())?;
            let name = safe_entry(file.name())?;
            if file.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
                return Err("Archive contains a symbolic link".into());
            }
            if video(&name) {
                continue;
            }
            expanded = expanded
                .checked_add(file.size())
                .ok_or("Archive is too large")?;
            if file.size() > 200 * 1024 * 1024 || expanded > 1024 * 1024 * 1024 {
                return Err("Beatmap archive exceeds extraction limits".into());
            }
            let dest = stage.join(&name);
            if file.is_dir() {
                fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
                continue;
            }
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut out = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&dest)
                .map_err(|e| e.to_string())?;
            let actual = std::io::copy(&mut file.by_ref().take(200 * 1024 * 1024 + 1), &mut out)
                .map_err(|e| e.to_string())?;
            if actual != file.size() {
                return Err("An archive entry has an invalid size".into());
            }
            if name
                .extension()
                .is_some_and(|v| v.eq_ignore_ascii_case("osu"))
            {
                if file.size() > 20_000_000 {
                    return Err("A beatmap file exceeds the size limit".into());
                }
                maps.push(name);
            }
        }
        if maps.is_empty() {
            return Err("The archive contains no beatmap files".into());
        }
        // Keep existing folders and custom revisions intact.
        let mut dest = root.join(format!("{set} Score Explorer"));
        let mut suffix = 1;
        while dest.exists() {
            dest = root.join(format!("{set} Score Explorer ({suffix})"));
            suffix += 1;
        }
        fs::rename(&stage, &dest).map_err(|e| e.to_string())?;
        Ok(maps.into_iter().map(|p| dest.join(p)).collect())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&stage);
    }
    result
}
pub fn run(path: &Path, options: Options, ctl: Arc<Controller>) -> Result<(), String> {
    queue::run(path, options, ctl)
}
#[cfg(test)]
fn run_http(
    path: &Path,
    options: Options,
    ctl: Arc<Controller>,
    mut http: Http,
) -> Result<(), String> {
    ctl.change(|s| {
        s.running = true;
        s.phase = "preparing".into();
        s.message = "Finding missing maps and reading beatmap IDs…".into();
        s.issues.clear();
    });
    let (c, root, selected) = prepare(path, &options)?;
    let mut c = c;
    let jobs: Vec<String> = {
        let mut q = c
            .prepare(
                "SELECT hash FROM download_jobs WHERE target=?1 AND state='pending' ORDER BY hash",
            )
            .map_err(|e| e.to_string())?;
        q.query_map([root.to_string_lossy()], |r| r.get(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?
    };
    let scratch = path.parent().unwrap().join("beatmap-downloads");
    fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;
    let mut attempts: HashMap<i64, Result<Vec<PathBuf>, String>> = HashMap::new();
    for hash in jobs {
        if !selected.contains(&hash) {
            continue;
        }
        if ctl.pause.load(Ordering::Acquire) {
            break;
        }
        totals(&c, &root, &ctl, Some(&selected))?;
        let already = crate::maps::cached(&c, &hash)?
            .iter()
            .any(|m| crate::maps::safe_file(Path::new(&m.root), Path::new(&m.path)).is_some());
        if already {
            job(&c, &root, &hash, "installed", "")?;
            continue;
        }
        ctl.change(|s| {
            s.phase = "resolving".into();
            s.message = format!("Finding beatmap {hash}…");
            s.bytes = 0;
            s.total_bytes = None;
        });
        let cached: Option<(i64, i64)> = c
            .query_row(
                "SELECT beatmap_id,set_id FROM map_ids WHERE hash=?1",
                [&hash],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let ids = if let Some(ids) = cached {
            Some(ids)
        } else if options.client_id.trim().is_empty() || options.client_secret.trim().is_empty() {
            job(
                &c,
                &root,
                &hash,
                "needs_api",
                "No catalog ID found. Add osu! API credentials to resolve this hash.",
            )?;
            continue;
        } else {
            match http.resolve(&hash, &ctl) {
                Ok(Some(ids)) => {
                    c.execute(
                        "INSERT OR REPLACE INTO map_ids VALUES(?1,?2,?3,'api')",
                        params![hash, ids.0, ids.1],
                    )
                    .map_err(|e| e.to_string())?;
                    Some(ids)
                }
                Ok(None) => {
                    job(
                        &c,
                        &root,
                        &hash,
                        "not_found",
                        "osu! API could not identify this exact revision",
                    )?;
                    None
                }
                Err(e) => {
                    job(&c, &root, &hash, "failed", &e)?;
                    ctl.change(|s| s.issues.push(e));
                    break;
                }
            }
        };
        let Some((_, set)) = ids else { continue };
        if let std::collections::hash_map::Entry::Vacant(attempt) = attempts.entry(set) {
            ctl.change(|s| {
                s.phase = "downloading".into();
                s.message = format!("Downloading mapset {set} without video…");
            });
            let archive = scratch.join(format!("{set}.osz.part"));
            let _ = fs::remove_file(&archive);
            let existing: Option<String> = c
                .query_row(
                    "SELECT path FROM download_installs WHERE target=?1 AND set_id=?2",
                    params![root.to_string_lossy(), set],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            let saved = existing
                .and_then(|v| serde_json::from_str::<Vec<String>>(&v).ok())
                .map(|files| {
                    files
                        .into_iter()
                        .filter_map(|v| crate::maps::safe_file(&root, Path::new(&v)))
                        .collect::<Vec<_>>()
                })
                .filter(|files| !files.is_empty());
            let result = if let Some(files) = saved {
                Ok(files)
            } else {
                http.download(set, &archive, &ctl).and_then(|_| {
                    ctl.change(|s| {
                        s.phase = "installing".into();
                        s.message = format!("Installing mapset {set} and updating scores…");
                    });
                    install(&archive, &root, set, &ctl)
                })
            };
            let _ = fs::remove_file(&archive);
            let result = result.and_then(|files| {
                let updates = crate::recovery::installed(&mut c, &root, &files)?;
                if !files.is_empty() {
                    c.execute(
                        "INSERT OR REPLACE INTO download_installs VALUES(?1,?2,?3)",
                        params![
                            root.to_string_lossy(),
                            set,
                            serde_json::to_string(&files).unwrap()
                        ],
                    )
                    .map_err(|e| e.to_string())?;
                }
                ctl.change(|s| {
                    s.updates.extend(updates);
                    s.installed_sets += 1;
                });
                Ok(files)
            });
            attempt.insert(result);
            if ctl.pause.load(Ordering::Acquire) {
                break;
            }
            wait(&ctl, Duration::from_millis(1500)).ok();
        }
        match attempts.get(&set).unwrap() {
            Ok(_) => {
                let found = !crate::maps::cached(&c, &hash)?.is_empty();
                job(
                    &c,
                    &root,
                    &hash,
                    if found { "installed" } else { "mismatch" },
                    if found {
                        ""
                    } else {
                        "Downloaded set does not contain this historical map revision"
                    },
                )?;
            }
            Err(e) => {
                job(&c, &root, &hash, "failed", e)?;
                ctl.change(|s| {
                    if s.issues.len() < 8 {
                        s.issues.push(format!("Mapset {set}: {e}"));
                    }
                });
            }
        }
    }
    totals(&c, &root, &ctl, Some(&selected))?;
    ctl.change(|s| {
        s.running = false;
        s.phase = if ctl.pause.load(Ordering::Acquire) {
            "paused"
        } else {
            "finished"
        }
        .into();
        s.message = if s.phase == "paused" {
            "Download queue paused. Resume when ready.".into()
        } else {
            format!(
                "{} map revisions ready · {} unavailable or need API lookup",
                s.completed, s.unavailable
            )
        };
    });
    Ok(())
}
pub fn finish(ctl: &Controller, result: Result<(), String>) {
    if let Err(e) = result {
        ctl.change(|s| {
            s.running = false;
            s.phase = "error".into();
            s.message = e;
        });
    }
    ctl.running.store(false, Ordering::Release);
}

#[path = "download_queue.rs"]
mod queue;
#[cfg(test)]
#[path = "downloads_tests.rs"]
pub(crate) mod tests;
