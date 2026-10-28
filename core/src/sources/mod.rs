//! Readers for the usage logs AI tools keep on this machine.

mod claude_code;
mod codex;
mod gemini_cli;
mod opencode;
mod qwen_code;

use std::path::{Path, PathBuf};

use serde::Serialize;
use walkdir::WalkDir;

use crate::db::Store;
use crate::jsonl;
use crate::model::UsageEvent;

/// Where the tools keep their logs. Tests point `home` at a fixture folder.
#[derive(Debug, Clone)]
pub struct Roots {
    pub home: PathBuf,
}

impl Roots {
    pub fn from_env() -> Option<Self> {
        std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(|h| Self { home: h.into() })
    }

    fn claude_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = vec![self.home.join(".claude/projects"), self.home.join(".config/claude/projects")];
        if let Some(custom) = std::env::var_os("CLAUDE_CONFIG_DIR") {
            dirs.push(PathBuf::from(custom).join("projects"));
        }
        dirs
    }

    fn codex_dir(&self) -> PathBuf {
        std::env::var_os("CODEX_HOME").map_or_else(|| self.home.join(".codex"), PathBuf::from).join("sessions")
    }

    fn gemini_dir(&self) -> PathBuf {
        self.home.join(".gemini/tmp")
    }

    fn qwen_dir(&self) -> PathBuf {
        self.home.join(".qwen/usage")
    }

    fn opencode_db(&self) -> PathBuf {
        self.home.join(".local/share/opencode/opencode.db")
    }

    /// Existing folders worth watching for changes.
    pub fn watch_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = self.claude_dirs();
        dirs.extend([self.codex_dir(), self.gemini_dir(), self.qwen_dir()]);
        if let Some(dir) = self.opencode_db().parent() {
            dirs.push(dir.to_path_buf());
        }
        dirs.sort();
        dirs.dedup();
        dirs.retain(|d| d.is_dir());
        dirs
    }
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct ScanReport {
    pub files_read: usize,
    pub events: usize,
    pub errors: Vec<String>,
}

/// Reads everything new since the last scan, from every source.
pub fn scan_all(store: &Store, roots: &Roots) -> ScanReport {
    let mut report = ScanReport::default();
    for dir in roots.claude_dirs() {
        for file in files_with_ext(&dir, "jsonl") {
            scan_jsonl::<claude_code::Parser>(store, &file, &mut report);
        }
    }
    for file in files_with_ext(&roots.codex_dir(), "jsonl") {
        scan_jsonl::<codex::Parser>(store, &file, &mut report);
    }
    for file in files_with_ext(&roots.qwen_dir(), "jsonl") {
        scan_jsonl::<qwen_code::Parser>(store, &file, &mut report);
    }
    for file in files_with_ext(&roots.gemini_dir(), "json") {
        if file.parent().is_some_and(|p| p.ends_with("chats")) {
            note(&mut report, &file, gemini_cli::scan(store, &file));
        }
    }
    let oc = roots.opencode_db();
    if oc.is_file() {
        note(&mut report, &oc, opencode::scan(store, &oc));
    }
    report
}

fn note(report: &mut ScanReport, file: &Path, result: anyhow::Result<Option<usize>>) {
    match result {
        Ok(Some(n)) => {
            report.files_read += 1;
            report.events += n;
        }
        Ok(None) => {}
        Err(e) => report.errors.push(format!("{}: {e:#}", file.display())),
    }
}

fn files_with_ext(dir: &Path, ext: &str) -> Vec<PathBuf> {
    if !dir.is_dir() {
        return Vec::new();
    }
    let mut files: Vec<PathBuf> = WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file() && e.path().extension().is_some_and(|x| x == ext))
        .map(|e| e.into_path())
        .collect();
    files.sort();
    files
}

/// A line-by-line parser for one append-only log. `state` survives between
/// scans so a resumed read keeps context such as the current model.
trait LineParser {
    fn new(path: &Path, state: Option<&str>) -> Self;
    fn line(&mut self, line: &str, out: &mut Vec<UsageEvent>);
    fn state(&self) -> Option<String> {
        None
    }
}

fn scan_jsonl<P: LineParser>(store: &Store, path: &Path, report: &mut ScanReport) {
    let result = (|| {
        let key = path.to_string_lossy();
        let mut prev = store.cursor(&key)?;
        if let Some(c) = &prev
            && jsonl::replaced(path, c)?
        {
            prev = None;
        }
        let mut parser = P::new(path, prev.as_ref().and_then(|c| c.state.as_deref()));
        let mut events = Vec::new();
        let Some(mut cursor) = jsonl::read_new(path, prev.as_ref(), |l| parser.line(l, &mut events))? else {
            return Ok(None);
        };
        cursor.state = parser.state();
        store.commit(&key, &cursor, &events)?;
        Ok(Some(events.len()))
    })();
    note(report, path, result);
}
