use super::*;
use std::{io::Cursor, net::TcpListener};

pub(crate) fn fixture(label: &str) -> (PathBuf, PathBuf, PathBuf) {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("sx-download-{label}-{nonce}"));
    let songs = root.join("Songs");
    fs::create_dir_all(&songs).unwrap();
    let cache = root.join("library.sqlite");
    (root, songs, cache)
}
pub(crate) fn map(title: &str) -> Vec<u8> {
    format!("osu file format v14\n[General]\nAudioFilename: audio.mp3\nMode:0\n[Metadata]\nTitle:{title}\nArtist:Artist\nCreator:Mapper\nVersion:Hard\nBeatmapID:10\nBeatmapSetID:5\n[Difficulty]\nHPDrainRate:5\nCircleSize:4\nOverallDifficulty:5\nApproachRate:5\nSliderMultiplier:1.4\nSliderTickRate:1\n[Events]\n0,0,\"bg.jpg\",0,0\nVideo,0,\"video.mp4\"\n[TimingPoints]\n0,500,4,2,0,100,1,0\n[HitObjects]\n256,192,1000,1,0,0:0:0:0:\n256,192,1500,1,0,0:0:0:0:\n").into_bytes()
}
fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        out.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        out.write_all(bytes).unwrap();
    }
    out.finish().unwrap().into_inner()
}
pub(crate) fn score(c: &Connection, id: &str, hash: &str, player: &str) {
    let s:Score=serde_json::from_value(serde_json::json!({"id":id,"mapHash":hash,"player":player,"date":"2020-01-01T00:00:00Z","mode":0,"title":"Unmatched beatmap","artist":"","difficulty":"","mapper":"","mods":0,"n300":1,"n100":0,"n50":0,"misses":1,"combo":1,"maxCombo":null,"rawScore":200,"accuracy":50.0,"fc":false,"ss":false,"stars":null,"pp":null,"fcPp":null,"source":"stable","status":"Map missing","hasArtwork":false})).unwrap();
    c.execute(
        "INSERT INTO scores VALUES(?1,?2,NULL)",
        params![id, serde_json::to_string(&s).unwrap()],
    )
    .unwrap();
}
fn options(songs: &Path) -> Options {
    Options {
        songs: songs.to_string_lossy().into(),
        player: Some("Friend".into()),
        client_id: String::new(),
        client_secret: String::new(),
    }
}
fn server(
    responses: Vec<(u16, &'static str, Vec<u8>)>,
) -> (String, Arc<Mutex<Vec<String>>>, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let captured = requests.clone();
    let worker = std::thread::spawn(move || {
        for (status, extra, body) in responses {
            let deadline = Instant::now() + Duration::from_secs(15);
            let mut socket = loop {
                match listener.accept() {
                    Ok((s, _)) => break s,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            Instant::now() < deadline,
                            "Expected HTTP request did not arrive"
                        );
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(e) => panic!("{e}"),
                }
            };
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut b = [0; 4096];
            loop {
                let n = socket.read(&mut b).unwrap();
                if n == 0 {
                    break;
                }
                bytes.extend_from_slice(&b[..n]);
                if let Some(end) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&bytes[..end]);
                    let len = head
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|n| n.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if bytes.len() >= end + 4 + len {
                        break;
                    }
                }
            }
            captured
                .lock()
                .unwrap()
                .push(String::from_utf8_lossy(&bytes).into());
            write!(
                socket,
                "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n",
                body.len()
            )
            .unwrap();
            socket.write_all(&body).unwrap();
        }
    });
    (base, requests, worker)
}
fn local_http(o: &Options, base: &str) -> Http {
    let mut h = Http::new(o).unwrap();
    h.client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    h.oauth = format!("{base}/oauth");
    h.api = format!("{base}/lookup");
    h.mirror = format!("{base}/download");
    h
}

