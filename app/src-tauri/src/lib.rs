//! Tauri shell around ganana-core: a background scanner that watches the AI
//! tools' log folders, provider account syncs with keys in the system
//! keyring, limit and credit alerts, a tray icon that keeps it running with
//! the window closed, and commands the dashboard calls.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc;
use std::time::Duration;

use chrono::Local;
use ganana_core::accounts::{self, Account, AccountsReport, HttpStatus, csv_import};
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
/// How often provider accounts are read again. Their data lands minutes
/// after a call, and the APIs ask for at most one poll a minute.
const ACCOUNT_SYNC: Duration = Duration::from_secs(15 * 60);
const KEYRING_SERVICE: &str = "ganana";
/// Largest CSV file accepted for import.
const CSV_LIMIT: usize = 50 << 20;
/// Must match `identifier` in tauri.conf.json: it names the data folder.
const IDENTIFIER: &str = "dev.ganana.app";
const BAR_PLUGIN: &str = "ganana.usage";
/// The Omarchy bar plugin, built into the binary so the app can install it.
const BAR_PLUGIN_FILES: &[(&str, &str)] = &[
    ("manifest.json", include_str!("../../../integrations/omarchy/ganana.usage/manifest.json")),
    ("Panel.qml", include_str!("../../../integrations/omarchy/ganana.usage/Panel.qml")),
    ("UsageContent.qml", include_str!("../../../integrations/omarchy/ganana.usage/UsageContent.qml")),
    ("UsageRing.qml", include_str!("../../../integrations/omarchy/ganana.usage/UsageRing.qml")),
];

struct App {
    db: PathBuf,
    roots: Option<Roots>,
    /// Where the Claude status line relay writes; see `claude_relay`.
    relay_dir: PathBuf,
    status: Mutex<Status>,
    /// One scan at a time: the watcher and the rescan button share it.
    scan_lock: Mutex<()>,
    /// One account sync at a time, for the same reason.
    sync_lock: Mutex<()>,
    /// Why the system keyring is unavailable, if it is.
    keyring_error: Option<String>,
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

    /// Tells the user when a window they were warned about has reset, if
    /// they asked for that. Resets happen by the clock, so this runs on a
    /// timer rather than after a scan.
    fn reset_notices(&self, app: &AppHandle) {
        let due = self.store().and_then(|store| {
            let now = chrono::Utc::now().timestamp_millis();
            let windows = limits::windows(&store.limits_since(now - 8 * 24 * 3_600_000)?, now);
            Ok(limits::new_resets(&store, &windows, now / 1000)?.into_iter().cloned().collect::<Vec<_>>())
        });
        for w in due.unwrap_or_default() {
            let _ = app
                .notification()
                .builder()
                .title(format!(
                    "{} {} window has reset",
                    limits::meter_name(&w.meter),
                    limits::window_name(w.window_minutes)
                ))
                .body("You can go on: the limit starts again from zero.")
                .show();
        }
    }

    /// Reads every API account again with its stored key, then tells the UI.
    fn sync_accounts(&self, app: &AppHandle) {
        let _one_at_a_time = self.sync_lock.lock().unwrap();
        let Ok(store) = self.store() else { return };
        for a in store.accounts().unwrap_or_default().iter().filter(|a| a.kind != accounts::CSV) {
            // A failed sync records its own error on the account.
            if let Err(e) = self.key(a.id).map(|key| accounts::sync(&store, a, &key, chrono::Utc::now())) {
                let _ = store.set_account_error(a.id, Some(&e));
            }
        }
        let _ = app.emit("accounts-updated", ());
        self.credit_alert(app, &store);
    }

    /// Warns once per top-up when prepaid credit runs low: under a tenth of
    /// what was bought, or under a dollar.
    fn credit_alert(&self, app: &AppHandle, store: &Store) {
        for a in store.accounts().unwrap_or_default() {
            let (Some(left), Some(total)) = (a.low_credit(), a.credit_total) else { continue };
            let setting = format!("credit-alert-{}", a.id);
            let total_text = format!("{total:.2}");
            if store.setting(&setting).ok().flatten().as_deref() == Some(total_text.as_str()) {
                continue;
            }
            let _ = store.set_setting(&setting, &total_text);
            let _ = app
                .notification()
                .builder()
                .title(format!("{} credit is low", a.name))
                .body(format!("${left:.2} left of ${total:.2}. Top up to keep it running."))
                .show();
        }
    }

