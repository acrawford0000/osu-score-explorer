// Only internally generated official osu! URLs reach the system browser.
#[cfg(windows)]
fn open_browser(address: &str) -> Result<(), String> {
    use windows_sys::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};
    let url: Vec<u16> = address.encode_utf16().chain(Some(0)).collect();
    let operation: Vec<u16> = "open".encode_utf16().chain(Some(0)).collect();
    // Both strings are NUL-terminated and alive for this synchronous call.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            url.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if result as isize <= 32 {
        Err("Could not open the osu! page in your browser".into())
    } else {
        Ok(())
    }
}
#[cfg(not(windows))]
fn open_browser(address: &str) -> Result<(), String> {
    Err(format!("Open {address} in your browser"))
}
pub fn open_registration() -> Result<(), String> {
    open_browser("https://osu.ppy.sh/home/account/edit#new-oauth-application")
}
pub fn score_url(id: &str, mode: u8, legacy: bool) -> Result<String, String> {
    if !id.bytes().all(|b| b.is_ascii_digit())
        || !id
            .parse::<u64>()
            .is_ok_and(|n| n > 0 && n < i64::MAX as u64)
    {
        return Err("This score has no valid online score ID".into());
    }
    let modes = ["osu", "taiko", "fruits", "mania"];
    let mode = modes
        .get(usize::from(mode))
        .ok_or("Unknown score ruleset")?;
    Ok(if legacy {
        format!("https://osu.ppy.sh/scores/{mode}/{id}")
    } else {
        format!("https://osu.ppy.sh/scores/{id}")
    })
}
pub fn open_score(id: &str, mode: u8, legacy: bool) -> Result<(), String> {
    open_browser(&score_url(id, mode, legacy)?)
}
