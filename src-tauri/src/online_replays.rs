//! Public-scope replay lookup and verified .osr export. Credentials stay in memory.
use crate::{
    library::{self, Score},
    replays,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
const MAX_REPLAY: u64 = 64 * 1024 * 1024;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Credentials {
    pub client_id: String,
    pub client_secret: String,
}
struct Auth {
    id: String,
    secret: String,
    token: String,
    expires: Instant,
    client: reqwest::blocking::Client,
}
pub struct Service {
    auth: Mutex<Option<Auth>>,
    checks: Mutex<()>,
    api: String,
    oauth: String,
    cooldown: Mutex<Option<Instant>>,
}
impl Default for Service {
    fn default() -> Self {
        Self {
            auth: Mutex::new(None),
            checks: Mutex::new(()),
            api: "https://osu.ppy.sh/api/v2".into(),
            oauth: "https://osu.ppy.sh/oauth/token".into(),
            cooldown: Mutex::new(None),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub id: String,
    pub state: String,
    pub message: String,
    pub retry_after: Option<u64>,
}
impl Status {
    fn new(id: &str, state: &str, message: &str) -> Self {
        Self {
            id: id.into(),
            state: state.into(),
            message: message.into(),
            retry_after: None,
        }
    }
}
pub(crate) fn schema(c: &Connection) -> Result<(), String> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS online_replay_checks(key TEXT PRIMARY KEY,input TEXT NOT NULL,available INTEGER NOT NULL,checked INTEGER NOT NULL);CREATE TABLE IF NOT EXISTS online_replay_exports(id TEXT NOT NULL,path TEXT NOT NULL,PRIMARY KEY(id,path));").map_err(|e|e.to_string())
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
#[derive(Clone)]
struct Candidate {
    key: String,
    score: Score,
}
fn candidates(rows: &[Score]) -> Vec<Candidate> {
    let modes = ["osu", "taiko", "fruits", "mania"];
    let mut result = Vec::new();
    for s in rows {
        let valid = |v: &&String| v.parse::<u64>().is_ok_and(|n| n > 0 && n < i64::MAX as u64);
        if let Some(id) = s.solo_online_id.as_ref().filter(valid) {
            result.push(Candidate {
                key: format!("scores/{id}"),
                score: s.clone(),
            });
        }
        if let Some(id) = s.legacy_online_id.as_ref().filter(valid)
            && let Some(mode) = modes.get(s.mode as usize)
        {
            result.push(Candidate {
                key: format!("scores/{mode}/{id}"),
                score: s.clone(),
            });
        }
    }
    let mut seen = std::collections::HashSet::new();
    result.retain(|v| seen.insert(v.key.clone()));
    result
}
fn fingerprint(s: &Score) -> String {
    format!(
        "{:x}",
        md5::compute(
            serde_json::json!([
                s.map_hash, s.mode, s.date, s.n300, s.n100, s.n50, s.n_geki, s.n_katu, s.misses,
                s.combo
            ])
            .to_string()
        )
    )
}
fn cached(c: &Connection, v: &Candidate) -> Option<bool> {
    let row: (String, bool, i64) = c
        .query_row(
            "SELECT input,available,checked FROM online_replay_checks WHERE key=?1",
            [&v.key],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .ok()??;
    (row.0 == fingerprint(&v.score)
        && now().saturating_sub(row.2.max(0) as u64) < if row.1 { 86400 } else { 6 * 3600 })
    .then_some(row.1)
}
fn remember(c: &Connection, v: &Candidate, available: bool) -> Result<(), String> {
    c.execute(
        "INSERT OR REPLACE INTO online_replay_checks VALUES(?1,?2,?3,?4)",
        params![v.key, fingerprint(&v.score), available, now() as i64],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
fn limited(mut response: reqwest::blocking::Response, max: u64) -> Result<Vec<u8>, String> {
    if response.content_length().is_some_and(|n| n > max) {
        return Err("Server response exceeds the supported size limit".into());
    }
    let mut bytes = Vec::new();
    response
        .by_ref()
        .take(max + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Replay request failed while reading the response")?;
    if bytes.len() as u64 > max {
        return Err("Server response exceeds the supported size limit".into());
    }
    Ok(bytes)
}
impl Service {
    pub fn disconnect(&self) {
        *self.auth.lock().unwrap() = None;
    }
    pub fn connect(&self, credentials: Credentials) -> Result<(), String> {
        if credentials.client_id.parse::<u64>().is_err()
            || credentials.client_secret.trim().is_empty()
        {
            return Err("Enter your osu! API client ID and client secret".into());
        }
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(25))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("osu-score-explorer/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| e.to_string())?;
        let (token, expires) =
            self.authorize(&client, &credentials.client_id, &credentials.client_secret)?;
        *self.auth.lock().map_err(|e| e.to_string())? = Some(Auth {
            id: credentials.client_id,
            secret: credentials.client_secret,
            token,
            expires,
            client,
        });
        Ok(())
    }
    fn authorize(
        &self,
        client: &reqwest::blocking::Client,
        id: &str,
        secret: &str,
    ) -> Result<(String, Instant), String> {
        crate::osu_api::pace(&self.oauth, || false)?;
        let r = client
            .post(&self.oauth)
            .form(&[
                ("client_id", id),
                ("client_secret", secret),
                ("grant_type", "client_credentials"),
                ("scope", "public"),
            ])
            .send()
            .map_err(|_| "Could not connect to the osu! API")?;
        if !r.status().is_success() {
            return Err(
                "osu! API connection failed. Check your application client ID and secret.".into(),
            );
        }
        let value: serde_json::Value = serde_json::from_slice(&limited(r, 1024 * 1024)?)
            .map_err(|_| "Invalid osu! authorization response")?;
        let token = value["access_token"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("osu! did not return an access token")?
            .to_string();
        let seconds = value["expires_in"]
            .as_u64()
            .unwrap_or(3600)
            .clamp(1, 86400)
            .saturating_sub(30)
            .max(1);
        Ok((token, Instant::now() + Duration::from_secs(seconds)))
    }
    fn session(&self) -> Result<(reqwest::blocking::Client, String), String> {
        let mut auth = self.auth.lock().map_err(|e| e.to_string())?;
        let a = auth
            .as_mut()
            .ok_or("Connect the osu! API in Settings to check online replays")?;
        if a.expires <= Instant::now() {
            let (token, expires) = self.authorize(&a.client, &a.id, &a.secret)?;
            a.token = token;
            a.expires = expires;
        }
        Ok((a.client.clone(), a.token.clone()))
    }
    fn get(&self, key: &str, download: bool) -> Result<reqwest::blocking::Response, String> {
        if let Some(until) = *self.cooldown.lock().map_err(|e| e.to_string())?
            && until > Instant::now()
        {
            return Err("osu! API is cooling down after a rate limit; try again shortly".into());
        }
        let (client, token) = self.session()?;
        let url = format!(
            "{}/{}{}",
            self.api,
            key,
            if download { "/download" } else { "" }
        );
        crate::osu_api::pace(&url, || false)?;
        let r = client
            .get(&url)
            .timeout(Duration::from_secs(if download { 120 } else { 25 }))
            .bearer_auth(token)
            .header("x-api-version", "20220705")
            .header(
                "Accept",
                if download {
                    "application/x-osu-replay"
                } else {
                    "application/json"
                },
            )
            .send()
            .map_err(|_| "osu! replay request failed or timed out")?;
        if r.status().as_u16() == 429 {
            let wait = r
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(60)
                .clamp(1, 300);
            *self.cooldown.lock().unwrap() = Some(Instant::now() + Duration::from_secs(wait));
            if self.api.starts_with("https://osu.ppy.sh/") {
                crate::osu_api::cooldown(wait);
            }
        }
        if r.status().as_u16() == 401
            && let Some(a) = self.auth.lock().unwrap().as_mut()
        {
            a.expires = Instant::now();
        }
        Ok(r)
    }
    fn retry_after(&self) -> Option<u64> {
        self.cooldown
            .lock()
            .ok()
            .and_then(|v| *v)
            .filter(|t| *t > Instant::now())
            .map(|t| t.saturating_duration_since(Instant::now()).as_secs() + 1)
    }
    pub fn check(&self, path: &Path, id: &str) -> Result<Status, String> {
        let _guard = self.checks.lock().map_err(|e| e.to_string())?;
        let c = library::db(path)?;
        schema(&c)?;
        let rows = replays::records(&c, id)?;
        let choices = candidates(&rows);
        if choices.is_empty() {
            return Ok(Status::new(
                id,
                "no-id",
                "This score has no online score ID",
            ));
        }
        for choice in choices {
            if let Some(available) = cached(&c, &choice) {
                if available {
                    return Ok(Status::new(id, "available", "Online replay available"));
                }
                continue;
            }
            if let Some(wait) = self.retry_after() {
                let mut s = Status::new(
                    id,
                    "rate-limited",
                    "Online checks paused while osu! API cools down",
                );
                s.retry_after = Some(wait);
                return Ok(s);
            }
            let r = self.get(&choice.key, false)?;
            match r.status().as_u16() {
                404 => {
                    remember(&c, &choice, false)?;
                    continue;
                }
                429 => {
                    let mut s = Status::new(
                        id,
                        "rate-limited",
                        "Online checks paused by osu! API rate limit",
                    );
                    s.retry_after = self.retry_after();
                    return Ok(s);
                }
                401 | 403 => {
                    return Err(
                        "osu! denied this replay lookup. Reconnect the API in Settings.".into(),
                    );
                }
                n if !(200..300).contains(&n) => {
                    return Err(format!(
                        "osu! replay lookup unavailable (HTTP {n}); retry later"
                    ));
                }
                _ => {}
            }
            let value: serde_json::Value = serde_json::from_slice(&limited(r, 2 * 1024 * 1024)?)
                .map_err(|_| "osu! returned an invalid score response")?;
            if !metadata_matches(&value, &choice) {
                return Err("Online score details do not match the imported score".into());
            }
            let available = value["has_replay"]
                .as_bool()
                .ok_or("osu! did not report replay availability")?;
            remember(&c, &choice, available)?;
            if available {
                return Ok(Status::new(id, "available", "Online replay available"));
            }
        }
        Ok(Status::new(
            id,
            "unavailable",
            "No online replay is available for this score",
        ))
    }
    pub fn export(&self, path: &Path, id: &str, directory: &Path) -> Result<PathBuf, String> {
        let c = library::db(path)?;
        schema(&c)?;
        replays::destination(&c, directory)?;
        if let Ok(file) = replays::export(path, id, directory) {
            return Ok(file);
        }
        let rows = replays::records(&c, id)?;
        for choice in candidates(&rows) {
            if cached(&c, &choice) == Some(false) {
                continue;
            }
            let r = self.get(&choice.key, true)?;
            if r.status().as_u16() == 404 {
                remember(&c, &choice, false)?;
                continue;
            }
            if !r.status().is_success() {
                return Err(format!(
                    "Replay download unavailable (HTTP {}). Reconnect or retry later.",
                    r.status().as_u16()
                ));
            }
            let bytes = limited(r, MAX_REPLAY)?;
            verify(&bytes, &rows)?;
            let file = replays::save(path, id, directory, &bytes)?;
            c.execute(
                "INSERT OR REPLACE INTO online_replay_exports VALUES(?1,?2)",
                params![id, file.to_string_lossy()],
            )
            .map_err(|e| e.to_string())?;
            remember(&c, &choice, true)?;
            return Ok(file);
        }
        Err("The online replay is no longer available".into())
    }
}
fn metadata_matches(value: &serde_json::Value, candidate: &Candidate) -> bool {
    let s = &candidate.score;
    let id = if candidate.key.matches('/').count() == 1 {
        value["id"].as_u64()
    } else {
        value["legacy_score_id"].as_u64()
    };
    let expected = candidate
        .key
        .rsplit('/')
        .next()
        .and_then(|v| v.parse::<u64>().ok());
    id == expected
        && value["ruleset_id"].as_u64() == Some(u64::from(s.mode))
        && value["beatmap"]["checksum"]
            .as_str()
            .is_none_or(|hash| hash.eq_ignore_ascii_case(&s.map_hash))
        && value["ended_at"]
            .as_str()
            .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok())
            .zip(chrono::DateTime::parse_from_rfc3339(&s.date).ok())
            .is_some_and(|(a, b)| (a.timestamp_millis() - b.timestamp_millis()).abs() <= 1000)
}
fn verify(bytes: &[u8], rows: &[Score]) -> Result<(), String> {
    let replay = osu_db::Replay::from_bytes(bytes)
        .map_err(|_| "Downloaded file is not a readable osu! replay")?;
    if replay.raw_replay_data.as_ref().is_none_or(|v| v.len() < 13) {
        return Err("Downloaded replay contains no playable replay data".into());
    }
    if !rows.iter().any(|s| {
        replay.mode as u8 == s.mode
            && replay
                .beatmap_hash
                .as_ref()
                .is_some_and(|h| h.eq_ignore_ascii_case(&s.map_hash))
            && chrono::DateTime::parse_from_rfc3339(&s.date).is_ok_and(|d| {
                (replay.timestamp.timestamp_millis() - d.timestamp_millis()).abs() <= 1000
            })
            && u32::from(replay.max_combo) == s.combo
            && u32::from(replay.count_300) == s.n300
            && u32::from(replay.count_100) == s.n100
            && u32::from(replay.count_50) == s.n50
            && u32::from(replay.count_miss) == s.misses
            && u32::from(replay.count_geki) == s.n_geki
            && u32::from(replay.count_katsu) == s.n_katu
            && (s.gameplay_lazer || s.raw_score == replay.score)
    }) {
        return Err(
            "Downloaded replay does not match this score's map, date, combo, or judgments".into(),
        );
    }
    Ok(())
}
pub(crate) fn saved_bytes(
    c: &Connection,
    id: &str,
    rows: &[Score],
) -> Result<Option<Vec<u8>>, String> {
    schema(c)?;
    let mut q=c.prepare("SELECT path FROM online_replay_exports WHERE id=?1 OR id IN (SELECT alias_id FROM score_links WHERE canonical_id=?1)").map_err(|e|e.to_string())?;
    for file in q
        .query_map([id], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .flatten()
    {
        let p = Path::new(&file);
        if p.is_absolute()
            && std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.len() <= MAX_REPLAY)
            && let Ok(bytes) = std::fs::read(p)
            && verify(&bytes, rows).is_ok()
        {
            return Ok(Some(bytes));
        }
    }
    Ok(None)
}
#[cfg(test)]
#[path = "online_replays_tests.rs"]
mod tests;
