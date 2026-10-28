//! Plan limits: Codex readings, the Claude status line relay and its
//! install, window maths, alerts and plan value. Synthetic data only.

use std::fs;
use std::path::PathBuf;

use chrono::{Local, NaiveDate, TimeZone};
use ganana_core::claude_relay::{self, RelayStatus};
use ganana_core::limits::{self, Window, billing_period, new_alerts, windows};
use ganana_core::{Catalog, LimitSnapshot, Plan, Roots, Store, Tokens, UsageEvent, scan_all};
use serde_json::{Value, json};
use tempfile::TempDir;

struct Home {
    dir: TempDir,
    store: Store,
}

impl Home {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let store = Store::open(&dir.path().join("ganana.db")).unwrap();
        Self { dir, store }
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.dir.path().join(rel)
    }

    fn relay_dir(&self) -> PathBuf {
        self.path("relay")
    }

    fn scan(&self) {
        let roots = Roots { home: self.path("home"), relay_dir: Some(self.relay_dir()) };
        let report = scan_all(&self.store, &roots);
        assert!(report.errors.is_empty(), "{:?}", report.errors);
    }

    fn limits(&self) -> Vec<LimitSnapshot> {
        self.store.limits_since(0).unwrap()
    }
}

fn codex_count(ts: &str, primary: f64, secondary: f64) -> String {
    format!(
        r#"{{"timestamp":"{ts}","type":"event_msg","payload":{{"type":"token_count","info":null,"rate_limits":{{"limit_id":"codex","primary":{{"used_percent":{primary},"window_minutes":300,"resets_at":1800000000}},"secondary":{{"used_percent":{secondary},"window_minutes":10080,"resets_at":1800500000}}}}}}}}"#
    ) + "\n"
}

#[test]
fn codex_limits_keep_changes_only() {
    let h = Home::new();
    let premium = r#"{"timestamp":"2026-10-02T10:00:03Z","type":"event_msg","payload":{"type":"token_count","info":null,"rate_limits":{"limit_id":"premium","primary":null,"secondary":null}}}"#;
    let log = codex_count("2026-10-02T10:00:00Z", 10.0, 3.0)
        + &codex_count("2026-10-02T10:00:01Z", 10.0, 3.0)
        + &codex_count("2026-10-02T10:00:02Z", 12.0, 3.0)
        + premium
        + "\n";
    let p = h.path("home/.codex/sessions/2026/10/02/rollout-a.jsonl");
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, log).unwrap();
    h.scan();
    h.scan();

    let l = h.limits();
    assert_eq!(l.len(), 3, "{l:?}");
    let first = chrono::DateTime::parse_from_rfc3339("2026-10-02T10:00:00Z").unwrap().timestamp_millis();
    let weekly = l.iter().find(|s| s.window_minutes == 10080).unwrap();
    assert_eq!((weekly.meter.as_str(), weekly.used_pct, weekly.seen_at), ("codex", 3.0, first));
    assert!(l.iter().any(|s| s.window_minutes == 300 && s.used_pct == 12.0));
    assert!(h.store.events_since(0).unwrap().is_empty(), "limit-only lines are not usage");
}

fn status_json(five: Option<f64>) -> String {
    let mut v = json!({ "model": { "display_name": "Opus" }, "session_id": "s", "transcript_path": "/secret/path" });
    if let Some(p) = five {
        v["rate_limits"] = json!({
            "five_hour": { "used_percentage": p, "resets_at": 1800000000 },
            "seven_day": { "used_percentage": 41.0, "resets_at": 1800500000 }
        });
    }
    v.to_string()
}

#[test]
fn relay_records_changes_and_the_scan_reads_them() {
    let h = Home::new();
    let dir = h.relay_dir();
    let file = dir.join(claude_relay::LIMITS_FILE);

    assert_eq!(claude_relay::relay(&status_json(None), &dir, 1), "Opus");
    assert!(!file.exists(), "nothing to record without rate_limits");
    assert_eq!(claude_relay::relay(&status_json(Some(23.5)), &dir, 2), "Opus · 5h 24% · week 41%");
    claude_relay::relay(&status_json(Some(23.5)), &dir, 3);
    claude_relay::relay(&status_json(Some(30.0)), &dir, 4);

    let text = fs::read_to_string(&file).unwrap();
    assert_eq!(text.lines().count(), 2);
    assert!(!text.contains("secret") && !text.contains("Opus"), "only limits are kept");

    h.scan();
    let l = h.limits();
    assert_eq!(l.len(), 3);
    assert!(l.iter().all(|s| s.meter == "claude" && s.provider == "anthropic"));
    assert!(l.iter().any(|s| s.window_minutes == 300 && s.used_pct == 30.0 && s.seen_at == 4));
}