    fn key_entry(&self, id: i64) -> CommandResult<keyring_core::Entry> {
        if let Some(e) = &self.keyring_error {
            return Err(format!("the system keyring is not available: {e}"));
        }
        keyring_core::Entry::new(KEYRING_SERVICE, &format!("account-{id}")).map_err(err)
    }

    fn key(&self, id: i64) -> CommandResult<String> {
        self.key_entry(id)?.get_password().map_err(|e| match e {
            keyring_core::Error::NoEntry => {
                "the key is missing from the keyring; remove the account and add it again".into()
            }
            e => err(e),
        })
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
    let (meter, window) = (limits::meter_name(&a.meter), limits::window_name(a.window_minutes));
    let title = if a.used_pct >= 100.0 {
        format!("{meter} {window} limit reached")
    } else {
        format!("{meter} {window} limit {:.0}% used", a.used_pct)
    };
    (title, format!("It resets at {}.", limits::clock(a.resets_at, Local::now())))
}

/// The path other programs (Claude Code, the bar) should run. Inside an
/// AppImage the binary's own path is a temporary mount, so the AppImage file
/// itself is used.
fn self_exe() -> CommandResult<PathBuf> {
    match std::env::var_os("APPIMAGE") {
        Some(p) => Ok(p.into()),
        None => std::env::current_exe().map_err(err),
    }
}

/// Uses the platform's secret store for account keys.
fn init_keyring() -> Result<(), String> {
    #[cfg(target_os = "linux")]
    keyring_core::set_default_store(zbus_secret_service_keyring_store::Store::new().map_err(err)?);
    #[cfg(windows)]
    keyring_core::set_default_store(windows_native_keyring_store::Store::new().map_err(err)?);
    #[cfg(not(any(target_os = "linux", windows)))]
    return Err("no supported keyring on this system".into());
    #[allow(unreachable_code)]
    Ok(())
}

/// Checks once a minute for windows that reset.
fn reset_loop(app: AppHandle) {
    loop {
        std::thread::sleep(Duration::from_secs(60));
        app.state::<App>().reset_notices(&app);
    }
}

/// Syncs the provider accounts now and then every `ACCOUNT_SYNC`.
fn sync_loop(app: AppHandle) {
    loop {
        app.state::<App>().sync_accounts(&app);
        std::thread::sleep(ACCOUNT_SYNC);
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

#[tauri::command]
fn set_reset_alerts(on: bool, state: State<'_, App>) -> CommandResult<()> {
    state.store().and_then(|s| s.set_setting(limits::RESET_ALERTS, if on { "on" } else { "off" })).map_err(err)
}

#[tauri::command]
async fn accounts_report(days: u32, state: State<'_, App>) -> CommandResult<AccountsReport> {
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let store = Store::open(&db)?;
        accounts::report(&store, &Catalog::load(&store)?, days, chrono::Utc::now())
    })
    .await
    .map_err(err)?
    .map_err(err)
}

/// Connects an API account: keeps the key in the keyring and syncs once. A
/// refused key is not kept; any other failure keeps the account with its
/// error, to retry later.
#[tauri::command]
async fn add_account(kind: String, name: String, key: String, app: AppHandle) -> CommandResult<Account> {
    let Some(&(_, provider)) = accounts::API_KINDS.iter().find(|(k, _)| *k == kind) else {
        return Err(format!("unknown account kind {kind}"));
    };
    let key = key.trim().to_string();
    if key.len() < 8 || key.chars().any(char::is_whitespace) {
        return Err("paste the whole key, with no spaces".into());
    }
    let name = if name.trim().is_empty() { kind.clone() } else { name.trim().to_string() };
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<App>();
        let store = state.store().map_err(err)?;
        let hint: String = key.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
        let id = store.add_account(&kind, provider, &name, Some(&hint)).map_err(err)?;
        let forget = |store: &Store| {
            let _ = state.key_entry(id).map(|e| e.delete_credential());
            let _ = store.delete_account(id);
        };
        if let Err(e) = state.key_entry(id).and_then(|e| e.set_password(&key).map_err(err)) {
            forget(&store);
            return Err(e);
        }
        let account = store.account(id).map_err(err)?.ok_or("the account vanished")?;
        let _one_at_a_time = state.sync_lock.lock().unwrap();
        if let Err(e) = accounts::sync(&store, &account, &key, chrono::Utc::now())
            && e.downcast_ref::<HttpStatus>().is_some_and(|s| matches!(s.status, 401 | 403))
        {
            forget(&store);
            return Err(err(e));
        }
        let _ = app.emit("accounts-updated", ());
        store.account(id).map_err(err)?.ok_or_else(|| "the account vanished".into())
    })
    .await
    .map_err(err)?
}

