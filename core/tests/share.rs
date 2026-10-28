use std::path::Path;

use ganana_core::db::Cursor;
use ganana_core::{LimitSnapshot, Store, Tokens, UsageEvent, share};

fn event(key: &str, output: i64) -> UsageEvent {
    UsageEvent {
        key: key.into(),
        source: "claude-code".into(),
        provider: "anthropic".into(),
        model: "claude-sonnet-5-5".into(),
        project: Some("demo".into()),
        session: Some("s1".into()),
        ts: 1_790_000_000_000,
        tokens: Tokens { input: 10, output, ..Tokens::default() },
        reported_cost: None,
    }
}

fn limit(used_pct: f64) -> LimitSnapshot {
    LimitSnapshot {
        meter: "claude".into(),
        provider: "anthropic".into(),
        window_minutes: 300,
        resets_at: 1_790_010_000,
        used_pct,
        seen_at: 1_790_000_000_000,
    }
}

fn log(store: &Store, path: &str, events: &[UsageEvent], limits: &[LimitSnapshot]) {
    store.commit(path, &Cursor::default(), events, limits).unwrap();
}

fn total_output(store: &Store) -> i64 {
    store.events_since(0).unwrap().iter().map(|e| e.tokens.output).sum()
}

fn lines(path: &Path) -> usize {
    std::fs::read_to_string(path).unwrap().lines().count()
}

#[test]
fn computers_read_each_other_without_echo_or_double_counting() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("shared");
    std::fs::create_dir(&folder).unwrap();
    let a = Store::open(&dir.path().join("a.db")).unwrap();
    let b = Store::open(&dir.path().join("b.db")).unwrap();

    log(&a, "a-log", &[event("k1", 5), event("both", 7)], &[limit(20.0)]);
    log(&b, "b-log", &[event("k2", 3), event("both", 7)], &[]);

    let ra = share::sync(&a, &folder, "laptop-a");
    assert!(ra.errors.is_empty(), "{:?}", ra.errors);
    assert_eq!((ra.written, ra.events), (3, 0));
    let rb = share::sync(&b, &folder, "laptop-b");
    assert_eq!((rb.written, rb.events, rb.limits), (2, 2, 1));
    let ra = share::sync(&a, &folder, "laptop-a");
    // A writes nothing it read from B, and reads B's two calls.
    assert_eq!((ra.written, ra.events), (0, 2));

    // k1 + k2 + "both" once, on each computer.
    assert_eq!(total_output(&a), 15);
    assert_eq!(total_output(&b), 15);
    assert_eq!(b.limits_since(0).unwrap().len(), 1);

    // A call both computers logged stays local; the other is labelled.
    let devices = share::devices(&b).unwrap();
    let named: Vec<(&str, bool, i64)> = devices.iter().map(|d| (d.name.as_str(), d.this, d.calls)).collect();
    assert_eq!(named[1], ("laptop-a", false, 1));
    assert!(named[0].1 && named[0].2 == 2);

    // Nothing new: nothing written or read, and no limit reading echoed.
    let rb = share::sync(&b, &folder, "laptop-b");
    assert_eq!((rb.written, rb.events), (0, 0));
    let c = Store::open(&dir.path().join("c.db")).unwrap();
    share::sync(&c, &folder, "laptop-c");
    assert_eq!(share::sync(&c, &folder, "laptop-c").written, 0);
    assert_eq!(lines(&folder.join("laptop-b.ganana.jsonl")), 2);

    // A local call that grows is written again, and the newer count wins.
    log(&a, "a-log", &[event("k1", 9)], &[]);
    assert_eq!(share::sync(&a, &folder, "laptop-a").written, 1);
    share::sync(&b, &folder, "laptop-b");
    assert_eq!(total_output(&b), 19);
}

#[test]
fn a_deleted_file_is_written_again_in_full() {
    let dir = tempfile::tempdir().unwrap();
    let a = Store::open(&dir.path().join("a.db")).unwrap();
    log(&a, "a-log", &[event("k1", 5), event("k2", 5)], &[]);
    let file = dir.path().join("me.ganana.jsonl");
    share::sync(&a, dir.path(), "me");
    std::fs::remove_file(&file).unwrap();
    assert_eq!(share::sync(&a, dir.path(), "me").written, 2);
    assert_eq!(lines(&file), 2);
}

#[test]
fn a_missing_folder_is_reported_and_names_are_safe() {
    let dir = tempfile::tempdir().unwrap();
    let a = Store::open(&dir.path().join("a.db")).unwrap();
    let r = share::sync(&a, &dir.path().join("gone"), "me");
    assert_eq!(r.errors.len(), 1);
    assert_eq!(share::clean_name(" My Laptop (work)\n"), "my-laptop--work");
    assert_eq!(share::clean_name("../.."), "computer");
}
