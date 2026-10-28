//! Claude Pro/Max limits. Claude Code shows them only to its status line
//! command, as `rate_limits.five_hour` and `rate_limits.seven_day` in the
//! JSON it pipes to that command. Gaṇanā's binary can act as that command
//! (`--ganana-statusline <dir>`): it appends changed limit readings to
//! `<dir>/claude-limits.jsonl`, which the app reads like any other log, and
//! prints a status line. Nothing else from the JSON is kept.
//!
//! A status line the user already had is saved to `<dir>/statusline-wrap.json`
//! and still runs: its output is shown instead of Gaṇanā's.

use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde_json::{Map, Value, json};

pub const FLAG: &str = "--ganana-statusline";
pub const LIMITS_FILE: &str = "claude-limits.jsonl";
const WRAP_FILE: &str = "statusline-wrap.json";
const WINDOWS: [&str; 2] = ["five_hour", "seven_day"];

/// Entry point when Claude Code runs us as its status line.
pub fn run(dir: &Path) {
    let mut input = String::new();
    let _ = std::io::stdin().take(1 << 20).read_to_string(&mut input);
    let line = relay(&input, dir, chrono::Utc::now().timestamp_millis());
    println!("{line}");
}

/// Records the limits in `input` if they changed and returns the status line
/// to print. Never fails: a status line must always print something.
pub fn relay(input: &str, dir: &Path, now_ms: i64) -> String {
    let v: Value = serde_json::from_str(input).unwrap_or_default();
    let windows = limit_windows(&v["rate_limits"]);
    if !windows.is_empty() && last_windows(&dir.join(LIMITS_FILE)).as_ref() != Some(&windows) {
        let mut record = windows.clone();
        record.insert("ts".into(), now_ms.into());
        let _ = fs::create_dir_all(dir).and_then(|_| {
            let mut f = fs::OpenOptions::new().create(true).append(true).open(dir.join(LIMITS_FILE))?;
            // One short write per line, so a reader never sees half a line.
            f.write_all(format!("{}\n", Value::Object(record)).as_bytes())
        });
    }
    if let Some(out) = wrapped(input, dir) {
        return out;
    }
    let mut parts: Vec<String> = v["model"]["display_name"].as_str().map(str::to_string).into_iter().collect();
    for (name, label) in WINDOWS.iter().zip(["5h", "week"]) {
        if let Some(p) = windows.get(*name).and_then(|w| w["used_percentage"].as_f64()) {
            parts.push(format!("{label} {p:.0}%"));
        }
    }
    parts.join(" · ")
}

/// The windows present, each reduced to its two numbers.
fn limit_windows(rate_limits: &Value) -> Map<String, Value> {
    WINDOWS
        .iter()
        .filter_map(|name| {
            let w = &rate_limits[*name];
            let (pct, reset) = (w["used_percentage"].as_f64()?, w["resets_at"].as_i64()?);
            Some((name.to_string(), json!({ "used_percentage": pct, "resets_at": reset })))
        })
        .collect()
}

/// The windows of the last line in the limits file.
fn last_windows(path: &Path) -> Option<Map<String, Value>> {
    let mut f = fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    f.seek(SeekFrom::Start(len.saturating_sub(4096))).ok()?;
    let mut tail = String::new();
    f.read_to_string(&mut tail).ok()?;
    let last = tail.lines().rev().find(|l| !l.trim().is_empty())?;
    let Value::Object(mut record) = serde_json::from_str(last).ok()? else { return None };
    record.remove("ts");
    Some(record)
}

/// Output of the user's own status line command, if they had one.
fn wrapped(input: &str, dir: &Path) -> Option<String> {
    let saved: Value = serde_json::from_str(&fs::read_to_string(dir.join(WRAP_FILE)).ok()?).ok()?;
    let command = saved["command"].as_str()?;
    let mut child = shell(command).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    let mut stdin = child.stdin.take()?;
    let input = input.to_string();
    // Write from a thread: a command that prints before reading could
    // otherwise fill its output pipe while we block on its input.
    let writer = std::thread::spawn(move || stdin.write_all(input.as_bytes()));
    let out = child.wait_with_output().ok()?;
    let _ = writer.join();
    Some(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
}

fn shell(command: &str) -> Command {
    if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.args(["/C", command]);
        c
    } else {
        let mut c = Command::new("sh");
        c.args(["-c", command]);
        c
    }
}