/// Removes an account, its key and everything read for it.
#[tauri::command]
fn remove_account(id: i64, app: AppHandle, state: State<'_, App>) -> CommandResult<()> {
    let store = state.store().map_err(err)?;
    let account = store.account(id).map_err(err)?.ok_or("no such account")?;
    if account.kind != accounts::CSV {
        match state.key_entry(id)?.delete_credential() {
            Ok(()) | Err(keyring_core::Error::NoEntry) => {}
            Err(e) => return Err(format!("could not remove the key from the keyring: {}", err(e))),
        }
    }
    store.delete_account(id).map_err(err)?;
    let _ = app.emit("accounts-updated", ());
    Ok(())
}

#[tauri::command]
async fn sync_now(app: AppHandle) {
    let _ = tauri::async_runtime::spawn_blocking(move || app.state::<App>().sync_accounts(&app)).await;
}

#[tauri::command]
fn csv_inspect(text: String) -> CommandResult<csv_import::Inspect> {
    if text.len() > CSV_LIMIT {
        return Err("the file is over 50 MB".into());
    }
    csv_import::inspect(&text).map_err(err)
}

/// Imports a CSV file into an existing CSV account (`account`), or into a
/// new one named `name` for `provider`.
#[tauri::command]
fn csv_import(
    account: Option<i64>,
    name: String,
    provider: String,
    text: String,
    mapping: csv_import::Mapping,
    app: AppHandle,
    state: State<'_, App>,
) -> CommandResult<Account> {
    if text.len() > CSV_LIMIT {
        return Err("the file is over 50 MB".into());
    }
    let batch = csv_import::parse(&text, &mapping).map_err(err)?;
    let store = state.store().map_err(err)?;
    let id = match account {
        Some(id) => {
            let a = store.account(id).map_err(err)?.ok_or("no such account")?;
            if a.kind != accounts::CSV {
                return Err("only imported accounts take files".into());
            }
            id
        }
        None => {
            let provider = provider.trim().to_lowercase();
            if provider.is_empty()
                || provider.len() > 32
                || !provider.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
            {
                return Err("choose who bills this usage".into());
            }
            if name.trim().is_empty() {
                return Err("give the account a name".into());
            }
            store.add_account(accounts::CSV, &provider, name.trim(), None).map_err(err)?
        }
    };
    store.save_account_batch(id, &batch, chrono::Utc::now().timestamp_millis()).map_err(err)?;
    let _ = app.emit("accounts-updated", ());
    store.account(id).map_err(err)?.ok_or_else(|| "the account vanished".into())
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
    let exe = self_exe()?;
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

/// The app's data folder, the same one Tauri gives the app, found without
/// starting Tauri.
fn data_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join(IDENTIFIER))
}

/// `ganana bar [--format json|plain|waybar]`: prints a summary for status
/// bars and returns the exit code. It only reads; the app keeps the data
/// current.
pub fn bar(args: Vec<std::ffi::OsString>) -> i32 {
    let format = match args.iter().position(|a| a == "--format").map(|i| args.get(i + 1)) {
        None => "json".to_string(),
        Some(Some(f)) => f.to_string_lossy().into_owned(),
        Some(None) => String::new(),
    };
    if !["json", "plain", "waybar"].contains(&format.as_str()) {
        eprintln!("usage: ganana bar [--format json|plain|waybar]");
        return 2;
    }
    let Some(db) = data_dir().map(|d| d.join("ganana.db")).filter(|p| p.exists()) else {
        eprintln!("Gaṇanā has not run yet: open the app once so it can read your usage.");
        return 1;
    };
    let summary =
        Store::open(&db).and_then(|store| ganana_core::bar::bar(&store, &Catalog::load(&store)?, Local::now()));
    match summary {
        Ok(b) => {
            match format.as_str() {
                "plain" => println!("{}", b.text),
                "waybar" => println!("{}", ganana_core::bar::waybar(&b)),
                _ => println!("{}", serde_json::to_string(&b).unwrap_or_default()),
            }
            0
        }
        Err(e) => {
            eprintln!("Could not read Gaṇanā's data: {}", err(e));
            1
        }
    }
}