#[test]
fn install_keeps_and_undo_restores_the_users_status_line() {
    let h = Home::new();
    let dir = h.relay_dir();
    let settings = h.path("claude/settings.json");
    fs::create_dir_all(settings.parent().unwrap()).unwrap();
    let original = "{\n  \"model\": \"opus\",\n  \"statusLine\": {\n    \"type\": \"command\",\n    \"command\": \"echo mine\",\n    \"padding\": 0\n  },\n  \"theme\": \"dark\"\n}\n";
    fs::write(&settings, original).unwrap();

    let exe = PathBuf::from("/opt/it's here/ganana");
    claude_relay::install(&settings, &dir, &exe).unwrap();
    claude_relay::install(&settings, &dir, &exe).unwrap();
    let text = fs::read_to_string(&settings).unwrap();
    let v: Value = serde_json::from_str(&text).unwrap();
    let command = v["statusLine"]["command"].as_str().unwrap();
    assert_eq!(command, format!(r"'/opt/it'\''s here/ganana' {} '{}'", claude_relay::FLAG, dir.display()));
    assert_eq!(v["statusLine"]["padding"], 0);
    assert!(text.find("\"model\"").unwrap() < text.find("\"theme\"").unwrap(), "key order kept");
    assert_eq!(
        fs::read_dir(&dir)
            .unwrap()
            .filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().starts_with("settings-backup"))
            .count(),
        1
    );
    assert_eq!(claude_relay::status(&settings, &dir).unwrap(), RelayStatus::On { wraps: Some("echo mine".into()) });
    if cfg!(unix) {
        assert_eq!(claude_relay::relay(&status_json(Some(5.0)), &dir, 1), "mine");
    }

    let moved = PathBuf::from("/new/ganana");
    claude_relay::install(&settings, &dir, &moved).unwrap();
    let v: Value = serde_json::from_str(&fs::read_to_string(&settings).unwrap()).unwrap();
    assert!(v["statusLine"]["command"].as_str().unwrap().starts_with("'/new/ganana' "), "moved app is repaired");

    claude_relay::uninstall(&settings, &dir).unwrap();
    assert_eq!(fs::read_to_string(&settings).unwrap(), original);
    assert_eq!(claude_relay::status(&settings, &dir).unwrap(), RelayStatus::Off { existing: Some("echo mine".into()) });
}

#[test]
fn install_without_settings_and_refusal_of_bad_settings() {
    let h = Home::new();
    let dir = h.relay_dir();
    let settings = h.path("claude/settings.json");
    claude_relay::install(&settings, &dir, &PathBuf::from("/bin/ganana")).unwrap();
    assert!(matches!(claude_relay::status(&settings, &dir).unwrap(), RelayStatus::On { wraps: None }));
    claude_relay::uninstall(&settings, &dir).unwrap();
    assert_eq!(fs::read_to_string(&settings).unwrap(), "{}\n");

    fs::write(&settings, "[1, 2]").unwrap();
    assert!(claude_relay::install(&settings, &dir, &PathBuf::from("/bin/ganana")).is_err());
    assert_eq!(fs::read_to_string(&settings).unwrap(), "[1, 2]");
}

fn snap(window_minutes: i64, resets_at: i64, used_pct: f64, seen_at: i64) -> LimitSnapshot {
    LimitSnapshot { meter: "claude".into(), provider: "anthropic".into(), window_minutes, resets_at, used_pct, seen_at }
}

const NOW_S: i64 = 1_800_000_000;
const NOW_MS: i64 = NOW_S * 1000;
const MIN_MS: i64 = 60_000;

