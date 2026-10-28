//! End-to-end tests: synthetic logs in a temporary home folder, scanned into
//! a temporary store. No real logs are used.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{Local, TimeZone};
use ganana_core::{Catalog, Roots, Store, Tokens, overview, scan_all};
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
        self.dir.path().join("home").join(rel)
    }

    fn write(&self, rel: &str, text: &str) -> PathBuf {
        let p = self.path(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, text).unwrap();
        p
    }

    fn append(&self, path: &Path, text: &str) {
        OpenOptions::new().append(true).open(path).unwrap().write_all(text.as_bytes()).unwrap();
    }

    fn scan(&self) -> usize {
        let report = scan_all(
            &self.store,
            &Roots { home: self.dir.path().join("home"), relay_dir: Some(self.dir.path().join("relay")) },
        );
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        report.events
    }

    fn events(&self) -> Vec<ganana_core::UsageEvent> {
        self.store.events_since(0).unwrap()
    }
}

fn claude_line(id: &str, req: &str, ts: &str, model: &str, input: i64, output: i64) -> String {
    format!(
        r#"{{"type":"assistant","timestamp":"{ts}","cwd":"/work/alpha","sessionId":"s1","requestId":"{req}","message":{{"id":"{id}","model":"{model}","content":[{{"type":"text","text":"secret prompt text"}}],"usage":{{"input_tokens":{input},"output_tokens":{output},"cache_creation_input_tokens":300,"cache_read_input_tokens":1000,"cache_creation":{{"ephemeral_5m_input_tokens":100,"ephemeral_1h_input_tokens":200}},"output_tokens_details":{{"thinking_tokens":5}}}}}}}}"#
    ) + "\n"
}

const USER_LINE: &str = "{\"type\":\"user\",\"timestamp\":\"2026-10-01T10:00:00Z\",\"message\":{\"role\":\"user\",\"content\":\"hello\"}}\n";

#[test]
fn claude_code_dedupes_and_normalises() {
    let h = Home::new();
    let log = USER_LINE.to_string()
        + &claude_line("m1", "r1", "2026-10-01T10:00:01Z", "claude-opus-5-5", 10, 50)
        // Same response logged again for another content block.
        + &claude_line("m1", "r1", "2026-10-01T10:00:02Z", "claude-opus-5-5", 10, 50)
        + &claude_line("m2", "r2", "2026-10-01T10:00:03Z", "<synthetic>", 0, 0)
        + &claude_line("m3", "r3", "2026-10-01T10:01:00Z", "claude-sonnet-5-5", 20, 60);
    h.write(".claude/projects/-work-alpha/s1.jsonl", &log);
    h.scan();

    let events = h.events();
    assert_eq!(events.len(), 2);
    let e = &events[0];
    assert_eq!(e.key, "claude-code:m1:r1");
    assert_eq!(e.project.as_deref(), Some("alpha"));
    assert_eq!(e.ts, chrono::DateTime::parse_from_rfc3339("2026-10-01T10:00:01Z").unwrap().timestamp_millis());
    assert_eq!(
        e.tokens,
        Tokens { input: 10, output: 50, cache_read: 1000, cache_write: 100, cache_write_1h: 200, reasoning: 5 }
    );
}

#[test]
fn resumed_session_keeps_first_time() {
    let h = Home::new();
    h.write(".claude/projects/p/a.jsonl", &claude_line("m1", "r1", "2026-10-01T23:59:00Z", "claude-opus-5-5", 1, 1));
    h.write(".claude/projects/p/b.jsonl", &claude_line("m1", "r1", "2026-10-03T08:00:00Z", "claude-opus-5-5", 1, 1));
    h.scan();
    let events = h.events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].ts, chrono::DateTime::parse_from_rfc3339("2026-10-01T23:59:00Z").unwrap().timestamp_millis());
}

#[test]
fn rescan_reads_only_new_complete_lines() {
    let h = Home::new();
    let line = claude_line("m1", "r1", "2026-10-01T10:00:00Z", "claude-opus-5-5", 1, 1);
    let (head, tail) = line.split_at(40);
    let p = h.write(".claude/projects/p/s.jsonl", head);
    h.scan();
    assert_eq!(h.events().len(), 0, "a half-written line is not read");

    h.append(&p, tail);
    h.scan();
    assert_eq!(h.events().len(), 1);

    assert_eq!(h.scan(), 0, "an unchanged file is skipped");
    h.append(&p, &claude_line("m2", "r2", "2026-10-01T10:05:00Z", "claude-opus-5-5", 1, 1));
    assert_eq!(h.scan(), 1, "only the new line is parsed");
    assert_eq!(h.events().len(), 2);
}

