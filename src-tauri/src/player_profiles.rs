//! Cached public profile names and account creation dates; never resolve an old name by its current holder alone.
use crate::{library, players};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io::Read,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Profile {
    pub id: u64,
    pub name: String,
    pub aliases: Vec<String>,
    pub joined: String,
    pub fetched: u64,
}
pub fn cached(c: &Connection) -> HashMap<u64, Profile> {
    let Ok(mut q) = c.prepare("SELECT data FROM metadata WHERE key LIKE 'player_profile:%'") else {
        return HashMap::new();
    };
    let Ok(rows) = q.query_map([], |r| r.get::<_, String>(0)) else {
        return HashMap::new();
    };
    rows.filter_map(Result::ok)
        .filter_map(|v| serde_json::from_str::<Profile>(&v).ok())
        .map(|p| (p.id, p))
        .collect()
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn parse(html: &str, id: u64) -> Result<Profile, String> {
    for tail in html.split("data-initial-data=\"").skip(1) {
        let Some(encoded) = tail.split('"').next() else {
            continue;
        };
        let decoded = encoded
            .replace("&quot;", "\"")
            .replace("&#039;", "'")
            .replace("&#39;", "'")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&");
        let Ok(data) = serde_json::from_str::<serde_json::Value>(&decoded) else {
            continue;
        };
        let user = data.get("user").unwrap_or(&data);
        if user["id"].as_u64() != Some(id) {
            continue;
        }
        let Some(name) = user["username"].as_str() else {
            continue;
        };
        let Some(joined) = user["join_date"]
            .as_str()
            .filter(|s| chrono::DateTime::parse_from_rfc3339(s).is_ok())
        else {
            continue;
        };
        let aliases = user["previous_usernames"]
            .as_array()
            .ok_or("Profile name history unavailable")?
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
        return Ok(Profile {
            id,
            name: name.into(),
            aliases,
            joined: joined.into(),
            fetched: now(),
        });
    }
    Err("Public profile name history could not be read; saved player matching is unchanged".into())
}
pub fn store(c: &Connection, profile: &Profile) -> Result<(), String> {
    c.execute(
        "INSERT OR REPLACE INTO metadata VALUES(?1,?2)",
        params![
            format!("player_profile:{}", profile.id),
            serde_json::to_string(profile).map_err(|e| e.to_string())?
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
fn fetch(c: &Connection, client: &reqwest::blocking::Client, id: u64) -> Result<Profile, String> {
    if let Some(p) = cached(c)
        .remove(&id)
        .filter(|p| now().saturating_sub(p.fetched) < 7 * 86400)
    {
        return Ok(p);
    }
    let attempt = format!("player_profile_attempt:{id}");
    if crate::sources::value(c, &attempt)
        .and_then(|s| s.parse::<u64>().ok())
        .is_some_and(|t| now().saturating_sub(t) < 6 * 3600)
    {
        return Err("Profile lookup is cooling down; cached names remain available".into());
    }
    c.execute(
        "INSERT OR REPLACE INTO metadata VALUES(?1,?2)",
        params![attempt, now().to_string()],
    )
    .map_err(|e| e.to_string())?;
    static LAST: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
    {
        let mut last = LAST.lock().map_err(|e| e.to_string())?;
        if let Some(previous) = *last {
            std::thread::sleep(Duration::from_millis(1100).saturating_sub(previous.elapsed()));
        }
        *last = Some(std::time::Instant::now());
    }
    let response = client
        .get(format!("https://osu.ppy.sh/users/{id}"))
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|e| format!("Profile lookup unavailable: {e}"))?;
    let mut bytes = Vec::new();
    response
        .take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err("Public profile response exceeded the size limit".into());
    }
    let profile = parse(&String::from_utf8(bytes).map_err(|e| e.to_string())?, id)?;
    store(c, &profile)?;
    Ok(profile)
}
#[derive(Serialize)]
pub struct Identity {
    pub accounts: Vec<players::Account>,
    pub assignments: Vec<(String, Option<u64>)>,
}
pub fn refresh(path: &Path, id: u64) -> Result<Identity, String> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = LOCK.lock().map_err(|e| e.to_string())?;
    if id <= 1 {
        return Err("Choose a known osu! account".into());
    }
    let initial = library::load(path)?;
    if !initial.player_accounts.iter().any(|a| a.id == id) {
        return Err("Choose an imported or saved account".into());
    }
    let c = library::db(path)?;
    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!("osu-score-explorer/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(12))
        .build()
        .map_err(|e| e.to_string())?;
    let target = fetch(&c, &client, id)?;
    // A former username may now belong to another account. Fetch only the few
    // conflicting known holders, not every opponent/replay author in the library.
    let mut names = target.aliases.clone();
    names.push(target.name);
    let mut conflicts: Vec<_> = initial
        .player_accounts
        .iter()
        .filter(|a| {
            a.id != id
                && a.aliases.iter().any(|v| {
                    names
                        .iter()
                        .any(|n| v.trim().eq_ignore_ascii_case(n.trim()))
                })
        })
        .map(|a| a.id)
        .collect();
    conflicts.sort_unstable();
    conflicts.dedup();
    for conflict in conflicts.into_iter().take(4) {
        let _ = fetch(&c, &client, conflict);
    }
    drop(c);
    let result = library::load(path)?;
    Ok(Identity {
        accounts: result.player_accounts,
        assignments: result
            .scores
            .into_iter()
            .map(|s| (s.id, s.player_id))
            .collect(),
    })
}
