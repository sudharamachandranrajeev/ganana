//! Tauri shell around ganana-core: a background scanner that watches the AI
//! tools' log folders, and commands the dashboard calls.

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::mpsc;
use std::time::Duration;

use ganana_core::{Catalog, Overview, Roots, ScanReport, Store};
use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

const PRICE_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const DEBOUNCE: Duration = Duration::from_secs(2);

struct App {
    db: PathBuf,
    roots: Option<Roots>,
    status: Mutex<Status>,
    /// One scan at a time: the watcher and the rescan button share it.
    scan_lock: Mutex<()>,
}

#[derive(Debug, Default, Clone, Serialize)]
struct Status {
    scanning: bool,
    /// Unix ms of the last finished scan.
    last_scan: Option<i64>,
    last_report: Option<ScanReport>,
    prices_fetched_at: Option<i64>,
    price_error: Option<String>,
    watched: Vec<String>,
    watch_error: Option<String>,
}

type CommandResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    format!("{e:#}")
}

impl App {
    fn store(&self) -> anyhow::Result<Store> {
        Store::open(&self.db)
    }

    fn set_status(&self, app: &AppHandle, edit: impl FnOnce(&mut Status)) {
        let status = {
            let mut s = self.status.lock().unwrap();
            edit(&mut s);
            s.clone()
        };
        let _ = app.emit("status", status);
    }

    /// Reads new log lines from every source, then tells the UI.
    fn scan(&self, app: &AppHandle) {
        let Some(roots) = &self.roots else { return };
        let _one_at_a_time = self.scan_lock.lock().unwrap();
        self.set_status(app, |s| s.scanning = true);
        let report = match self.store() {
            Ok(store) => ganana_core::scan_all(&store, roots),
            Err(e) => ScanReport { errors: vec![err(e)], ..ScanReport::default() },
        };
        let changed = report.events > 0;
        self.set_status(app, |s| {
            s.scanning = false;
            s.last_scan = Some(chrono::Utc::now().timestamp_millis());
            s.last_report = Some(report);
        });
        if changed {
            let _ = app.emit("usage-updated", ());
        }
    }

    fn refresh_prices(&self, app: &AppHandle) {
        let result = self.store().and_then(|s| Catalog::refresh_if_stale(&s, PRICE_MAX_AGE));
        let fetched = self.store().ok().and_then(|s| s.catalog_json().ok().flatten()).map(|(at, _)| at);
        self.set_status(app, |s| {
            s.prices_fetched_at = fetched;
            s.price_error = result.as_ref().err().map(err);
        });
        if matches!(result, Ok(true)) {
            let _ = app.emit("usage-updated", ());
        }
    }
}

/// First scan, price refresh, then a rescan whenever a watched folder changes.
fn watch(app: AppHandle) {
    let state = app.state::<App>();
    state.scan(&app);
    state.refresh_prices(&app);

    let Some(roots) = &state.roots else { return };
    let (tx, rx) = mpsc::channel();
    let mut debouncer = match new_debouncer(DEBOUNCE, tx) {
        Ok(d) => d,
        Err(e) => {
            state.set_status(&app, |s| s.watch_error = Some(err(e)));
            return;
        }
    };
    let mut watched = Vec::new();
    for dir in roots.watch_dirs() {
        if debouncer.watcher().watch(&dir, RecursiveMode::Recursive).is_ok() {
            watched.push(dir.display().to_string());
        }
    }
    state.set_status(&app, |s| s.watched = watched);
    for result in rx {
        if result.is_ok() {
            state.scan(&app);
        }
    }
}

#[tauri::command]
async fn overview(days: u32, state: State<'_, App>) -> CommandResult<Overview> {
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let store = Store::open(&db)?;
        let catalog = Catalog::load(&store)?;
        ganana_core::overview(&store, &catalog, days, chrono::Local::now())
    })
    .await
    .map_err(err)?
    .map_err(err)
}

#[tauri::command]
fn status(state: State<'_, App>) -> Status {
    state.status.lock().unwrap().clone()
}

#[tauri::command]
async fn rescan(app: AppHandle) {
    let _ = tauri::async_runtime::spawn_blocking(move || app.state::<App>().scan(&app)).await;
}

/// Every priced model as "provider/model", for mapping an unpriced name.
#[tauri::command]
fn price_models(state: State<'_, App>) -> CommandResult<Vec<String>> {
    let store = Store::open(&state.db).map_err(err)?;
    Ok(Catalog::load(&store).map_err(err)?.model_ids())
}

/// Model name -> "provider/model" mappings the user has set.
#[tauri::command]
fn aliases(state: State<'_, App>) -> CommandResult<std::collections::HashMap<String, String>> {
    Store::open(&state.db).and_then(|s| s.aliases()).map_err(err)
}

/// Prices `alias` as `target` ("provider/model"); `None` removes the mapping.
#[tauri::command]
fn set_alias(alias: String, target: Option<String>, app: AppHandle, state: State<'_, App>) -> CommandResult<()> {
    let store = Store::open(&state.db).map_err(err)?;
    if let Some(t) = &target
        && !Catalog::load(&store).map_err(err)?.model_ids().contains(t)
    {
        return Err(format!("unknown model {t}"));
    }
    store.set_alias(&alias, target.as_deref()).map_err(err)?;
    let _ = app.emit("usage-updated", ());
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let db = app.path().app_data_dir()?.join("ganana.db");
            Store::open(&db).map_err(err)?;
            app.manage(App {
                db,
                roots: Roots::from_env(),
                status: Mutex::new(Status::default()),
                scan_lock: Mutex::new(()),
            });
            let handle = app.handle().clone();
            std::thread::spawn(move || watch(handle));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![overview, status, rescan, price_models, aliases, set_alias])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
