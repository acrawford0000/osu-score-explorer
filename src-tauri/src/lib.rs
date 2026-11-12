pub mod lazer;
pub mod library;

#[cfg(feature = "desktop")]
mod desktop {
    use crate::library::{self, Library};
    use std::{
        path::PathBuf,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
    };
    use tauri::{Manager, State};
    struct Store {
        path: PathBuf,
        importing: Arc<AtomicBool>,
    }
    struct ImportGuard(Arc<AtomicBool>);
    impl Drop for ImportGuard {
        fn drop(&mut self) {
            self.0.store(false, Ordering::Release);
        }
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
                app.manage(Store {
                    path: directory.join("library.sqlite"),
                    importing: Arc::new(AtomicBool::new(false)),
                });
                Ok(())
            })
            .invoke_handler(tauri::generate_handler![
                load_library,
                import_stable,
                import_lazer,
                artwork
            ])
            .run(tauri::generate_context!())
            .expect("Could not start Score Explorer");
    }
}
#[cfg(feature = "desktop")]
pub use desktop::run;