/// Whether Claude Code's status line runs through Gaṇanā.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum RelayStatus {
    /// Installed; `wraps` is the user's own command that still runs.
    On { wraps: Option<String> },
    /// Not installed; `existing` is the status line that would be kept.
    Off { existing: Option<String> },
}

/// Claude Code's user settings file.
pub fn settings_path(home: &Path) -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR").map_or_else(|| home.join(".claude"), PathBuf::from).join("settings.json")
}

pub fn status(settings: &Path, dir: &Path) -> Result<RelayStatus> {
    let s = read_settings(settings)?;
    let command = s.get("statusLine").and_then(|l| l["command"].as_str()).map(str::to_string);
    Ok(match command {
        Some(c) if c.contains(FLAG) => {
            let wraps = fs::read_to_string(dir.join(WRAP_FILE))
                .ok()
                .and_then(|t| serde_json::from_str::<Value>(&t).ok())
                .and_then(|v| v["command"].as_str().map(str::to_string));
            RelayStatus::On { wraps }
        }
        existing => RelayStatus::Off { existing },
    })
}

/// Points Claude Code's status line at `exe`. Backs up the settings file
/// into `dir` first and keeps any existing status line running. If it is
/// already installed from another path (the app moved), only the path changes.
pub fn install(settings: &Path, dir: &Path, exe: &Path) -> Result<()> {
    let mut s = read_settings(settings)?;
    let command = format!("{} {FLAG} {}", quote(exe), quote(dir));
    let existing = s.get("statusLine").cloned();
    if let Some(current) = existing.as_ref().and_then(|l| l["command"].as_str()).filter(|c| c.contains(FLAG)) {
        if current != command {
            s["statusLine"]["command"] = command.into();
            write_settings(settings, &s)?;
        }
        return Ok(());
    }
    fs::create_dir_all(dir)?;
    if settings.exists() {
        let backup = dir.join(format!("settings-backup-{}.json", chrono::Utc::now().timestamp()));
        fs::copy(settings, &backup).with_context(|| format!("backing up {}", settings.display()))?;
    }
    let mut line = match &existing {
        Some(Value::Object(old)) if old.get("command").is_some_and(Value::is_string) => {
            fs::write(dir.join(WRAP_FILE), serde_json::to_string_pretty(&existing)?)?;
            old.clone()
        }
        _ => {
            let _ = fs::remove_file(dir.join(WRAP_FILE));
            Map::new()
        }
    };
    line.insert("type".into(), "command".into());
    line.insert("command".into(), command.into());
    s.insert("statusLine".into(), Value::Object(line));
    write_settings(settings, &s)
}

/// Undoes `install`: restores the user's own status line, or removes ours.
pub fn uninstall(settings: &Path, dir: &Path) -> Result<()> {
    let mut s = read_settings(settings)?;
    let ours = s.get("statusLine").and_then(|l| l["command"].as_str()).is_some_and(|c| c.contains(FLAG));
    if !ours {
        return Ok(());
    }
    let saved = fs::read_to_string(dir.join(WRAP_FILE)).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok());
    match saved {
        Some(old @ Value::Object(_)) => s.insert("statusLine".into(), old),
        _ => s.remove("statusLine"),
    };
    write_settings(settings, &s)?;
    let _ = fs::remove_file(dir.join(WRAP_FILE));
    Ok(())
}

fn read_settings(path: &Path) -> Result<Map<String, Value>> {
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Map::new()),
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str(&text) {
        Ok(Value::Object(m)) => Ok(m),
        _ => bail!("{} is not a JSON object, so it was left unchanged", path.display()),
    }
}

/// Writes through a temporary file and a rename, so Claude Code never reads
/// a half-written file. A symlinked settings file (dotfiles) stays a symlink.
fn write_settings(path: &Path, s: &Map<String, Value>) -> Result<()> {
    let target = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let dir = target.parent().context("settings file has no folder")?;
    fs::create_dir_all(dir)?;
    let tmp = dir.join(".settings.json.ganana-tmp");
    fs::write(&tmp, format!("{}\n", serde_json::to_string_pretty(s)?))?;
    if let Ok(meta) = fs::metadata(&target) {
        fs::set_permissions(&tmp, meta.permissions())?;
    }
    fs::rename(&tmp, &target)?;
    Ok(())
}

/// Quotes a path for the shell Claude Code runs the command in.
fn quote(p: &Path) -> String {
    let s = p.to_string_lossy();
    if cfg!(windows) { format!("\"{s}\"") } else { format!("'{}'", s.replace('\'', r"'\''")) }
}