#[test]
fn window_rate_and_full_time() {
    let resets = NOW_S + 3 * 3600;
    let w = &windows(
        &[
            snap(300, NOW_S - 10 * 3600, 90.0, NOW_MS - 12 * 60 * MIN_MS),
            snap(300, resets, 40.0, NOW_MS - 60 * MIN_MS),
            snap(300, resets + 30, 50.0, NOW_MS - 30 * MIN_MS),
            snap(300, resets - 20, 60.0, NOW_MS - MIN_MS),
        ],
        NOW_MS,
    )[0];
    assert_eq!(w.used_pct, 60.0);
    assert_eq!(w.history.len(), 3, "the earlier window is not part of this one");
    assert_eq!(w.rate_per_hour, Some(20.0));
    assert_eq!(w.full_at, Some(NOW_S + 2 * 3600));

    let slow =
        &windows(&[snap(300, NOW_S + 3600, 40.0, NOW_MS - 60 * MIN_MS), snap(300, NOW_S + 3600, 50.0, NOW_MS)], NOW_MS)
            [0];
    assert_eq!(slow.full_at, None, "at 10%/h it resets before it fills");

    let old = &windows(&[snap(300, NOW_S - 60, 80.0, NOW_MS - 120 * MIN_MS)], NOW_MS)[0];
    assert!(old.reset_passed);
    assert_eq!((old.used_pct, old.rate_per_hour, old.history.len()), (0.0, None, 0));
}

fn window(used_pct: f64, resets_at: i64) -> Window {
    windows(&[snap(300, resets_at, used_pct, NOW_MS)], NOW_MS).remove(0)
}

#[test]
fn alerts_fire_once_per_window_at_the_highest_threshold() {
    let h = Home::new();
    let t = limits::thresholds(&h.store).unwrap();
    assert_eq!(t, vec![75, 90, 100]);

    let a = new_alerts(&h.store, &[window(95.0, NOW_S + 3600)], &t).unwrap();
    assert_eq!(a.iter().map(|a| a.threshold).collect::<Vec<_>>(), vec![90]);
    assert!(new_alerts(&h.store, &[window(96.0, NOW_S + 3620)], &t).unwrap().is_empty(), "same window, drifted reset");
    assert_eq!(new_alerts(&h.store, &[window(100.0, NOW_S + 3600)], &t).unwrap()[0].threshold, 100);
    assert_eq!(new_alerts(&h.store, &[window(80.0, NOW_S + 5 * 3600)], &t).unwrap()[0].threshold, 75, "new window");

    limits::set_thresholds(&h.store, &[90, 0, 50, 90, 150]).unwrap();
    assert_eq!(limits::thresholds(&h.store).unwrap(), vec![50, 90]);
}

#[test]
fn billing_periods_clamp_to_short_months() {
    let d = |y, m, d| NaiveDate::from_ymd_opt(y, m, d).unwrap();
    assert_eq!(billing_period(d(2026, 10, 28), 15), (d(2026, 10, 15), d(2026, 11, 15)));
    assert_eq!(billing_period(d(2026, 10, 3), 15), (d(2026, 9, 15), d(2026, 10, 15)));
    assert_eq!(billing_period(d(2026, 3, 1), 31), (d(2026, 2, 28), d(2026, 3, 31)));
    assert_eq!(billing_period(d(2026, 2, 28), 31), (d(2026, 2, 28), d(2026, 3, 31)));
}

#[test]
fn plan_value_counts_only_its_app_in_the_period() {
    let h = Home::new();
    let now = Local.with_ymd_and_hms(2026, 10, 28, 12, 0, 0).unwrap();
    let event = |key: &str, source: &str, day: u32| UsageEvent {
        key: key.into(),
        source: source.into(),
        provider: if source == "codex" { "openai" } else { "anthropic" }.into(),
        model: if source == "codex" { "gpt-5" } else { "claude-sonnet-4-5" }.into(),
        project: None,
        session: None,
        ts: Local.with_ymd_and_hms(2026, 10, day, 9, 0, 0).unwrap().timestamp_millis(),
        tokens: Tokens { input: 1_000_000, ..Tokens::default() },
        reported_cost: None,
    };
    let events = [event("a", "claude-code", 20), event("b", "claude-code", 2), event("c", "codex", 21)];
    h.store.commit("test", &Default::default(), &events, &[]).unwrap();
    let id = h
        .store
        .save_plan(&Plan {
            id: 0,
            provider: "anthropic".into(),
            name: "Max".into(),
            monthly_usd: 100.0,
            renews_day: 15,
        })
        .unwrap();
    assert!(id > 0);

    let l = limits::limits(&h.store, &Catalog::load(&h.store).unwrap(), now).unwrap();
    let p = &l.plans[0];
    assert_eq!((p.period_start.as_str(), p.period_end.as_str()), ("2026-10-15", "2026-11-15"));
    assert_eq!(p.source.as_deref(), Some("claude-code"));
    assert_eq!(p.usage.calls, 1);
    assert!(p.usage.cost > 0.0, "sonnet input is priced");

    h.store.delete_plan(id).unwrap();
    assert!(h.store.plans().unwrap().is_empty());
}
