pub mod audio;
pub mod calculators;
pub mod downloads;
pub mod lazer;
pub mod library;
pub mod maps;
pub mod online_replays;
pub mod osu_api;
pub mod player_profiles;
pub mod players;
pub mod recovery;
pub mod replays;
pub mod sources;

#[cfg(feature = "desktop")]
mod desktop {
    use crate::library::{self, Library};
    use std::{
        path::PathBuf,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, Ordering},
        },
    };
    use tauri::{Manager, State};
    struct Store {
        path: PathBuf,
        importing: Arc<AtomicBool>,
        replay_lock: Arc<Mutex<()>>,
        online_replays: Arc<crate::online_replays::Service>,
        downloads: Arc<crate::downloads::Controller>,
        calculators: Arc<crate::calculators::Controller>,
    }
    struct ImportGuard(Arc<AtomicBool>);
    impl Drop for ImportGuard {
        fn drop(&mut self) {
            self.0.store(false, Ordering::Release);
        }
    }

    #[tauri::command]
    async fn refresh_player_identity(
        id: u64,
        state: State<'_, Store>,
    ) -> Result<crate::player_profiles::Identity, String> {
        let path = state.path.clone();
        tauri::async_runtime::spawn_blocking(move || crate::player_profiles::refresh(&path, id))
            .await
            .map_err(|e| e.to_string())?
    }
    #[tauri::command]
    async fn save_player_account(
        account: crate::players::Account,
        state: State<'_, Store>,
    ) -> Result<Library, String> {
        let path = state.path.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let c = library::db(&path)?;
            crate::players::save(&c, account)?;
            library::load(&path)
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[tauri::command]
    fn calculation_status(state: State<'_, Store>) -> crate::calculators::Status {
        state.calculators.status()
    }
    #[tauri::command]
    fn start_calculations(state: State<'_, Store>) {
        state.calculators.start(state.path.clone());
    }
    #[tauri::command]
    async fn select_calculator(
        calculator: String,
        state: State<'_, Store>,
    ) -> Result<Library, String> {
        if calculator != "lazer" && calculator != "rosu" {
            return Err("Unknown calculator".into());
        }
        let path = state.path.clone();
        let selected = calculator.clone();
        let result = tauri::async_runtime::spawn_blocking(move || {
            let c = library::db(&path)?;
            c.execute(
                "INSERT OR REPLACE INTO metadata VALUES('calculator',?1)",
                [selected],
            )
            .map_err(|e| e.to_string())?;
            library::load(&path)
        })
        .await
        .map_err(|e| e.to_string())?;
        if calculator == "lazer" {
            state.calculators.start(state.path.clone());
        } else {
            state.calculators.stop();
        }
        result
    }
    #[tauri::command]
    fn download_status(state: State<'_, Store>) -> crate::downloads::Status {
        state.downloads.status()
    }
    #[tauri::command]
    fn pause_downloads(state: State<'_, Store>) {
        state.downloads.pause();
    }
    #[tauri::command]
    fn start_downloads(
        options: crate::downloads::Options,
        state: State<'_, Store>,
    ) -> Result<(), String> {
        if !std::path::Path::new(&options.songs).is_absolute()
            || !std::path::Path::new(&options.songs).is_dir()
        {
            return Err("Choose an existing Songs folder first".into());
        }
        state
            .importing
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "Pause the download queue or wait for the current import to finish")?;
        let guard = ImportGuard(state.importing.clone());
        state
            .downloads
            .running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "The download queue is already running")?;
        state.downloads.pause.store(false, Ordering::Release);
        let ctl = state.downloads.clone();
        let path = state.path.clone();
        std::thread::spawn(move || {
            let _guard = guard;
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::downloads::run(&path, options, ctl.clone())
            }))
            .unwrap_or_else(|_| Err("The download worker stopped. Resume to retry.".into()));
            crate::downloads::finish(&ctl, result);
        });
        Ok(())
    }
    #[tauri::command]
    async fn audio_track(
        id: String,
        app: tauri::AppHandle,
        state: State<'_, Store>,
    ) -> Result<Option<String>, String> {
        let path = state.path.clone();
        let file = tauri::async_runtime::spawn_blocking(move || crate::audio::track(&path, &id))
            .await
            .map_err(|e| e.to_string())??;
        if let Some(file) = file {
            app.asset_protocol_scope()
                .allow_file(&file)
                .map_err(|e| e.to_string())?;
            Ok(Some(file.to_string_lossy().to_string()))
        } else {
            Ok(None)
        }
    }
    #[tauri::command]
    async fn save_sources(
        sources: Vec<crate::sources::GameSource>,
        state: State<'_, Store>,
    ) -> Result<Vec<crate::sources::GameSource>, String> {
        let path = state.path.clone();
        tauri::async_runtime::spawn_blocking(move || {
            crate::sources::save(&library::db(&path)?, sources)
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[tauri::command]
    async fn connect_online_replays(
        credentials: crate::online_replays::Credentials,
        state: State<'_, Store>,
    ) -> Result<(), String> {
        let service = state.online_replays.clone();
        tauri::async_runtime::spawn_blocking(move || service.connect(credentials))
            .await
            .map_err(|e| e.to_string())?
    }
    #[tauri::command]
    async fn disconnect_online_replays(state: State<'_, Store>) -> Result<(), String> {
        let service = state.online_replays.clone();
        tauri::async_runtime::spawn_blocking(move || service.disconnect())
            .await
            .map_err(|e| e.to_string())
    }
    #[tauri::command]
    async fn check_online_replay(
        id: String,
        state: State<'_, Store>,
    ) -> Result<crate::online_replays::Status, String> {
        let path = state.path.clone();
        let service = state.online_replays.clone();
        tauri::async_runtime::spawn_blocking(move || service.check(&path, &id))
            .await
            .map_err(|e| e.to_string())?
    }
    #[tauri::command]
    async fn replay_availability(
        ids: Vec<String>,
        state: State<'_, Store>,
    ) -> Result<crate::replays::Availability, String> {
        let path = state.path.clone();
        let lock = state.replay_lock.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let _guard = lock.lock().map_err(|e| e.to_string())?;
            let warning = crate::replays::refresh_lazer(&path).err();
            crate::replays::availability(&path, &ids, warning)
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[tauri::command]
    async fn export_replay(
        id: String,
        directory: String,
        online: Option<bool>,
        state: State<'_, Store>,
    ) -> Result<String, String> {
        let path = state.path.clone();
        let service = state.online_replays.clone();
        tauri::async_runtime::spawn_blocking(move || {
            if online.unwrap_or(false) {
                service.export(&path, &id, &PathBuf::from(directory))
            } else {
                crate::replays::export(&path, &id, &PathBuf::from(directory))
            }
            .map(|p| p.to_string_lossy().to_string())
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[tauri::command]
    async fn load_library(state: State<'_, Store>) -> Result<Library, String> {
        let path = state.path.clone();
        tauri::async_runtime::spawn_blocking(move || library::load(&path))
            .await
            .map_err(|e| e.to_string())?
    }
    #[tauri::command]
    async fn import_stable(
        root: String,
        songs: Option<String>,
        force: Option<bool>,
        state: State<'_, Store>,
    ) -> Result<Library, String> {
        state
            .importing
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "An import is already running".to_string())?;
        let guard = ImportGuard(state.importing.clone());
        let path = state.path.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let _guard = guard;
            library::import_scan(
                &path,
                &PathBuf::from(root),
                songs.map(PathBuf::from).as_deref(),
                force.unwrap_or(false),
            )
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[tauri::command]
    async fn artwork(id: String, state: State<'_, Store>) -> Result<Option<String>, String> {
        let path = state.path.clone();
        tauri::async_runtime::spawn_blocking(move || library::artwork(&path, &id))
            .await
            .map_err(|e| e.to_string())?
    }
    #[tauri::command]
    async fn import_lazer(
        root: String,
        force: Option<bool>,
        state: State<'_, Store>,
    ) -> Result<Library, String> {
        state
            .importing
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "An import is already running".to_string())?;
        let guard = ImportGuard(state.importing.clone());
        let path = state.path.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let _guard = guard;
            crate::lazer::import(&path, &PathBuf::from(root), force.unwrap_or(false))
        })
        .await
        .map_err(|e| e.to_string())?
    }
    pub fn run() {
        tauri::Builder::default()
            .plugin(tauri_plugin_dialog::init())
            .setup(|app| {
                let directory = app.path().app_local_data_dir()?;
                std::fs::create_dir_all(&directory)?;
                let c = library::db(&directory.join("library.sqlite"))?;
                c.execute(
                    "INSERT OR IGNORE INTO metadata VALUES('calculator','lazer')",
                    [],
                )?;
                drop(c);
                let downloads = Arc::new(crate::downloads::Controller::default());
                downloads.restore(&directory.join("library.sqlite"));
                app.manage(Store {
                    path: directory.join("library.sqlite"),
                    importing: Arc::new(AtomicBool::new(false)),
                    replay_lock: Arc::new(Mutex::new(())),
                    online_replays: Arc::new(crate::online_replays::Service::default()),
                    downloads,
                    calculators: Arc::new(crate::calculators::Controller::default()),
                });
                Ok(())
            })
            .invoke_handler(tauri::generate_handler![
                load_library,
                import_stable,
                import_lazer,
                artwork,
                replay_availability,
                connect_online_replays,
                disconnect_online_replays,
                check_online_replay,
                export_replay,
                save_sources,
                audio_track,
                start_downloads,
                download_status,
                pause_downloads,
                calculation_status,
                start_calculations,
                select_calculator,
                save_player_account,
                refresh_player_identity
            ])
            .run(tauri::generate_context!())
            .expect("Could not start Score Explorer");
    }
}
#[cfg(feature = "desktop")]
pub use desktop::run;
