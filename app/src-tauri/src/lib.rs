//! Tauri shell around ganana-core: a background scanner that watches the AI
//! tools' log folders, limit alerts, a tray icon that keeps it running with
//! the window closed, and commands the dashboard calls.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc;
use std::time::Duration;

use chrono::{Local, TimeZone};
use ganana_core::claude_relay::{self, RelayStatus};
use ganana_core::limits::{self, Alert};
use ganana_core::{Catalog, Limits, Overview, Plan, Roots, ScanReport, Store};
use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode};
use serde::Serialize;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tauri_plugin_notification::NotificationExt;

const PRICE_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const DEBOUNCE: Duration = Duration::from_secs(2);

struct App {
    db: PathBuf,
    roots: Option<Roots>,
    /// Where the Claude status line relay writes; see `claude_relay`.
    relay_dir: PathBuf,
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
        let changed = report.events > 0 || report.limits > 0;
        self.set_status(app, |s| {
            s.scanning = false;
            s.last_scan = Some(chrono::Utc::now().timestamp_millis());
            s.last_report = Some(report);
        });
        if changed {
            let _ = app.emit("usage-updated", ());
            self.alert(app);
        }
    }

    /// Shows a desktop notification for each limit that crossed a threshold.
    fn alert(&self, app: &AppHandle) {
        let due = self.store().and_then(|store| {
            let now = chrono::Utc::now().timestamp_millis();
            let windows = limits::windows(&store.limits_since(now - 8 * 24 * 3_600_000)?, now);
            limits::new_alerts(&store, &windows, &limits::thresholds(&store)?)
        });
        for a in due.unwrap_or_default() {
            let (title, body) = alert_text(&a);
            let _ = app.notification().builder().title(title).body(body).show();
        }
    }

    fn claude_settings(&self) -> CommandResult<PathBuf> {
        let roots = self.roots.as_ref().ok_or("no home folder")?;
        Ok(claude_relay::settings_path(&roots.home))
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

fn alert_text(a: &Alert) -> (String, String) {
    let meter = match a.meter.as_str() {
        "claude" => "Claude",
        "codex" => "ChatGPT (Codex)",
        other => other,
    };
    let window = match a.window_minutes {
        300 => "5-hour".to_string(),
        10080 => "weekly".to_string(),
        m if m % 60 == 0 => format!("{}-hour", m / 60),
        m => format!("{m}-minute"),
    };
    let title = if a.used_pct >= 100.0 {
        format!("{meter} {window} limit reached")
    } else {
        format!("{meter} {window} limit {:.0}% used", a.used_pct)
    };
    let resets = Local.timestamp_opt(a.resets_at, 0).single().map_or_else(String::new, |t| {
        let fmt = if t.date_naive() == Local::now().date_naive() { "%H:%M" } else { "%a %H:%M" };
        t.format(fmt).to_string()
    });
    (title, format!("It resets at {resets}."))
}

/// The path Claude Code should run. Inside an AppImage the binary's own path
/// is a temporary mount, so the AppImage file itself is used.
fn relay_exe() -> CommandResult<PathBuf> {
    match std::env::var_os("APPIMAGE") {
        Some(p) => Ok(p.into()),
        None => std::env::current_exe().map_err(err),
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
async fn limits(state: State<'_, App>) -> CommandResult<Limits> {
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let store = Store::open(&db)?;
        ganana_core::limits(&store, &Catalog::load(&store)?, chrono::Local::now())
    })
    .await
    .map_err(err)?
    .map_err(err)
}

/// Adds (id 0) or updates a plan; returns its id.
#[tauri::command]
fn save_plan(plan: Plan, state: State<'_, App>) -> CommandResult<i64> {
    if plan.name.trim().is_empty()
        || !(1..=31).contains(&plan.renews_day)
        || !plan.monthly_usd.is_finite()
        || plan.monthly_usd < 0.0
    {
        return Err("a plan needs a name, a price and a renewal day from 1 to 31".into());
    }
    state.store().and_then(|s| s.save_plan(&plan)).map_err(err)
}

#[tauri::command]
fn delete_plan(id: i64, state: State<'_, App>) -> CommandResult<()> {
    state.store().and_then(|s| s.delete_plan(id)).map_err(err)
}

#[tauri::command]
fn set_thresholds(thresholds: Vec<i64>, state: State<'_, App>) -> CommandResult<()> {
    state.store().and_then(|s| limits::set_thresholds(&s, &thresholds)).map_err(err)
}

#[derive(Serialize)]
struct Relay {
    #[serde(flatten)]
    status: RelayStatus,
    /// Claude Code's settings file, shown so the user knows what changes.
    settings: String,
}

fn relay(state: &App, change: impl FnOnce(&Path, &Path) -> anyhow::Result<()>) -> CommandResult<Relay> {
    let settings = state.claude_settings()?;
    change(&settings, &state.relay_dir).map_err(err)?;
    let status = claude_relay::status(&settings, &state.relay_dir).map_err(err)?;
    Ok(Relay { status, settings: settings.display().to_string() })
}

#[tauri::command]
fn relay_status(state: State<'_, App>) -> CommandResult<Relay> {
    relay(&state, |_, _| Ok(()))
}

/// Points Claude Code's status line at Gaṇanā (with a backup), so Claude
/// Pro/Max limits reach the dashboard.
#[tauri::command]
fn relay_install(state: State<'_, App>) -> CommandResult<Relay> {
    let exe = relay_exe()?;
    relay(&state, |settings, dir| claude_relay::install(settings, dir, &exe))
}

#[tauri::command]
fn relay_uninstall(state: State<'_, App>) -> CommandResult<Relay> {
    relay(&state, claude_relay::uninstall)
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

fn show_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Tray icon with Open and Quit: closing the window only hides it, so
/// scanning and alerts go on.
fn tray(app: &tauri::App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Gaṇanā", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("Gaṇanā")
        .menu(&Menu::with_items(app, &[&open, &quit])?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let tauri::tray::TrayIconEvent::Click { button: tauri::tray::MouseButton::Left, .. } = event {
                show_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        // A second launch shows the running app instead of starting another.
        .plugin(tauri_plugin_single_instance::init(|app, _, _| show_window(app)))
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let data = app.path().app_data_dir()?;
            let db = data.join("ganana.db");
            Store::open(&db).map_err(err)?;
            let relay_dir = data.join("relay");
            std::fs::create_dir_all(&relay_dir)?;
            let roots = Roots::from_env().map(|r| Roots { relay_dir: Some(relay_dir.clone()), ..r });
            let state = App { db, roots, relay_dir, status: Mutex::new(Status::default()), scan_lock: Mutex::new(()) };
            // Keep an installed relay pointing at this binary if the app moved.
            if let (Ok(settings), Ok(exe)) = (state.claude_settings(), relay_exe())
                && matches!(claude_relay::status(&settings, &state.relay_dir), Ok(RelayStatus::On { .. }))
            {
                let _ = claude_relay::install(&settings, &state.relay_dir, &exe);
            }
            app.manage(state);
            tray(app)?;
            let handle = app.handle().clone();
            std::thread::spawn(move || watch(handle));
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            overview,
            limits,
            save_plan,
            delete_plan,
            set_thresholds,
            relay_status,
            relay_install,
            relay_uninstall,
            status,
            rescan,
            price_models,
            aliases,
            set_alias
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