#[derive(Serialize)]
struct BarWidget {
    /// This desktop has the Omarchy shell.
    available: bool,
    installed: bool,
}

/// Where Omarchy keeps user plugins, if this desktop runs Omarchy.
fn omarchy_plugins() -> Option<PathBuf> {
    let on_path =
        std::env::var_os("PATH").is_some_and(|path| std::env::split_paths(&path).any(|d| d.join("omarchy").is_file()));
    let dir = dirs::config_dir()?.join("omarchy");
    (on_path && dir.is_dir()).then(|| dir.join("plugins"))
}

fn omarchy(args: &[&str]) -> CommandResult<()> {
    let out = std::process::Command::new("omarchy").args(args).output().map_err(err)?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("omarchy {} failed: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim()))
    }
}

#[tauri::command]
fn bar_widget_status() -> BarWidget {
    let plugins = omarchy_plugins();
    BarWidget {
        available: plugins.is_some(),
        installed: plugins.is_some_and(|p| p.join(BAR_PLUGIN).join("manifest.json").is_file()),
    }
}

/// Puts the Gaṇanā widget on the Omarchy bar, pointed at this program.
#[tauri::command]
fn bar_widget_install() -> CommandResult<BarWidget> {
    let dir = omarchy_plugins().ok_or("this desktop does not run the Omarchy shell")?.join(BAR_PLUGIN);
    std::fs::create_dir_all(&dir).map_err(err)?;
    for (name, text) in BAR_PLUGIN_FILES {
        std::fs::write(dir.join(name), text).map_err(err)?;
    }
    let exe = self_exe()?;
    omarchy(&["plugin", "enable", BAR_PLUGIN])?;
    omarchy(&["bar", "set", BAR_PLUGIN, "command", &exe.to_string_lossy()])?;
    Ok(bar_widget_status())
}

/// Takes the widget off the bar. Omarchy keeps a backup of the folder.
#[tauri::command]
fn bar_widget_remove() -> CommandResult<BarWidget> {
    omarchy(&["plugin", "remove", BAR_PLUGIN, "--yes"])?;
    Ok(bar_widget_status())
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
            let state = App {
                db,
                roots,
                relay_dir,
                status: Mutex::new(Status::default()),
                scan_lock: Mutex::new(()),
                sync_lock: Mutex::new(()),
                keyring_error: init_keyring().err(),
            };
            // Keep an installed relay pointing at this binary if the app moved.
            if let (Ok(settings), Ok(exe)) = (state.claude_settings(), self_exe())
                && matches!(claude_relay::status(&settings, &state.relay_dir), Ok(RelayStatus::On { .. }))
            {
                let _ = claude_relay::install(&settings, &state.relay_dir, &exe);
            }
            app.manage(state);
            tray(app)?;
            let handle = app.handle().clone();
            std::thread::spawn(move || watch(handle));
            let handle = app.handle().clone();
            std::thread::spawn(move || sync_loop(handle));
            let handle = app.handle().clone();
            std::thread::spawn(move || reset_loop(handle));
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
            set_reset_alerts,
            bar_widget_status,
            bar_widget_install,
            bar_widget_remove,
            accounts_report,
            add_account,
            remove_account,
            sync_now,
            csv_inspect,
            csv_import,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Touches the real system keyring, so it only runs on request:
    /// `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn keyring_round_trip() {
        init_keyring().unwrap();
        let entry = keyring_core::Entry::new(KEYRING_SERVICE, "account-test").unwrap();
        entry.set_password("sk-test-not-a-key").unwrap();
        assert_eq!(entry.get_password().unwrap(), "sk-test-not-a-key");
        entry.delete_credential().unwrap();
        assert!(matches!(entry.get_password(), Err(keyring_core::Error::NoEntry)));
    }
}
