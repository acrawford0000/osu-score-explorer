//! Repair only the scores associated with newly installed, exact map revisions.
use crate::{
    library::{self, Score},
    maps,
};
use rusqlite::{Connection, params};
use std::{collections::HashMap, fs, path::Path};

pub fn installed(
    c: &mut Connection,
    root: &Path,
    files: &[std::path::PathBuf],
) -> Result<Vec<Score>, String> {
    let mut updates = Vec::new();
    let generation = crate::sources::value(c, "map_index_generation")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0)
        + 1;
    let tx = c.transaction().map_err(|e| e.to_string())?;
    let mut diffs = HashMap::new();
    for file in files {
        let Some(file) = maps::safe_file(root, file) else {
            continue;
        };
        let bytes = fs::read(&file).map_err(|e| e.to_string())?;
        let meta = maps::parse(root, &file, &bytes);
        tx.execute(
            "INSERT OR REPLACE INTO beatmap_files VALUES(?1,?2,?3,?4,?5,1)",
            params![
                meta.path,
                meta.root,
                format!("maps-1:{}:", library::stamp(&file)?),
                meta.hash,
                serde_json::to_string(&meta).unwrap()
            ],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT OR REPLACE INTO beatmap_changes VALUES(?1,?2)",
            params![meta.hash, generation],
        )
        .map_err(|e| e.to_string())?;
        let records: Vec<String> = {
            let mut q = tx
                .prepare("SELECT data FROM scores WHERE lower(json_extract(data,'$.mapHash'))=?1")
                .map_err(|e| e.to_string())?;
            q.query_map([&meta.hash], |r| r.get(0))
                .map_err(|e| e.to_string())?
                .collect::<Result<_, _>>()
                .map_err(|e| e.to_string())?
        };
        let map = rosu_pp::Beatmap::from_bytes(&bytes).ok();
        let mut lazer_map = crate::calculators::Map::new(&bytes).ok();
        let using_lazer = crate::calculators::selected(&tx) == "lazer";
        let image = meta
            .background
            .as_ref()
            .and_then(|v| maps::safe_file(root, Path::new(v)))
            .map(|v| v.to_string_lossy().to_string());
        for json in records {
            let mut score: Score = serde_json::from_str(&json).map_err(|e| e.to_string())?;
            if !meta.title.is_empty() {
                score.title = meta.title.clone();
            }
            if !meta.artist.is_empty() {
                score.artist = meta.artist.clone();
            }
            score.difficulty = meta.difficulty.clone();
            score.mapper = meta.mapper.clone();
            score.has_artwork |= image.is_some();
            if score.pp.is_none() {
                if let Some(map) = &map {
                    if score.source == "lazer" {
                        crate::lazer::recover(&tx, &mut score, map)?;
                    } else if score.mods & (128 | 2048 | 8192 | 536870912) == 0 {
                        let key = format!("{}:{}:{}", meta.hash, score.mode, score.mods);
                        if let Some(attrs) = diffs
                            .entry(key)
                            .or_insert_with(|| library::difficulty(map, score.mode, score.mods))
                        {
                            score.stars = Some(attrs.stars());
                            score.max_combo = Some(attrs.max_combo());
                            if library::complete(attrs, &score) {
                                let pp = library::performance(attrs.clone(), &score);
                                let mut fc = score.clone();
                                fc.combo = attrs.max_combo();
                                fc.misses = 0;
                                match attrs {
                                    rosu_pp::any::DifficultyAttributes::Catch(a) => {
                                        fc.n300 = a.n_fruits;
                                        fc.n100 = a.n_droplets;
                                    }
                                    rosu_pp::any::DifficultyAttributes::Mania(_) => {
                                        fc.n_geki += score.misses
                                    }
                                    _ => fc.n300 += score.misses,
                                }
                                let fc_pp = library::performance(attrs.clone(), &fc);
                                if pp.is_finite() && fc_pp.is_finite() && attrs.stars().is_finite()
                                {
                                    score.pp = Some(pp);
                                    score.fc_pp = Some(fc_pp);
                                    score.calculation_version = library::ENGINE.into();
                                    score.fc =
                                        score.misses == 0 && score.combo == attrs.max_combo();
                                    score.status = "Ready".into();
                                    tx.execute("INSERT OR REPLACE INTO calculations VALUES(?1,?2)",params![format!("{}:{}",library::ENGINE,score.id),serde_json::json!({"stars":attrs.stars(),"pp":pp,"fc_pp":fc_pp,"max_combo":attrs.max_combo()}).to_string()]).map_err(|e|e.to_string())?;
                                }
                            } else {
                                score.status =
                                    "Incomplete play or judgment counts do not match this map"
                                        .into();
                            }
                        }
                    }
                }
            } else if score.status == "Cached calculation; local map missing" {
                score.status = "Ready".into();
            }
            if let Some(image) = &image {
                tx.execute(
                    "INSERT OR REPLACE INTO score_artwork VALUES(?1,?2,?3)",
                    params![score.id, format!("download:{}", root.display()), image],
                )
                .map_err(|e| e.to_string())?;
            }
            tx.execute(
                "UPDATE scores SET data=?2,artwork=COALESCE(?3,artwork) WHERE id=?1",
                params![score.id, serde_json::to_string(&score).unwrap(), image],
            )
            .map_err(|e| e.to_string())?;
            if let Some(map) = &mut lazer_map {
                let cached = crate::calculators::cache(&tx, &score, map)?;
                if using_lazer {
                    crate::calculators::apply(&mut score, Some(&cached));
                }
            } else if using_lazer {
                crate::calculators::apply(&mut score, None);
            }
            updates.push(score);
        }
    }
    tx.execute(
        "INSERT OR REPLACE INTO metadata VALUES('map_index_generation',?1)",
        [generation.to_string()],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(updates)
}
