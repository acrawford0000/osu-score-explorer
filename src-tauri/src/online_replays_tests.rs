use super::*;
use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    sync::Arc,
};
fn fixture() -> (PathBuf, PathBuf, PathBuf, Score) {
    let root = std::env::temp_dir().join(format!(
        "sx-online-replay-{}-{}",
        std::process::id(),
        now_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let output = root.join("exports");
    fs::create_dir(&output).unwrap();
    let path = root.join("library.sqlite");
    let mut s = replays::tests::score("stable");
    s.legacy_online_id = Some("7".into());
    let c = library::db(&path).unwrap();
    c.execute(
        "INSERT INTO scores(id,data) VALUES(?1,?2)",
        params![s.id, serde_json::to_string(&s).unwrap()],
    )
    .unwrap();
    (root, path, output, s)
}
fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}
type Reply = (String, u16, String, Vec<u8>);
fn server(
    replies: Vec<Reply>,
) -> (
    Service,
    Arc<Mutex<Vec<String>>>,
    std::thread::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    let worker = std::thread::spawn(move || {
        for (expected, status, headers, body) in replies {
            let until = Instant::now() + Duration::from_secs(12);
            let mut socket = loop {
                if let Ok((s, _)) = listener.accept() {
                    break s;
                }
                assert!(Instant::now() < until, "Missing request for {expected}");
                std::thread::sleep(Duration::from_millis(5));
            };
            socket.set_nonblocking(false).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0; 4096];
            while !bytes.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = socket.read(&mut buffer).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
            }
            let request = String::from_utf8_lossy(&bytes).to_string();
            assert_eq!(request.split_whitespace().nth(1), Some(expected.as_str()));
            if expected.starts_with("/api/") {
                assert!(
                    request
                        .to_lowercase()
                        .contains("authorization: bearer fixture-token")
                );
                assert!(!request.contains("fixture-secret"));
            }
            log.lock().unwrap().push(request);
            write!(
                socket,
                "HTTP/1.1 {status} OK\r\nContent-Length: {}\r\n{headers}Connection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            socket.write_all(&body).unwrap();
        }
    });
    (
        Service {
            api: format!("{base}/api/v2"),
            oauth: format!("{base}/oauth/token"),
            ..Service::default()
        },
        seen,
        worker,
    )
}
fn auth() -> Reply {
    (
        "/oauth/token".into(),
        200,
        "Content-Type: application/json\r\n".into(),
        br#"{"access_token":"fixture-token","expires_in":3600}"#.to_vec(),
    )
}
fn details(s: &Score, available: bool) -> Vec<u8> {
    serde_json::json!({"id":900,"legacy_score_id":7,"ruleset_id":s.mode,"ended_at":s.date,"has_replay":available,"beatmap":{"checksum":s.map_hash}}).to_string().into_bytes()
}
fn connect(service: &Service) {
    service
        .connect(Credentials {
            client_id: "123".into(),
            client_secret: "fixture-secret".into(),
        })
        .unwrap();
}
#[test]
fn namespace_ids_are_strict_and_all_four_legacy_rulesets_are_supported() {
    let mut s = replays::tests::score("stable");
    s.legacy_online_id = Some("7".into());
    for (mode, name) in ["osu", "taiko", "fruits", "mania"].iter().enumerate() {
        s.mode = mode as u8;
        assert_eq!(candidates(&[s.clone()])[0].key, format!("scores/{name}/7"));
    }
    s.solo_online_id = Some("900".into());
    assert_eq!(candidates(&[s.clone()])[0].key, "scores/900");
    s.legacy_online_id = Some("../7".into());
    s.solo_online_id = Some("0".into());
    assert!(candidates(&[s]).is_empty());
}
#[test]
fn availability_is_cached_and_no_online_id_does_not_make_a_request() {
    let (root, path, _, s) = fixture();
    let (service, seen, worker) = server(vec![
        auth(),
        (
            "/api/v2/scores/osu/7".into(),
            200,
            String::new(),
            details(&s, true),
        ),
    ]);
    connect(&service);
    assert_eq!(service.check(&path, &s.id).unwrap().state, "available");
    assert_eq!(service.check(&path, &s.id).unwrap().state, "available");
    let c = library::db(&path).unwrap();
    let mut offline = s.clone();
    offline.id = "offline".into();
    offline.legacy_online_id = None;
    c.execute(
        "INSERT INTO scores(id,data) VALUES(?1,?2)",
        params![offline.id, serde_json::to_string(&offline).unwrap()],
    )
    .unwrap();
    assert_eq!(service.check(&path, &offline.id).unwrap().state, "no-id");
    worker.join().unwrap();
    assert_eq!(seen.lock().unwrap().len(), 2);
    drop(c);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn exports_verified_original_bytes_preserves_extensions_and_reuses_the_saved_copy() {
    let (root, path, output, s) = fixture();
    let bytes = replays::tests::replay_bytes(&s, 30_000_019);
    let (service, seen, worker) = server(vec![
        auth(),
        (
            "/api/v2/scores/osu/7".into(),
            200,
            String::new(),
            details(&s, true),
        ),
        (
            "/api/v2/scores/osu/7/download".into(),
            200,
            "Content-Type: application/x-osu-replay\r\n".into(),
            bytes.clone(),
        ),
    ]);
    connect(&service);
    assert_eq!(service.check(&path, &s.id).unwrap().state, "available");
    let first = service.export(&path, &s.id, &output).unwrap();
    assert_eq!(fs::read(&first).unwrap(), bytes);
    service.disconnect();
    let second = replays::export(&path, &s.id, &output).unwrap();
    assert_ne!(first, second);
    assert_eq!(fs::read(second).unwrap(), bytes);
    assert_eq!(
        replays::availability(&path, std::slice::from_ref(&s.id), None)
            .unwrap()
            .available
            .len(),
        1
    );
    worker.join().unwrap();
    assert_eq!(seen.lock().unwrap().len(), 3);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn rate_limits_and_temporary_errors_never_cache_replays_as_missing() {
    for status in [429, 503] {
        let (root, path, _, s) = fixture();
        let (service, _, worker) = server(vec![
            auth(),
            (
                "/api/v2/scores/osu/7".into(),
                status,
                "Retry-After: 60\r\n".into(),
                vec![],
            ),
        ]);
        connect(&service);
        let result = service.check(&path, &s.id);
        if status == 429 {
            let result = result.unwrap();
            assert_eq!(result.state, "rate-limited");
            assert!(result.retry_after.unwrap() > 0);
            assert_eq!(service.check(&path, &s.id).unwrap().state, "rate-limited");
        } else {
            assert!(result.is_err());
        }
        let c = library::db(&path).unwrap();
        assert_eq!(
            c.query_row("SELECT count(*) FROM online_replay_checks", [], |r| r
                .get::<_, u32>(0))
                .unwrap(),
            0
        );
        drop(c);
        worker.join().unwrap();
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn merged_stable_score_can_download_through_its_lazer_online_id_after_legacy_not_found() {
    let (root, path, output, s) = fixture();
    let mut alias = s.clone();
    alias.id = "lazer:alias".into();
    alias.source = "lazer".into();
    alias.solo_online_id = Some("900".into());
    alias.legacy_online_id = None;
    let c = library::db(&path).unwrap();
    c.execute(
        "INSERT INTO scores(id,data) VALUES(?1,?2)",
        params![alias.id, serde_json::to_string(&alias).unwrap()],
    )
    .unwrap();
    c.execute(
        "INSERT INTO score_links VALUES(?1,?2)",
        params![alias.id, s.id],
    )
    .unwrap();
    drop(c);
    let mut old_name = s.clone();
    old_name.player = "Historical username".into();
    let bytes = replays::tests::replay_bytes(&old_name, 30_000_019);
    let (service, _, worker) = server(vec![
        auth(),
        ("/api/v2/scores/osu/7".into(), 404, String::new(), vec![]),
        (
            "/api/v2/scores/900".into(),
            200,
            String::new(),
            details(&s, true),
        ),
        (
            "/api/v2/scores/900/download".into(),
            200,
            String::new(),
            bytes.clone(),
        ),
    ]);
    connect(&service);
    assert_eq!(service.check(&path, &s.id).unwrap().state, "available");
    assert_eq!(
        fs::read(service.export(&path, &s.id, &output).unwrap()).unwrap(),
        bytes
    );
    worker.join().unwrap();
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn missing_replays_are_cached_but_mismatching_score_responses_are_not() {
    for mismatch in [false, true] {
        let (root, path, _, s) = fixture();
        let mut value: serde_json::Value = serde_json::from_slice(&details(&s, false)).unwrap();
        if mismatch {
            value["ruleset_id"] = serde_json::json!(1);
        }
        let (service, _, worker) = server(vec![
            auth(),
            (
                "/api/v2/scores/osu/7".into(),
                200,
                String::new(),
                value.to_string().into_bytes(),
            ),
        ]);
        connect(&service);
        let result = service.check(&path, &s.id);
        if mismatch {
            assert!(result.is_err());
        } else {
            assert_eq!(result.unwrap().state, "unavailable");
            assert_eq!(service.check(&path, &s.id).unwrap().state, "unavailable");
        }
        worker.join().unwrap();
        fs::remove_dir_all(root).unwrap();
    }
}
#[test]
fn wrong_replay_and_html_downloads_are_rejected_without_writing_an_export() {
    for html in [false, true] {
        let (root, path, output, s) = fixture();
        let mut other = s.clone();
        other.map_hash = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into();
        let bytes = if html {
            b"<html>sign in</html>".to_vec()
        } else {
            replays::tests::replay_bytes(&other, 20_170_101)
        };
        let (service, _, worker) = server(vec![
            auth(),
            (
                "/api/v2/scores/osu/7/download".into(),
                200,
                String::new(),
                bytes,
            ),
        ]);
        connect(&service);
        assert!(service.export(&path, &s.id, &output).is_err());
        assert_eq!(fs::read_dir(&output).unwrap().count(), 0);
        worker.join().unwrap();
        fs::remove_dir_all(root).unwrap();
    }
}