#[test]
fn replaced_file_is_read_again_without_double_counting() {
    let h = Home::new();
    let two = claude_line("m1", "r1", "2026-10-01T10:00:00Z", "claude-opus-5-5", 1, 1)
        + &claude_line("m2", "r2", "2026-10-01T10:01:00Z", "claude-opus-5-5", 1, 1);
    let p = h.write(".claude/projects/p/s.jsonl", &two);
    h.scan();
    fs::write(&p, claude_line("m3", "r3", "2026-10-01T10:02:00Z", "claude-opus-5-5", 1, 1)).unwrap();
    h.scan();
    assert_eq!(h.events().len(), 3);
}

fn codex_usage(input: i64, cached: i64, output: i64, reasoning: i64) -> String {
    format!(
        r#"{{"input_tokens":{input},"cached_input_tokens":{cached},"cache_write_input_tokens":0,"output_tokens":{output},"reasoning_output_tokens":{reasoning},"total_tokens":{}}}"#,
        input + output
    )
}

#[test]
fn codex_record_and_count_share_one_event() {
    let h = Home::new();
    let u = codex_usage(1000, 600, 50, 20);
    let log = [
        r#"{"timestamp":"2026-10-02T09:00:00Z","type":"session_meta","payload":{"id":"sess-1","cwd":"/work/beta"}}"#.to_string(),
        r#"{"timestamp":"2026-10-02T09:00:01Z","type":"turn_context","payload":{"model":"gpt-6-astra","cwd":"/work/beta"}}"#.to_string(),
        format!(r#"{{"timestamp":"2026-10-02T09:00:05Z","type":"token_usage_record","payload":{{"response_id":"x","usage":{u},"thread_token_usage":{{"total_tokens":1050}}}}}}"#),
        format!(r#"{{"timestamp":"2026-10-02T09:00:05Z","type":"event_msg","payload":{{"type":"token_count","info":{{"last_token_usage":{u},"total_token_usage":{{"total_tokens":1050}}}}}}}}"#),
    ]
    .join("\n")
        + "\n";
    let p = h.write(".codex/sessions/2026/10/02/rollout-a.jsonl", &log);
    h.scan();

    // A later scan of appended lines still knows the model and folder.
    h.append(
        &p,
        &(format!(r#"{{"timestamp":"2026-10-02T09:01:00Z","type":"event_msg","payload":{{"type":"token_count","info":{{"last_token_usage":{u},"total_token_usage":{{"total_tokens":2100}}}}}}}}"#) + "\n"),
    );
    h.scan();

    let events = h.events();
    assert_eq!(events.len(), 2);
    for e in &events {
        assert_eq!(e.model, "gpt-6-astra");
        assert_eq!(e.project.as_deref(), Some("beta"));
        assert_eq!(e.tokens, Tokens { input: 400, output: 50, cache_read: 600, reasoning: 20, ..Tokens::default() });
    }
}

#[test]
fn qwen_code_subtracts_cached_input() {
    let h = Home::new();
    h.write(
        ".qwen/usage/token-usage-2026-10.jsonl",
        "{\"id\":\"q1\",\"timestamp\":\"2026-10-03T10:00:00Z\",\"sessionId\":\"s\",\"model\":\"qwen3.8-max\",\"inputTokens\":1000,\"outputTokens\":90,\"cachedTokens\":700,\"thoughtsTokens\":30,\"totalTokens\":1090}\n",
    );
    h.scan();
    let e = &h.events()[0];
    assert_eq!(e.provider, "alibaba");
    assert_eq!(e.tokens, Tokens { input: 300, output: 90, cache_read: 700, reasoning: 30, ..Tokens::default() });
}

#[test]
fn gemini_cli_reads_whole_session_files() {
    let h = Home::new();
    let session = |extra: &str| {
        format!(
            r#"{{"sessionId":"g1","messages":[{{"id":"u1","type":"user","content":"hi"}},{{"id":"a1","type":"gemini","timestamp":"2026-10-04T10:00:00Z","model":"gemini-3-pro","tokens":{{"input":500,"output":40,"cached":200,"thoughts":10,"tool":0,"total":550}}}}{extra}]}}"#
        )
    };
    let p = h.write(".gemini/tmp/myproj/chats/session-1.json", &session(""));
    h.scan();
    assert_eq!(h.events().len(), 1);
    let e = &h.events()[0];
    assert_eq!(e.project.as_deref(), Some("myproj"));
    assert_eq!(e.tokens, Tokens { input: 300, output: 50, cache_read: 200, reasoning: 10, ..Tokens::default() });

    fs::write(
        &p,
        session(r#",{"id":"a2","type":"gemini","timestamp":"2026-10-04T10:05:00Z","model":"gemini-3-pro","tokens":{"input":10,"output":1,"cached":0,"thoughts":0}}"#),
    )
    .unwrap();
    h.scan();
    assert_eq!(h.events().len(), 2);
}

#[test]
fn opencode_reads_updated_messages() {
    let h = Home::new();
    let db = h.path(".local/share/opencode/opencode.db");
    fs::create_dir_all(db.parent().unwrap()).unwrap();
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute_batch(
        "CREATE TABLE session_v2 (id TEXT PRIMARY KEY, directory TEXT NOT NULL);
         CREATE TABLE session_message (id TEXT PRIMARY KEY, session_id TEXT NOT NULL, type TEXT NOT NULL,
            seq INTEGER NOT NULL, time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL, data TEXT NOT NULL);
         INSERT INTO session_v2 VALUES ('s1', '/work/gamma');",
    )
    .unwrap();
    let data = |output: i64| {
        format!(
            r#"{{"model":{{"id":"claude-opus-5-5","providerID":"anthropic"}},"cost":0,"tokens":{{"input":100,"output":{output},"reasoning":5,"cache":{{"read":50,"write":0}}}}}}"#
        )
    };
    conn.execute(
        "INSERT INTO session_message VALUES ('m1','s1','assistant',1,1759312800000,1759312800000,?1)",
        [data(10)],
    )
    .unwrap();
    h.scan();
    assert_eq!(h.events()[0].tokens.output, 15);

    // Still streaming: the same row gets final counts later.
    conn.execute("UPDATE session_message SET data = ?1, time_updated = 1759312900000", [data(40)]).unwrap();
    h.scan();
    let events = h.events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].tokens.output, 45);
    assert_eq!(events[0].project.as_deref(), Some("gamma"));
}

#[test]
fn overview_buckets_by_local_day_and_flags_unpriced_models() {
    let h = Home::new();
    let now = Local.with_ymd_and_hms(2026, 10, 10, 12, 0, 0).unwrap();
    let at = |d: u32, hour: u32| Local.with_ymd_and_hms(2026, 10, d, hour, 0, 0).unwrap().to_rfc3339();
    let log = claude_line("m1", "r1", &at(10, 9), "claude-opus-5-5", 1_000_000, 0)
        + &claude_line("m2", "r2", &at(8, 9), "router-alias", 500, 0)
        + &claude_line("m3", "r3", &at(1, 9), "claude-opus-5-5", 1_000_000, 0);
    h.write(".claude/projects/p/s.jsonl", &log);
    h.scan();

    let catalog = Catalog::bundled();
    let o = overview(&h.store, &catalog, 7, now).unwrap();
    assert_eq!(o.daily.len(), 7);
    assert_eq!(o.daily.last().unwrap().date, "2026-10-10");
    assert_eq!(o.today.calls, 1);
    assert_eq!(o.period.calls, 2, "the 1 October call is outside the window");
    assert_eq!(o.unpriced.len(), 1);
    assert_eq!(o.unpriced[0].model, "router-alias");
    assert!(o.today.cost > 0.0);

    // Mapping the alias prices it, for all history.
    h.store.set_alias("router-alias", Some("anthropic/claude-opus-5-5")).unwrap();
    let o = overview(&h.store, &Catalog::load(&h.store).unwrap(), 7, now).unwrap();
    assert!(o.unpriced.is_empty());
}
