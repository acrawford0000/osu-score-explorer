//! Account identity for filtering; original names and score records stay intact.
use crate::library::Score;
#[cfg(test)]
#[path = "players_tests.rs"]
mod tests;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Account {
    pub id: u64,
    pub name: String,
    pub aliases: Vec<String>,
}
fn norm(name: &str) -> String {
    name.trim().to_lowercase()
}
pub fn saved(c: &Connection) -> Vec<Account> {
    crate::sources::value(c, "player_accounts")
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_default()
}
pub fn resolve(c: &Connection, scores: &mut [Score]) -> Vec<Account> {
    let profiles = crate::player_profiles::cached(c);
    // Legacy lazer imports may have looked up a reused username and assigned
    // its current holder. A score cannot belong to an account created later.
    for score in scores.iter_mut() {
        if !score.gameplay_lazer
            && score.solo_online_id.is_none()
            && score
                .player_id
                .and_then(|id| profiles.get(&id))
                .is_some_and(|p| score.date < p.joined)
        {
            score.player_id = None;
        }
    }
    let mut accounts: BTreeMap<u64, Account> = BTreeMap::new();
    let mut dates: HashMap<u64, String> = HashMap::new();
    for score in scores.iter() {
        if let Some(id) = score.player_id.filter(|id| *id > 1) {
            let a = accounts.entry(id).or_insert_with(|| Account {
                id,
                name: score.player.clone(),
                aliases: vec![],
            });
            a.aliases.extend(score.player_aliases.iter().cloned());
            a.aliases.push(score.player.clone());
            if dates.get(&id).is_none_or(|date| *date < score.date) {
                a.name = score.player.clone();
                dates.insert(id, score.date.clone());
            }
        }
    }
    for profile in profiles.values() {
        let a = accounts.entry(profile.id).or_insert_with(|| Account {
            id: profile.id,
            name: profile.name.clone(),
            aliases: vec![],
        });
        a.aliases.extend(profile.aliases.iter().cloned());
        a.aliases.push(profile.name.clone());
        a.name = profile.name.clone();
    }
    for manual in saved(c) {
        let a = accounts.entry(manual.id).or_insert_with(|| manual.clone());
        a.aliases.extend(manual.aliases);
        a.aliases.push(manual.name.clone());
        a.name = manual.name;
    }
    let mut owners: HashMap<String, HashSet<u64>> = HashMap::new();
    for a in accounts.values_mut() {
        a.aliases.push(a.name.clone());
        a.aliases.sort_by_key(|s| norm(s));
        a.aliases.dedup_by(|a, b| norm(a) == norm(b));
        a.aliases.retain(|s| !s.trim().is_empty());
        for name in &a.aliases {
            owners.entry(norm(name)).or_default().insert(a.id);
        }
    }
    for score in scores {
        if score.player_id.is_none()
            && let Some(owners) = owners.get(&norm(&score.player))
        {
            let ids: Vec<_> = owners
                .iter()
                .filter(|id| profiles.get(id).is_none_or(|p| score.date >= p.joined))
                .copied()
                .collect();
            if ids.len() == 1 {
                score.player_id = ids.first().copied();
            }
        }
        score.player_aliases = score
            .player_id
            .and_then(|id| accounts.get(&id))
            .map(|a| a.aliases.clone())
            .unwrap_or_else(|| vec![score.player.clone()]);
    }
    accounts.into_values().collect()
}
pub fn save(c: &Connection, mut account: Account) -> Result<(), String> {
    if account.id <= 1 || account.name.trim().is_empty() || account.aliases.len() > 64 {
        return Err(
            "Enter an account ID greater than 1, a username, and up to 64 historical names".into(),
        );
    }
    account.name = account.name.trim().into();
    account.aliases.push(account.name.clone());
    account.aliases = account
        .aliases
        .into_iter()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .collect();
    if account.aliases.iter().any(|v| v.len() > 128) {
        return Err("A player name is too long".into());
    }
    let mut list = saved(c);
    list.retain(|a| a.id != account.id);
    list.push(account);
    c.execute(
        "INSERT OR REPLACE INTO metadata VALUES('player_accounts',?1)",
        [serde_json::to_string(&list).unwrap()],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
pub fn matches(score: &Score, filter: &str) -> bool {
    if let Some(name) = filter.strip_prefix("name:") {
        return score.player_id.is_none() && norm(&score.player) == norm(name);
    }
    if let Some(id) = filter
        .strip_prefix("id:")
        .and_then(|v| v.parse::<u64>().ok())
    {
        return score.player_id == Some(id);
    }
    norm(&score.player) == norm(filter)
        || score.player_aliases.iter().any(|v| norm(v) == norm(filter))
}
