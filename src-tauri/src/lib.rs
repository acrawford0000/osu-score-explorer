pub mod audio;
pub mod downloads;
pub mod lazer;
pub mod library;
pub mod maps;
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
        downloads: Arc<crate::downloads::Controller>,
    }
    struct ImportGuard(Arc<AtomicBool>);
    impl Drop for ImportGuard {
        fn drop(&mut self) {
            self.0.store(false, Ordering::Release);
        }
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
        state: State<'_, Store>,
    ) -> Result<String, String> {
        let path = state.path.clone();
        tauri::async_runtime::spawn_blocking(move || {
            crate::replays::export(&path, &id, &PathBuf::from(directory))
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
                let downloads = Arc::new(crate::downloads::Controller::default());
                downloads.restore(&directory.join("library.sqlite"));
                app.manage(Store {
                    path: directory.join("library.sqlite"),
                    importing: Arc::new(AtomicBool::new(false)),
                    replay_lock: Arc::new(Mutex::new(())),
                    downloads,
                });
                Ok(())
            })
            .invoke_handler(tauri::generate_handler![
                load_library,
                import_stable,
                import_lazer,
                artwork,
                replay_availability,
                export_replay,
                save_sources,
                audio_track,
                start_downloads,
                download_status,
                pause_downloads
            ])
            .run(tauri::generate_context!())
            .expect("Could not start Score Explorer");
    }
}
#[cfg(feature = "desktop")]
pub use desktop::run;