#[test]
fn catalog_ids_download_one_set_and_refresh_only_exact_scores_without_video() {
    let (root, songs, cache) = fixture("catalog");
    let c = library::db(&cache).unwrap();
    let a = map("Exact A");
    let b = map("Exact B");
    let ha = format!("{:x}", md5::compute(&a));
    let hb = format!("{:x}", md5::compute(&b));
    let old = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    for (id, h) in [("a", ha.as_str()), ("b", hb.as_str()), ("old", old)] {
        score(&c, id, h, "Friend");
        c.execute("INSERT INTO map_ids VALUES(?1,10,5,'catalog')", [h])
            .unwrap();
    }
    score(
        &c,
        "other",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "Other player",
    );
    let existing = songs.join("5 Existing");
    fs::create_dir(&existing).unwrap();
    fs::write(existing.join("custom.osu"), b"custom original").unwrap();
    let zip = archive(&[
        ("a.osu", &a),
        ("b.osu", &b),
        ("audio.mp3", b"audio"),
        ("bg.jpg", &[0xff, 0xd8, 0xff]),
        ("video.mp4", b"video"),
    ]);
    let (base, requests, worker) = server(vec![(
        200,
        "Content-Type: application/x-osu-beatmap-archive\r\n",
        zip,
    )]);
    let o = options(&songs);
    let ctl = Arc::new(Controller::default());
    run_http(&cache, o.clone(), ctl.clone(), local_http(&o, &base)).unwrap();
    worker.join().unwrap();
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].starts_with("GET /download/5?noVideo=1 "));
    assert!(!requests[0].to_ascii_lowercase().contains("authorization:"));
    let l = library::load(&cache).unwrap();
    for id in ["a", "b"] {
        let s = l.scores.iter().find(|s| s.id == id).unwrap();
        assert!(s.pp.is_some());
        assert!(s.fc_pp.is_some());
        assert!(s.stars.is_some());
        assert_eq!(s.max_combo, Some(2));
        assert!(s.has_artwork);
        assert_eq!(s.player, "Friend");
    }
    assert!(
        l.scores
            .iter()
            .find(|s| s.id == "old")
            .unwrap()
            .pp
            .is_none()
    );
    assert_eq!(
        l.scores.iter().find(|s| s.id == "other").unwrap().title,
        "Unmatched beatmap"
    );
    assert_eq!(
        fs::read(existing.join("custom.osu")).unwrap(),
        b"custom original"
    );
    assert!(!songs.join("5 Score Explorer/video.mp4").exists());
    assert_eq!(fs::read(songs.join("5 Score Explorer/a.osu")).unwrap(), a);
    let state = ctl.status();
    assert_eq!(state.installed_sets, 1);
    assert_eq!(state.completed, 2);
    assert_eq!(state.unavailable, 1);
    assert_eq!(state.updates.len(), 2);
    assert!(ctl.status().updates.is_empty());
    let persisted: String = c
        .query_row(
            "SELECT state FROM download_jobs WHERE hash=?1",
            [old],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(persisted, "mismatch");
    // Cached PP must survive a later importer, not just the immediate UI update.
    let calc: String = c
        .query_row(
            "SELECT data FROM calculations WHERE key=?1",
            [format!("{}:a", library::ENGINE)],
            |r| r.get(0),
        )
        .unwrap();
    assert!(serde_json::from_str::<serde_json::Value>(&calc).unwrap()["fc_pp"].is_number());
    drop(c);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn api_fallback_uses_checksum_oauth_and_caches_ids_without_saving_credentials() {
    let (root, songs, cache) = fixture("api");
    let c = library::db(&cache).unwrap();
    let a = map("API song");
    let hash = format!("{:x}", md5::compute(&a));
    score(&c, "api-score", &hash, "Friend");
    let zip = archive(&[("map.osu", &a), ("audio.mp3", b"audio")]);
    let (base, requests, worker) = server(vec![
        (
            200,
            "Content-Type: application/json\r\n",
            br#"{"access_token":"test-access-token","expires_in":86400}"#.to_vec(),
        ),
        (
            200,
            "Content-Type: application/json\r\n",
            serde_json::json!({"id":10,"beatmapset_id":5,"checksum":hash})
                .to_string()
                .into_bytes(),
        ),
        (200, "", zip),
    ]);
    let mut o = options(&songs);
    o.client_id = "123".into();
    o.client_secret = "test-client-secret".into();
    let ctl = Arc::new(Controller::default());
    run_http(&cache, o.clone(), ctl.clone(), local_http(&o, &base)).unwrap();
    worker.join().unwrap();
    let requests = requests.lock().unwrap();
    assert!(requests[0].starts_with("POST /oauth "));
    assert!(requests[0].contains("grant_type=client_credentials"));
    assert!(requests[1].starts_with(&format!("GET /lookup?checksum={hash} ")));
    assert!(
        requests[1]
            .to_ascii_lowercase()
            .contains("authorization: bearer test-access-token")
    );
    assert!(!requests[2].contains("test-access-token"));
    assert!(!requests[2].contains("test-client-secret"));
    let origin: String = c
        .query_row("SELECT origin FROM map_ids WHERE hash=?1", [hash], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(origin, "api");
    let metadata: Vec<String> = {
        let mut q = c.prepare("SELECT data FROM metadata").unwrap();
        q.query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    assert!(
        !metadata
            .iter()
            .any(|v| v.contains("test-client-secret") || v.contains("test-access-token"))
    );
    assert!(library::load(&cache).unwrap().scores[0].pp.is_some());
    drop(c);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn missing_credentials_and_pause_preserve_pending_jobs_for_resume() {
    let (root, songs, cache) = fixture("resume");
    let c = library::db(&cache).unwrap();
    score(&c, "missing", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "Friend");
    let o = options(&songs);
    let ctl = Arc::new(Controller::default());
    run(&cache, o.clone(), ctl.clone()).unwrap();
    assert_eq!(ctl.status().unavailable, 1);
    let state: String = c
        .query_row("SELECT state FROM download_jobs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(state, "needs_api");
    ctl.pause.store(true, Ordering::Release);
    let (_, _, _) = prepare(&cache, &o).unwrap();
    run(&cache, o, ctl.clone()).unwrap();
    assert_eq!(ctl.status().phase, "paused");
    let state: String = c
        .query_row("SELECT state FROM download_jobs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(state, "pending");
    assert!(!fs::read_dir(&songs).unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".score-explorer")
    }));
    drop(c);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rejects_escaping_paths_and_invalid_archives_without_touching_existing_files() {
    let (root, songs, _) = fixture("unsafe");
    let ctl = Controller::default();
    for name in [
        "../outside.osu",
        "a/../../outside.osu",
        "..\\outside.osu",
        "/absolute.osu",
        "C:\\escape.osu",
        "audio.mp3:stream",
    ] {
        assert!(safe_entry(name).is_err());
    }
    let file = root.join("bad.osz");
    fs::write(&file, archive(&[("../outside.osu", b"invalid")])).unwrap();
    assert!(install(&file, &songs, 5, &ctl).is_err());
    assert!(!root.join("outside.osu").exists());
    assert_eq!(fs::read_dir(&songs).unwrap().count(), 0);
    fs::write(&file, b"<html>not a map</html>").unwrap();
    assert!(install(&file, &songs, 5, &ctl).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rate_limited_download_retries_with_no_video_and_does_not_forward_credentials() {
    let (root, songs, _) = fixture("retry");
    let o = options(&songs);
    let (base, requests, worker) = server(vec![
        (429, "Retry-After: 0\r\n", vec![]),
        (200, "", b"zip bytes".to_vec()),
    ]);
    let h = local_http(&o, &base);
    let file = root.join("download.osz");
    h.download(5, &file, &Controller::default()).unwrap();
    worker.join().unwrap();
    assert_eq!(requests.lock().unwrap().len(), 2);
    assert_eq!(fs::read(&file).unwrap(), b"zip bytes");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn native_lazer_recovery_keeps_original_judgments_and_gameplay_rules() {
    let (root, songs, cache) = fixture("lazer");
    let mut c = library::db(&cache).unwrap();
    let bytes = map("Lazer song");
    let hash = format!("{:x}", md5::compute(&bytes));
    score(&c, "lazer:fixture", &hash, "Friend");
    c.execute("UPDATE scores SET data=json_set(data,'$.source','lazer','$.gameplayLazer',json('true'),'$.modsJson','[]')",[]).unwrap();
    let input = serde_json::json!(["realm-adapter-3",crate::lazer::ENGINE,"2020-01-01T00:00:00Z",{"Great":1,"Miss":1},{"Great":2},[],1,"sha",hash,false,null]);
    c.execute(
        "INSERT INTO metadata VALUES('lazer-input:lazer:fixture',?1)",
        [input.to_string()],
    )
    .unwrap();
    let file = songs.join("map.osu");
    fs::write(&file, bytes).unwrap();
    let updates = crate::recovery::installed(&mut c, &songs, &[file]).unwrap();
    assert_eq!(updates.len(), 1);
    let s = &updates[0];
    assert!(s.pp.is_some());
    assert!(s.fc_pp.is_some());
    assert_eq!(s.n300, 1);
    assert_eq!(s.misses, 1);
    assert!(s.gameplay_lazer);
    assert_eq!(s.calculation_version, crate::lazer::ENGINE);
    drop(c);
    fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "Explicit live mirror check; downloads one public mapset into an isolated temporary folder"]
fn live_mirror_download_and_extraction() {
    let (root, songs, _) = fixture("live");
    let o = options(&songs);
    let ctl = Controller::default();
    let archive = root.join("public.osz");
    Http::new(&o).unwrap().download(1, &archive, &ctl).unwrap();
    let files = install(&archive, &songs, 1, &ctl).unwrap();
    assert!(!files.is_empty());
    for file in &files {
        assert!(rosu_pp::Beatmap::from_bytes(&fs::read(file).unwrap()).is_ok());
    }
    let mut dirs = vec![songs];
    while let Some(dir) = dirs.pop() {
        for entry in fs::read_dir(dir).unwrap() {
            let p = entry.unwrap().path();
            if p.is_dir() {
                dirs.push(p);
            } else {
                assert!(!video(&p));
            }
        }
    }
    println!(
        "Live mirror returned {} readable maps, installed without video",
        files.len()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn provided_downloader_switches_mirrors_after_not_found_and_rate_limit() {
    for status in [404, 429] {
        let (root, songs, path) = fixture(&format!("failover-{status}"));
        let c = library::db(&path).unwrap();
        let bytes = map("Recovered through fallback");
        let hash = format!("{:x}", md5::compute(&bytes));
        score(&c, "fallback", &hash, "Friend");
        c.execute("INSERT INTO map_ids VALUES(?1,10,5,'catalog')", [hash])
            .unwrap();
        let (bad, bad_requests, bad_worker) = server(vec![(status, "Retry-After: 60\r\n", vec![])]);
        let (good, good_requests, good_worker) = server(vec![(
            200,
            "Content-Type: application/octet-stream\r\n",
            archive(&[("map.osu", &bytes)]),
        )]);
        let mirrors = vec![
            osu_downloader::Mirror::custom(format!(
                "{}/{{id}}?noVideo=1",
                bad.replace("127.0.0.1", "localhost")
            ))
            .unwrap(),
            osu_downloader::Mirror::custom(format!("{good}/{{id}}?noVideo=1")).unwrap(),
        ];
        let ctl = Arc::new(Controller::default());
        super::queue::run_with(&path, options(&songs), ctl.clone(), mirrors).unwrap();
        bad_worker.join().unwrap();
        good_worker.join().unwrap();
        assert_eq!(bad_requests.lock().unwrap().len(), 1);
        assert_eq!(good_requests.lock().unwrap().len(), 1);
        assert_eq!(ctl.status().completed, 1);
        assert!(library::load(&path).unwrap().scores[0].pp.is_some());
        drop(c);
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn provided_downloader_runs_three_downloads_concurrently_with_one_score_writer() {
    use std::sync::atomic::AtomicUsize;
    let (root, songs, path) = fixture("concurrent");
    let c = library::db(&path).unwrap();
    let mut bodies = HashMap::new();
    for id in [5, 6, 7] {
        let bytes = map(&format!("Song {id}"));
        let hash = format!("{:x}", md5::compute(&bytes));
        score(&c, &format!("score-{id}"), &hash, "Friend");
        c.execute(
            "INSERT INTO map_ids VALUES(?1,10,?2,'catalog')",
            params![hash, id],
        )
        .unwrap();
        bodies.insert(id, archive(&[("map.osu", &bytes)]));
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let active = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let observed = peak.clone();
    let network = std::thread::spawn(move || {
        let mut children = Vec::new();
        let limit = Instant::now() + Duration::from_secs(15);
        for _ in 0..3 {
            let mut socket = loop {
                match listener.accept() {
                    Ok((s, _)) => break s,
                    Err(_) => {
                        assert!(Instant::now() < limit, "Missing concurrent request");
                        std::thread::sleep(Duration::from_millis(10));
                    }
                }
            };
            let active = active.clone();
            let peak = peak.clone();
            let bodies = bodies.clone();
            children.push(std::thread::spawn(move||{
                socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();let mut request=Vec::new();let mut buf=[0;4096];while !request.windows(4).any(|w|w==b"\r\n\r\n"){let n=socket.read(&mut buf).unwrap();if n==0{break;}request.extend_from_slice(&buf[..n]);}
                let text=String::from_utf8_lossy(&request);let url=text.split_whitespace().nth(1).unwrap();let id=url.trim_start_matches('/').split('?').next().unwrap().parse::<i32>().unwrap();
                let n=active.fetch_add(1,Ordering::AcqRel)+1;peak.fetch_max(n,Ordering::AcqRel);std::thread::sleep(Duration::from_secs(1));let body=&bodies[&id];write!(socket,"HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Disposition: attachment; filename=\"{id}.osz\"\r\nConnection: close\r\n\r\n",body.len()).unwrap();socket.write_all(body).unwrap();active.fetch_sub(1,Ordering::AcqRel);
            }));
        }
        for c in children {
            c.join().unwrap();
        }
    });
    let cold = TcpListener::bind("127.0.0.1:0").unwrap();
    cold.set_nonblocking(true).unwrap();
    let mirrors = vec![
        osu_downloader::Mirror::custom(format!("{base}/{{id}}?noVideo=1")).unwrap(),
        osu_downloader::Mirror::custom(format!(
            "http://localhost:{}/{{id}}",
            cold.local_addr().unwrap().port()
        ))
        .unwrap(),
    ];
    let ctl = Arc::new(Controller::default());
    super::queue::run_with(&path, options(&songs), ctl.clone(), mirrors).unwrap();
    network.join().unwrap();
    assert!(
        cold.accept().is_err(),
        "Healthy primary downloads must not spill onto a cold fallback just to avoid normal request spacing"
    );
    assert!(
        observed.load(Ordering::Acquire) >= 2,
        "Requests should overlap"
    );
    assert!(observed.load(Ordering::Acquire) <= 3);
    assert_eq!(ctl.status().completed, 3);
    assert_eq!(
        library::load(&path)
            .unwrap()
            .scores
            .iter()
            .filter(|s| s.pp.is_some())
            .count(),
        3
    );
    drop(c);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn silent_response_headers_release_the_download_slot_and_use_another_mirror() {
    use std::sync::atomic::AtomicUsize;
    let (root, _, _) = fixture("silent-mirror");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let bad = format!("http://{}", listener.local_addr().unwrap());
    let stop = Arc::new(AtomicBool::new(false));
    let quit = stop.clone();
    let attempts = Arc::new(AtomicUsize::new(0));
    let observed = attempts.clone();
    let stalled = std::thread::spawn(move || {
        let mut sockets = Vec::new();
        while !quit.load(Ordering::Acquire) {
            if let Ok((socket, _)) = listener.accept() {
                observed.fetch_add(1, Ordering::AcqRel);
                sockets.push(socket);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    });
    let bytes = map("Healthy fallback");
    let zipped = archive(&[("map.osu", &bytes)]);
    let (good, requests, server) = server(vec![(
        200,
        "Content-Type: application/octet-stream\r\n",
        zipped,
    )]);
    let mirrors = vec![
        osu_downloader::Mirror::custom(format!("{bad}/{{id}}")).unwrap(),
        osu_downloader::Mirror::custom(format!("{good}/{{id}}")).unwrap(),
    ];
    let downloader = super::queue::downloader_with_timeout(
        mirrors,
        osu_downloader::SchedulerHandle::new(),
        Duration::from_millis(150),
    )
    .unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let start = Instant::now();
    let result = runtime.block_on(async {
        tokio::time::timeout(
            Duration::from_secs(8),
            downloader.download_many([5], &root).wait(),
        )
        .await
    });
    stop.store(true, Ordering::Release);
    stalled.join().unwrap();
    server.join().unwrap();
    assert!(
        result.is_ok(),
        "Silent headers must not hold a slot indefinitely"
    );
    result.unwrap().unwrap();
    assert!(start.elapsed() < Duration::from_secs(8));
    assert_eq!(
        attempts.load(Ordering::Acquire),
        3,
        "Bound every retry before trying another source"
    );
    assert_eq!(requests.lock().unwrap().len(), 1);
    assert!(
        fs::read_dir(&root)
            .unwrap()
            .filter_map(Result::ok)
            .any(|f| f.path().extension().is_some_and(|e| e == "osz"))
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "Explicit production downloader live check in an isolated temporary Songs folder"]
fn live_production_download_uses_bounded_concurrent_pipeline() {
    let (root, songs, path) = fixture("live-production");
    let c = library::db(&path).unwrap();
    let hash = "00000000000000000000000000000001";
    score(&c, "live", hash, "Friend");
    c.execute("INSERT INTO map_ids VALUES(?1,1,1,'catalog')", [hash])
        .unwrap();
    drop(c);
    let ctl = Arc::new(Controller::default());
    let started = Instant::now();
    super::queue::run_with(&path, options(&songs), ctl.clone(), super::queue::sources()).unwrap();
    let status = ctl.status();
    assert_eq!(status.downloaded_sets, 1, "{:?}", status.issues);
    assert_eq!(status.installed_sets, 1);
    assert_eq!(status.active_downloads, 0);
    println!(
        "Production downloader fetched and installed one public set in {:.2}s",
        started.elapsed().as_secs_f64()
    );
    fs::remove_dir_all(root).unwrap();
}
