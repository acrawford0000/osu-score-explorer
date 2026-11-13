//! One request budget for official API work, including replay checks and beatmap lookups.
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};
static NEXT: Mutex<Option<Instant>> = Mutex::new(None);
pub fn pace(url: &str, cancel: impl Fn() -> bool) -> Result<(), String> {
    if reqwest::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(String::from))
        .as_deref()
        != Some("osu.ppy.sh")
    {
        return Ok(());
    }
    loop {
        if cancel() {
            return Err("Paused".into());
        }
        let delay = {
            let mut next = NEXT.lock().map_err(|e| e.to_string())?;
            let now = Instant::now();
            if next.is_none_or(|t| t <= now) {
                *next = Some(now + Duration::from_millis(1100));
                return Ok(());
            }
            next.unwrap().saturating_duration_since(now)
        };
        std::thread::sleep(delay.min(Duration::from_millis(100)));
    }
}
pub fn cooldown(seconds: u64) {
    if let Ok(mut next) = NEXT.lock() {
        let until = Instant::now() + Duration::from_secs(seconds.clamp(1, 300));
        *next = Some(next.map_or(until, |t| t.max(until)));
    }
}
