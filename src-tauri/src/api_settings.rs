//! One durable API connection, protected by the current Windows user's DPAPI keys.
use crate::{
    library,
    online_replays::{Credentials, Service},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::path::Path;
const KEY: &str = "osu_api_connection_dpapi_v1";
#[derive(Serialize, Deserialize)]
struct Saved {
    credentials: Credentials,
    auto_connect: bool,
}
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub connected: bool,
    pub saved: bool,
    pub client_id: String,
    pub message: String,
}

#[cfg(windows)]
fn crypt(bytes: &[u8], protect: bool) -> Result<Vec<u8>, String> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
        },
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(bytes.len()).map_err(|_| "API details exceed the supported size")?,
        pbData: bytes.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    // DPAPI consumes the input synchronously. Output is a Windows allocation;
    // copy it before freeing it through LocalFree. No machine-wide flag is used.
    let ok = unsafe {
        if protect {
            CryptProtectData(
                &input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };
    if ok == 0 {
        return Err(if protect{"Windows could not protect the API details"}else{"Saved API details could not be unlocked by this Windows account. Re-enter them or forget the saved connection."}.into());
    }
    let result = if output.cbData == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() }
    };
    unsafe {
        LocalFree(output.pbData.cast());
    }
    Ok(result)
}
#[cfg(not(windows))]
fn crypt(_bytes: &[u8], _protect: bool) -> Result<Vec<u8>, String> {
    Err("Secure API persistence currently requires Windows".into())
}
fn load(path: &Path) -> Result<Option<Saved>, String> {
    let c = library::db(path)?;
    let data: Option<String> = c
        .query_row("SELECT data FROM metadata WHERE key=?1", [KEY], |r| {
            r.get(0)
        })
        .optional()
        .map_err(|e| e.to_string())?;
    let Some(data) = data else { return Ok(None) };
    if data.len() > 128 * 1024 {
        return Err("Saved API details are not valid".into());
    }
    let encrypted = STANDARD
        .decode(data)
        .map_err(|_| "Saved API details are not valid")?;
    let mut plain = crypt(&encrypted, false)?;
    let saved =
        serde_json::from_slice(&plain).map_err(|_| "Saved API details are not valid".to_string());
    plain.fill(0);
    saved.map(Some)
}
fn store(path: &Path, saved: &Saved) -> Result<(), String> {
    let mut plain = serde_json::to_vec(saved).map_err(|e| e.to_string())?;
    let encrypted = crypt(&plain, true);
    plain.fill(0);
    let encrypted = STANDARD.encode(encrypted?);
    library::db(path)?
        .execute(
            "INSERT OR REPLACE INTO metadata VALUES(?1,?2)",
            params![KEY, encrypted],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}
pub fn connect(
    path: &Path,
    service: &Service,
    mut credentials: Credentials,
) -> Result<State, String> {
    credentials.client_id = credentials.client_id.trim().into();
    credentials.client_secret = credentials.client_secret.trim().into();
    if credentials.client_secret.is_empty() {
        credentials.client_secret = load(path)?
            .filter(|s| s.credentials.client_id == credentials.client_id)
            .map(|s| s.credentials.client_secret)
            .ok_or("Enter the client secret for this Client ID")?;
    }
    let client_id = credentials.client_id.clone();
    service.connect_committing(credentials, |c| {
        store(
            path,
            &Saved {
                credentials: c.clone(),
                auto_connect: true,
            },
        )
    })?;
    Ok(State {
        connected: true,
        saved: true,
        client_id,
        message: String::new(),
    })
}
pub fn restore(path: &Path, service: &Service) -> Result<State, String> {
    let Some(saved) = load(path)? else {
        return Ok(State::default());
    };
    let client_id = saved.credentials.client_id.clone();
    if !saved.auto_connect {
        return Ok(State {
            saved: true,
            client_id,
            ..State::default()
        });
    }
    match service.connect(saved.credentials) {
        Ok(()) => Ok(State {
            connected: true,
            saved: true,
            client_id,
            message: String::new(),
        }),
        Err(e) => Ok(State {
            saved: true,
            client_id,
            message: format!("Your saved API details are available. Could not reconnect: {e}"),
            ..State::default()
        }),
    }
}
pub fn disconnect(path: &Path, service: &Service) -> Result<State, String> {
    let mut state = State::default();
    if let Some(mut saved) = load(path)? {
        saved.auto_connect = false;
        store(path, &saved)?;
        state.client_id = saved.credentials.client_id;
        state.saved = true;
    }
    service.disconnect();
    Ok(state)
}
pub fn forget(path: &Path, service: &Service) -> Result<State, String> {
    library::db(path)?
        .execute("DELETE FROM metadata WHERE key=?1", [KEY])
        .map_err(|e| e.to_string())?;
    service.disconnect();
    Ok(State::default())
}
#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn encrypted_details_survive_new_service_instances_and_disconnect_without_leaking_secrets() {
        let root = std::env::temp_dir().join(format!(
            "sx-api-settings-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("library.sqlite");
        store(
            &path,
            &Saved {
                credentials: Credentials {
                    client_id: "12345".into(),
                    client_secret: "test-secret-not-plaintext".into(),
                },
                auto_connect: false,
            },
        )
        .unwrap();
        let c = library::db(&path).unwrap();
        let encoded: String = c
            .query_row("SELECT data FROM metadata WHERE key=?1", [KEY], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(!encoded.contains("test-secret"));
        assert!(
            !STANDARD
                .decode(encoded)
                .unwrap()
                .windows(b"test-secret-not-plaintext".len())
                .any(|w| w == b"test-secret-not-plaintext")
        );
        drop(c);
        assert_eq!(
            load(&path).unwrap().unwrap().credentials.client_secret,
            "test-secret-not-plaintext"
        );
        let next_version = Service::default();
        let state = restore(&path, &next_version).unwrap();
        assert!(state.saved);
        assert!(!state.connected);
        assert_eq!(state.client_id, "12345");
        assert!(disconnect(&path, &next_version).unwrap().saved);
        assert!(load(&path).unwrap().is_some());
        assert!(!forget(&path, &next_version).unwrap().saved);
        assert!(load(&path).unwrap().is_none());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn damaged_encrypted_details_are_not_silently_replaced() {
        assert!(crypt(b"not a protected blob", false).is_err());
    }
}

/// Opens only the fixed osu! registration URL; no caller-supplied shell input.
#[cfg(windows)]
pub fn open_registration() -> Result<(), String> {
    use windows_sys::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};
    let url: Vec<u16> = "https://osu.ppy.sh/home/account/edit#new-oauth-application"
        .encode_utf16()
        .chain(Some(0))
        .collect();
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
        Err("Could not open the osu! account page in your browser".into())
    } else {
        Ok(())
    }
}
#[cfg(not(windows))]
pub fn open_registration() -> Result<(), String> {
    Err("Open https://osu.ppy.sh/home/account/edit#new-oauth-application in your browser".into())
}
