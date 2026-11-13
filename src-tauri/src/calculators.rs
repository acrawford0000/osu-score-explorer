//! Versioned 2026 lazer rules, translated to Rust by rosu-pp-gemini.
//! Keep the original rosu-pp score fields and cache independently.
use crate::library::Score;
#[cfg(test)]
#[path = "calculators_tests.rs"]
mod tests;
use rusqlite::{Connection, params};
use serde::de::DeserializeSeed;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

pub const ENGINE: &str = "lazer-2026-rust-ba174151-policy1";
pub fn schema(c: &Connection) -> Result<(), String> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS calculator_results(score_id TEXT NOT NULL,engine TEXT NOT NULL,input_key TEXT NOT NULL,data TEXT NOT NULL,PRIMARY KEY(score_id,engine));").map_err(|e|e.to_string())
}
pub fn selected(c: &Connection) -> String {
    crate::sources::value(c, "calculator")
        .filter(|v| v == "lazer" || v == "rosu")
        .unwrap_or_else(|| "rosu".into())
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Cached {
    result: Option<ResultData>,
    status: String,
}
fn input(s: &Score, extra: Option<&str>) -> String {
    format!(
        "{:x}",
        md5::compute(
            serde_json::json!([
                s.map_hash,
                s.mode,
                s.mods,
                s.mods_json,
                s.gameplay_lazer,
                s.raw_score,
                s.combo,
                s.n300,
                s.n100,
                s.n50,
                s.n_geki,
                s.n_katu,
                s.misses,
                extra
            ])
            .to_string()
        )
    )
}
fn extra(c: &Connection, s: &Score) -> Option<String> {
    crate::sources::value(c, &format!("lazer-input:{}", s.id))
}
pub(crate) fn apply(s: &mut Score, cached: Option<&Cached>) {
    s.calculation_version = ENGINE.into();
    if let Some(result) = cached.and_then(|v| v.result.as_ref()) {
        s.pp = Some(result.pp);
        s.fc_pp = Some(result.fc_pp);
        s.stars = Some(result.stars);
        s.max_combo = Some(result.max_combo);
        s.fc = result.fc;
        s.status = "Ready".into();
    } else {
        s.pp = None;
        s.fc_pp = None;
        s.stars = None;
        s.max_combo = None;
        s.fc = false;
        s.status = cached
            .map(|v| v.status.clone())
            .unwrap_or_else(|| "Lazer calculation pending; exact installed map required".into());
    }
}
pub fn project(c: &Connection, scores: &mut [Score]) -> Result<(), String> {
    if selected(c) != "lazer" {
        return Ok(());
    }
    let results: HashMap<String, (String, Cached)> = {
        let mut q = c
            .prepare("SELECT score_id,input_key,data FROM calculator_results WHERE engine=?1")
            .map_err(|e| e.to_string())?;
        q.query_map([ENGINE], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .filter_map(|row| row.ok())
        .filter_map(|(id, key, data)| serde_json::from_str(&data).ok().map(|v| (id, (key, v))))
        .collect()
    };
    let inputs: HashMap<String, String> = {
        let mut q = c
            .prepare("SELECT key,data FROM metadata WHERE key LIKE 'lazer-input:%'")
            .map_err(|e| e.to_string())?;
        q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?
    };
    for s in scores {
        let cached = results
            .get(&s.id)
            .filter(|(key, _)| {
                *key == input(
                    s,
                    inputs
                        .get(&format!("lazer-input:{}", s.id))
                        .map(String::as_str),
                )
            })
            .map(|(_, v)| v);
        apply(s, cached);
    }
    Ok(())
}
pub(crate) fn cache(c: &Connection, s: &Score, map: &mut Map) -> Result<Cached, String> {
    cache_input(c, s, map, extra(c, s).as_deref())
}
fn cache_input(
    c: &Connection,
    s: &Score,
    map: &mut Map,
    extra: Option<&str>,
) -> Result<Cached, String> {
    let key = input(s, extra);
    let value: Option<Value> = extra.and_then(|v| serde_json::from_str(v).ok());
    let computed = map.calculate(
        s,
        value.as_ref().and_then(|v| v.get(3)),
        value.as_ref().and_then(|v| v.get(4)),
    );
    let cached = match computed {
        Ok(result) => Cached {
            result: Some(result),
            status: "Ready".into(),
        },
        Err(status) => Cached {
            result: None,
            status,
        },
    };
    c.execute(
        "INSERT OR REPLACE INTO calculator_results VALUES(?1,?2,?3,?4)",
        params![s.id, ENGINE, key, serde_json::to_string(&cached).unwrap()],
    )
    .map_err(|e| e.to_string())?;
    Ok(cached)
}
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub running: bool,
    pub processed: usize,
    pub total: usize,
    pub calculated: usize,
    pub unavailable: usize,
    pub message: String,
    pub updates: Vec<Score>,
}
#[derive(Default)]
pub struct Controller {
    running: AtomicBool,
    cancel: AtomicBool,
    state: Mutex<Status>,
}
impl Controller {
    pub fn status(&self) -> Status {
        let mut state = self.state.lock().unwrap();
        state.running = self.running.load(Ordering::Acquire);
        let mut updates = std::mem::take(&mut state.updates);
        let remaining = updates.split_off(updates.len().min(1000));
        let mut result = state.clone();
        result.updates = updates;
        state.updates = remaining;
        result
    }
    pub fn stop(&self) {
        self.cancel.store(true, Ordering::Release);
    }
    pub fn start(self: &Arc<Self>, path: PathBuf) {
        self.cancel.store(false, Ordering::Release);
        if self
            .running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let ctl = self.clone();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ctl.run(&path)))
                .unwrap_or_else(|_| Err("Calculator worker stopped; retry calculation".into()));
            if let Err(e) = result {
                ctl.state.lock().unwrap().message = e;
            }
            ctl.running.store(false, Ordering::Release);
        });
    }
    fn run(&self, path: &Path) -> Result<(), String> {
        let mut c = crate::library::db(path)?;
        let inputs: HashMap<String, String> = {
            let mut q = c
                .prepare("SELECT key,data FROM metadata WHERE key LIKE 'lazer-input:%'")
                .map_err(|e| e.to_string())?;
            q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(|e| e.to_string())?
                .collect::<Result<_, _>>()
                .map_err(|e| e.to_string())?
        };
        let saved: HashMap<String, String> = {
            let mut q = c
                .prepare("SELECT score_id,input_key FROM calculator_results WHERE engine=?1")
                .map_err(|e| e.to_string())?;
            q.query_map([ENGINE], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(|e| e.to_string())?
                .collect::<Result<_, _>>()
                .map_err(|e| e.to_string())?
        };
        let mut groups: BTreeMap<String, Vec<Score>> = BTreeMap::new();
        {
            let mut q = c
                .prepare("SELECT data FROM scores")
                .map_err(|e| e.to_string())?;
            for row in q
                .query_map([], |r| r.get::<_, String>(0))
                .map_err(|e| e.to_string())?
            {
                let s: Score = serde_json::from_str(&row.map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
                if saved.get(&s.id).is_some_and(|key| {
                    *key == input(
                        &s,
                        inputs
                            .get(&format!("lazer-input:{}", s.id))
                            .map(String::as_str),
                    )
                }) {
                    continue;
                }
                groups.entry(s.map_hash.clone()).or_default().push(s);
            }
        }
        {
            let mut state = self.state.lock().unwrap();
            state.total = groups.values().map(Vec::len).sum();
            state.processed = 0;
            state.calculated = 0;
            state.unavailable = 0;
            state.message = "Calculating with lazer 2026 rules in the background…".into();
        }
        for (hash, mut scores) in groups {
            if self.cancel.load(Ordering::Acquire) {
                break;
            }
            let bytes = crate::maps::cached(&c, &hash.to_ascii_lowercase())?
                .iter()
                .find_map(|m| {
                    crate::maps::safe_file(Path::new(&m.root), Path::new(&m.path))
                        .and_then(|p| std::fs::read(p).ok())
                        .filter(|b| format!("{:x}", md5::compute(b)).eq_ignore_ascii_case(&hash))
                });
            if let Some(bytes) = bytes
                && let Ok(mut map) = Map::new(&bytes)
            {
                let tx = c.transaction().map_err(|e| e.to_string())?;
                for score in &mut scores {
                    let cached = cache_input(
                        &tx,
                        score,
                        &mut map,
                        inputs
                            .get(&format!("lazer-input:{}", score.id))
                            .map(String::as_str),
                    )?;
                    apply(score, Some(&cached));
                }
                tx.commit().map_err(|e| e.to_string())?;
                let mut state = self.state.lock().unwrap();
                state.processed += scores.len();
                state.calculated += scores.iter().filter(|s| s.pp.is_some()).count();
                state.unavailable += scores.iter().filter(|s| s.pp.is_none()).count();
                state.updates.extend(scores);
                continue;
            }
            let mut state = self.state.lock().unwrap();
            state.processed += scores.len();
            state.unavailable += scores.len();
        }
        let mut state = self.state.lock().unwrap();
        state.message = format!(
            "Lazer calculations finished · {} updated · {} need maps or supported score data",
            state.calculated, state.unavailable
        );
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultData {
    pub stars: f64,
    pub pp: f64,
    pub fc_pp: f64,
    pub max_combo: u32,
    pub fc: bool,
}
pub struct Map {
    map: lazer_pp::Beatmap,
    diffs: HashMap<String, lazer_pp::any::DifficultyAttributes>,
}
impl Map {
    pub fn new(bytes: &[u8]) -> Result<Self, String> {
        Ok(Self {
            map: lazer_pp::Beatmap::from_bytes(bytes).map_err(|e| e.to_string())?,
            diffs: HashMap::new(),
        })
    }
    pub fn calculate(
        &mut self,
        s: &Score,
        stats: Option<&Value>,
        maximum: Option<&Value>,
    ) -> Result<ResultData, String> {
        if s.mods & (128 | 2048 | 8192 | 536870912) != 0 {
            return Err("Automation or unsupported mods; PP unavailable".into());
        }
        let mode = match s.mode {
            0 => rosu_mods::GameMode::Osu,
            1 => rosu_mods::GameMode::Taiko,
            2 => rosu_mods::GameMode::Catch,
            3 => rosu_mods::GameMode::Mania,
            _ => return Err("Unsupported ruleset".into()),
        };
        let json = if s.mods_json.is_empty() {
            None
        } else {
            Some(s.mods_json.clone())
        };
        let mods = if let Some(json) = &json {
            rosu_mods::serde::GameModsSeed::Mode {
                mode,
                deny_unknown_fields: true,
            }
            .deserialize(&mut serde_json::Deserializer::from_str(json))
            .map_err(|e| e.to_string())?
        } else {
            rosu_mods::GameMods::from_intermode(
                &rosu_mods::GameModsIntermode::from_bits(s.mods),
                mode,
            )
        };
        let key = format!(
            "{}:{}:{}",
            s.mode,
            s.gameplay_lazer,
            serde_json::to_string(&mods).unwrap()
        );
        if !self.diffs.contains_key(&key) {
            let d = lazer_pp::Difficulty::new()
                .mods(mods.clone())
                .lazer(s.gameplay_lazer);
            use lazer_pp::any::DifficultyAttributes as A;
            let attrs = match s.mode {
                0 => d
                    .checked_calculate_for_mode::<lazer_pp::osu::Osu>(&self.map)
                    .map(A::Osu),
                1 => d
                    .checked_calculate_for_mode::<lazer_pp::taiko::Taiko>(&self.map)
                    .map(A::Taiko),
                2 => d
                    .checked_calculate_for_mode::<lazer_pp::catch::Catch>(&self.map)
                    .map(A::Catch),
                _ => d
                    .checked_calculate_for_mode::<lazer_pp::mania::Mania>(&self.map)
                    .map(A::Mania),
            }
            .map_err(|e| e.to_string())?;
            self.diffs.insert(key.clone(), attrs);
        }
        let attrs = self.diffs[&key].clone();
        use lazer_pp::any::DifficultyAttributes::*;
        let complete = match &attrs {
            Osu(a) => {
                a.n_circles + a.n_sliders + a.n_spinners == s.n300 + s.n100 + s.n50 + s.misses
            }
            Taiko(a) => a.max_combo == s.n300 + s.n100 + s.misses,
            Catch(a) => {
                a.n_fruits + a.n_droplets == s.n300 + s.n100 + s.misses
                    && a.n_tiny_droplets == s.n50 + s.n_katu
            }
            Mania(a) => a.n_objects == s.n300 + s.n100 + s.n50 + s.n_geki + s.n_katu + s.misses,
        };
        if !complete {
            return Err("Incomplete play or judgments do not match this map".into());
        }
        let stat = |values: Option<&Value>, key: &str| {
            values
                .and_then(|v| v.get(key))
                .and_then(Value::as_u64)
                .unwrap_or(0) as u32
        };
        if s.gameplay_lazer && stats.is_none() {
            return Err("Rescan lazer to recover complete slider statistics".into());
        }
        let perform = |score: &Score, fc: bool| -> Result<f64, String> {
            let mut p = attrs
                .clone()
                .performance()
                .mods(mods.clone())
                .lazer(score.gameplay_lazer)
                .combo(score.combo)
                .n300(score.n300)
                .n100(score.n100)
                .n50(score.n50)
                .n_geki(score.n_geki)
                .n_katu(score.n_katu)
                .misses(score.misses);
            if score.gameplay_lazer {
                let values = if fc { maximum.or(stats) } else { stats };
                p = p
                    .large_tick_hits(stat(values, "LargeTickHit"))
                    .small_tick_hits(stat(values, "SmallTickHit"))
                    .slider_end_hits(stat(values, "SliderTailHit"));
            } else if !fc {
                p = p.legacy_total_score(score.raw_score);
            }
            Ok(p.calculate().pp())
        };
        let pp = perform(s, false)?;
        let mut fc = s.clone();
        fc.combo = attrs.max_combo();
        fc.misses = 0;
        match &attrs {
            Catch(a) => {
                fc.n300 = a.n_fruits;
                fc.n100 = a.n_droplets;
            }
            Mania(_) => fc.n_geki += s.misses,
            _ => fc.n300 += s.misses,
        }
        let fc_pp = perform(&fc, true)?;
        if !pp.is_finite() || !fc_pp.is_finite() || !attrs.stars().is_finite() {
            return Err("Calculation produced invalid values".into());
        }
        Ok(ResultData {
            stars: attrs.stars(),
            pp,
            fc_pp,
            max_combo: attrs.max_combo(),
            fc: s.misses == 0 && s.combo == attrs.max_combo(),
        })
    }
}
