//! Concurrent mirror scheduling from osu-shelf's osu-downloader; one installation/DB writer.
use super::*;
use futures_util::StreamExt;
use osu_downloader::{
    ArchiveValidation, Downloader, Event, Mirror, MirrorKind, OnExists, SchedulerHandle,
};
use std::{collections::BTreeMap, sync::mpsc};

const POLICY: &str = "osu-downloader-0.22-mirrors-1";
pub(super) fn sources() -> Vec<Mirror> {
    let mut sources: Vec<_> = Mirror::builtins()
        .into_iter()
        .filter(|m| {
            !m.kind().requires_auth()
                && m.kind() != MirrorKind::Nzbasic
                && m.kind() != MirrorKind::Nekoha
        })
        .map(Mirror::no_video)
        .collect();
    sources.push(Mirror::custom("https://mirror.nekoha.moe/api/download/{id}?noVideo=1").unwrap());
    sources
}
fn downloader(mirrors: Vec<Mirror>, scheduler: SchedulerHandle) -> Result<Arc<Downloader>, String> {
    Downloader::builder()
        .mirrors(mirrors)
        .scheduler(scheduler)
        .concurrent_downloads(3)
        .archive_validation(ArchiveValidation::Eocd)
        .progress_timeout(Duration::from_secs(30))
        .network_retry_attempts(1)
        .on_exists(OnExists::Overwrite)
        .user_agent(concat!("osu-score-explorer/", env!("CARGO_PKG_VERSION")))
        .build()
        .map(Arc::new)
        .map_err(|e| e.to_string())
}
enum Finished {
    Complete(u32, String),
    Failed(u32, String),
}
fn batch(
    c: &mut Connection,
    root: &Path,
    scratch: &Path,
    sets: &BTreeMap<u32, Vec<String>>,
    selected: &HashSet<String>,
    ctl: &Arc<Controller>,
    downloader: Arc<Downloader>,
) -> Result<(), String> {
    if sets.is_empty() {
        return Ok(());
    }
    let ids: Vec<u32> = sets.keys().copied().collect();
    let dir = scratch.to_path_buf();
    let signal = ctl.clone();
    let (sender, receiver) = mpsc::sync_channel(8);
    let network = std::thread::spawn(move || -> Result<(), String> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        runtime.block_on(async move{
            let mut session=downloader.download_many(ids,&dir);let mut events=session.events().ok_or("No download event stream")?;
            loop {
                tokio::select!{
                    _=tokio::time::sleep(Duration::from_millis(100))=>{if signal.pause.load(Ordering::Acquire){session.cancel();}},
                    event=events.next()=>{
                        let Some(event)=event else {break};
                        match event {
                            Event::BeatmapsetCompleted{beatmapset_id,filename,..}=>{if sender.send(Finished::Complete(beatmapset_id,filename)).is_err(){session.cancel();break;}},
                            Event::BeatmapsetFailed{beatmapset_id,error,..}=>{let _=sender.send(Finished::Failed(beatmapset_id,error.to_string()));},
                            Event::BeatmapsetSkipped{beatmapset_id,reason}=>{let _=sender.send(Finished::Failed(beatmapset_id,format!("Download deferred or unavailable: {reason:?}")));},
                            Event::Progress{beatmapset_id,downloaded_bytes,total_bytes,..}=>{
                                signal.change(|s|{s.bytes=downloaded_bytes;s.total_bytes=total_bytes;});
                                if downloaded_bytes>MAX_ARCHIVE || total_bytes.is_some_and(|n|n>MAX_ARCHIVE){let _=sender.send(Finished::Failed(beatmapset_id,"Archive exceeds the download size limit".into()));session.cancel();}
                            },
                            Event::BeatmapsetStatus{beatmapset_id,status}=>signal.change(|s|{s.phase="downloading".into();s.message=match status {
                                osu_downloader::Status::Contacting{mirror}|osu_downloader::Status::Downloading{mirror}=>format!("Mapset {beatmapset_id} · downloading without video via {}",mirror.host),
                                osu_downloader::Status::Verifying{..}=>format!("Mapset {beatmapset_id} · checking downloaded archive"),
                                osu_downloader::Status::RateLimited{cooldown,..}=>format!("Mirrors are busy; automatically waiting {}s for an available source",cooldown.as_secs()),
                                osu_downloader::Status::RetryingTransient{mirror,..}=>format!("Mapset {beatmapset_id} · retrying {} or switching sources",mirror.host),
                            };}),
                            Event::BeatmapsetDeferred{beatmapset_id,retry_in,..}=>signal.change(|s|s.message=format!("Mirrors busy for mapset {beatmapset_id}; automatically retrying in {}s",retry_in.as_secs())),
                            _=>{},
                        }
                    }
                }
            }
            session.wait().await.map_err(|e|e.to_string())?;Ok(())
        })
    });
    // Network workers continue while this thread installs and computes one finished set at a time.
    while let Ok(event) = receiver.recv() {
        match event {
            Finished::Complete(id, name) => {
                let file = crate::maps::safe_file(scratch, &scratch.join(name))
                    .ok_or("Downloader returned an invalid archive path")?;
                let result = if fs::metadata(&file).map_err(|e| e.to_string())?.len() > MAX_ARCHIVE
                {
                    Err("Archive exceeds the download size limit".into())
                } else {
                    ctl.change(|s| {
                        s.phase = "installing".into();
                        s.message = format!(
                            "Installing mapset {id} · other downloads continue in the background"
                        );
                    });
                    install(&file, root, i64::from(id), ctl).and_then(|files| {
                        let updates = crate::recovery::installed(c, root, &files)?;
                        c.execute(
                            "INSERT OR REPLACE INTO download_installs VALUES(?1,?2,?3)",
                            params![
                                root.to_string_lossy(),
                                id,
                                serde_json::to_string(&files).unwrap()
                            ],
                        )
                        .map_err(|e| e.to_string())?;
                        ctl.change(|s| {
                            s.updates.extend(updates);
                            s.installed_sets += 1;
                        });
                        Ok(())
                    })
                };
                let _ = fs::remove_file(file);
                for hash in &sets[&id] {
                    if ctl.pause.load(Ordering::Acquire) {
                        continue;
                    }
                    match &result {
                        Ok(()) => {
                            let found = !crate::maps::cached(c, hash)?.is_empty();
                            job(
                                c,
                                root,
                                hash,
                                if found { "installed" } else { "mismatch" },
                                if found {
                                    ""
                                } else {
                                    "Mirror archive does not contain this historical revision"
                                },
                            )?;
                        }
                        Err(e) => job(c, root, hash, "failed", e)?,
                    }
                }
                if let Err(e) = result {
                    ctl.change(|s| {
                        if s.issues.len() < 8 {
                            s.issues.push(format!("Mapset {id}: {e}"));
                        }
                    });
                }
            }
            Finished::Failed(id, error) => {
                if !ctl.pause.load(Ordering::Acquire) {
                    for hash in &sets[&id] {
                        job(c, root, hash, "failed", &error)?;
                    }
                    ctl.change(|s| {
                        if s.issues.len() < 8 {
                            s.issues.push(format!(
                                "Mapset {id}: all available sources failed ({error})"
                            ));
                        }
                    });
                }
            }
        }
        totals(c, root, ctl, Some(selected))?;
    }
    network.join().map_err(|_| "Download scheduler stopped")??;
    Ok(())
}
pub(super) fn run(path: &Path, options: Options, ctl: Arc<Controller>) -> Result<(), String> {
    run_with(path, options, ctl, sources())
}
pub(super) fn run_with(
    path: &Path,
    options: Options,
    ctl: Arc<Controller>,
    mirrors: Vec<Mirror>,
) -> Result<(), String> {
    ctl.change(|s| {
        s.phase = "preparing".into();
        s.message = "Preparing automatic mirror downloads (up to 3 at once)…".into();
        s.issues.clear();
    });
    let (mut c, root, selected) = prepare(path, &options)?;
    // Give entries declared unavailable by the old single-mirror implementation a fresh chance once.
    if sources::value(&c, "mirror_policy").as_deref() != Some(POLICY) {
        c.execute("UPDATE download_jobs SET state='pending',error='' WHERE target=?1 AND state IN ('failed','mismatch')",[root.to_string_lossy()]).map_err(|e|e.to_string())?;
        c.execute(
            "INSERT OR REPLACE INTO metadata VALUES('mirror_policy',?1)",
            [POLICY],
        )
        .map_err(|e| e.to_string())?;
    }
    let jobs: Vec<String> = {
        let mut q = c
            .prepare("SELECT hash FROM download_jobs WHERE target=?1 AND state='pending'")
            .map_err(|e| e.to_string())?;
        q.query_map([root.to_string_lossy()], |r| r.get(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?
    };
    let mut known: BTreeMap<u32, Vec<String>> = BTreeMap::new();
    let mut unknown = Vec::new();
    for hash in jobs.into_iter().filter(|h| selected.contains(h)) {
        let id: Option<u32> = c
            .query_row("SELECT set_id FROM map_ids WHERE hash=?1", [&hash], |r| {
                r.get(0)
            })
            .optional()
            .map_err(|e| e.to_string())?;
        if let Some(id) = id {
            known.entry(id).or_default().push(hash);
        } else {
            unknown.push(hash);
        }
    }
    let scheduler = SchedulerHandle::new();
    let downloader = downloader(mirrors, scheduler)?;
    let scratch = path
        .parent()
        .unwrap()
        .join("beatmap-downloads")
        .join(format!(
            "queue-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;
    let mut seen = HashSet::new();
    let mut http = Http::new(&options)?;
    loop {
        if ctl.pause.load(Ordering::Acquire) {
            break;
        }
        if !known.is_empty() {
            totals(&c, &root, &ctl, Some(&selected))?;
            batch(
                &mut c,
                &root,
                &scratch,
                &known,
                &selected,
                &ctl,
                downloader.clone(),
            )?;
            seen.extend(known.keys().copied());
            known.clear();
        }
        if unknown.is_empty() {
            break;
        }
        for _ in 0..16 {
            if ctl.pause.load(Ordering::Acquire) {
                break;
            }
            let Some(hash) = unknown.pop() else { break };
            if options.client_id.is_empty() || options.client_secret.is_empty() {
                job(
                    &c,
                    &root,
                    &hash,
                    "needs_api",
                    "No catalog ID found. Add osu! API credentials.",
                )?;
                continue;
            }
            ctl.change(|s| {
                s.phase = "resolving".into();
                s.message = format!("Looking up beatmap ID {hash}…");
            });
            match http.resolve(&hash, &ctl) {
                Ok(Some((id, set))) => {
                    c.execute(
                        "INSERT OR REPLACE INTO map_ids VALUES(?1,?2,?3,'api')",
                        params![hash, id, set],
                    )
                    .map_err(|e| e.to_string())?;
                    if !crate::maps::cached(&c, &hash)?.is_empty() {
                        job(&c, &root, &hash, "installed", "")?;
                    } else if seen.contains(&(set as u32)) {
                        job(
                            &c,
                            &root,
                            &hash,
                            "mismatch",
                            "Downloaded mapset lacks this historical revision",
                        )?;
                    } else {
                        known.entry(set as u32).or_default().push(hash);
                    }
                }
                Ok(None) => job(
                    &c,
                    &root,
                    &hash,
                    "not_found",
                    "osu! API could not identify this exact revision",
                )?,
                Err(e) => {
                    job(&c, &root, &hash, "failed", &e)?;
                    ctl.change(|s| s.issues.push(e));
                    unknown.clear();
                    break;
                }
            }
        }
    }
    // Only our unique scratch directory is removed; installed game files are preserved.
    if scratch.parent() == Some(path.parent().unwrap().join("beatmap-downloads").as_path()) {
        let _ = fs::remove_dir_all(&scratch);
    }
    totals(&c, &root, &ctl, Some(&selected))?;
    ctl.change(|s| {
        s.phase = if ctl.pause.load(Ordering::Acquire) {
            "paused"
        } else {
            "finished"
        }
        .into();
        s.message = if s.phase == "paused" {
            "Downloads paused. Resume when ready.".into()
        } else {
            format!(
                "{} revisions ready · {} unavailable after automatic mirror recovery",
                s.completed, s.unavailable
            )
        };
    });
    Ok(())
}
