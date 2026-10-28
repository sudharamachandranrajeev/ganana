//! Usage from the user's other computers, through a folder they keep in
//! sync between them (Syncthing, a cloud drive, a USB stick). Each computer
//! appends its own calls and limit readings to `<device>.ganana.jsonl` in that
//! folder and reads the other computers' files.
//!
//! Only what the store keeps travels: token counts, model names, times,
//! project folder names and limit percentages. No keys, no prompts. Calls
//! keep their keys, so a call two computers both logged counts once.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::db::Store;
use crate::jsonl;
use crate::model::{LimitSnapshot, UsageEvent};

/// Setting: the shared folder.
pub const FOLDER: &str = "share_folder";
/// Setting: this computer's name in the folder.
pub const DEVICE: &str = "device_name";
const SUFFIX: &str = ".ganana.jsonl";

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum Line {
    Event(UsageEvent),
    Limit(LimitSnapshot),
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct ShareReport {
    /// Lines this computer added to its own file.
    pub written: usize,
    /// Calls and limit readings read from other computers.
    pub events: usize,
    pub limits: usize,
    pub errors: Vec<String>,
}

pub fn folder(store: &Store) -> Result<Option<PathBuf>> {
    Ok(store.setting(FOLDER)?.filter(|f| !f.is_empty()).map(PathBuf::from))
}

/// This computer's name: the one the user chose, or the host name.
pub fn device(store: &Store) -> Result<String> {
    Ok(store.setting(DEVICE)?.map_or_else(default_device, |d| clean_name(&d)))
}

pub fn default_device() -> String {
    let host = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .or_else(|_| std::fs::read_to_string("/etc/hostname"))
        .unwrap_or_default();
    clean_name(&host)
}

/// A name that is safe as a file name on every system.
pub fn clean_name(name: &str) -> String {
    let cleaned: String = name
        .trim()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .take(40)
        .collect();
    let cleaned = cleaned.trim_matches('-');
    if cleaned.is_empty() { "computer".into() } else { cleaned.to_lowercase() }
}

/// Writes this computer's new usage to the folder, then reads the others'.
pub fn sync(store: &Store, folder: &Path, device: &str) -> ShareReport {
    let mut report = ShareReport::default();
    if !folder.is_dir() {
        report.errors.push(format!("the shared folder {} is not there", folder.display()));
        return report;
    }
    match export(store, folder, device) {
        Ok(n) => report.written = n,
        Err(e) => report.errors.push(format!("{e:#}")),
    }
    import(store, folder, device, &mut report);
    report
}

fn export(store: &Store, folder: &Path, device: &str) -> Result<usize> {
    let file = folder.join(format!("{device}{SUFFIX}"));
    if !file.exists() {
        store.reset_shared()?;
    }
    // Create the file even with nothing to write: it being there means
    // everything up to now is in it.
    let mut out = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file)
        .with_context(|| format!("could not write {}", file.display()))?;
    let (events, limits) = store.unshared()?;
    if events.is_empty() && limits.is_empty() {
        return Ok(0);
    }
    let mut text = String::new();
    for e in &events {
        text.push_str(&serde_json::json!({ "event": e }).to_string());
        text.push('\n');
    }
    for l in &limits {
        text.push_str(&serde_json::json!({ "limit": l }).to_string());
        text.push('\n');
    }
    out.write_all(text.as_bytes())?;
    out.sync_all()?;
    store.mark_shared()?;
    Ok(events.len() + limits.len())
}

fn import(store: &Store, folder: &Path, own: &str, report: &mut ShareReport) {
    let Ok(entries) = std::fs::read_dir(folder) else { return };
    let mut files: Vec<(String, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let device = name.strip_suffix(SUFFIX)?.to_string();
            (device != own && e.path().is_file()).then(|| (device, e.path()))
        })
        .collect();
    files.sort();
    for (device, path) in files {
        let result = (|| -> Result<(usize, usize)> {
            let key = path.to_string_lossy();
            let mut prev = store.cursor(&key)?;
            if let Some(c) = &prev
                && jsonl::replaced(&path, c)?
            {
                prev = None;
            }
            let (mut events, mut limits) = (Vec::new(), Vec::new());
            let Some(cursor) = jsonl::read_new(&path, prev.as_ref(), |line| match serde_json::from_str(line) {
                Ok(Line::Event(e)) => events.push(e),
                Ok(Line::Limit(l)) => limits.push(l),
                // A line from a newer Gaṇanā: skip it.
                Err(_) => {}
            })?
            else {
                return Ok((0, 0));
            };
            store.commit_from(&device, &key, &cursor, &events, &limits)?;
            Ok((events.len(), limits.len()))
        })();
        match result {
            Ok((e, l)) => {
                report.events += e;
                report.limits += l;
            }
            Err(e) => report.errors.push(format!("{}: {e:#}", path.display())),
        }
    }
}

/// One computer's share of the usage in this store.
#[derive(Debug, Clone, Serialize)]
pub struct Device {
    pub name: String,
    /// This computer.
    pub this: bool,
    pub calls: i64,
    pub tokens: i64,
    /// Unix ms of its latest call.
    pub last_ts: i64,
}

pub fn devices(store: &Store) -> Result<Vec<Device>> {
    let me = device(store)?;
    let mut list: Vec<Device> = store
        .devices()?
        .into_iter()
        .map(|(name, calls, tokens, last_ts)| Device {
            this: name.is_empty(),
            name: if name.is_empty() { me.clone() } else { name },
            calls,
            tokens,
            last_ts,
        })
        .collect();
    list.sort_by(|a, b| b.this.cmp(&a.this).then(a.name.cmp(&b.name)));
    Ok(list)
}

/// Checks a folder before it is saved as the shared folder.
pub fn check_folder(folder: &Path) -> Result<()> {
    if !folder.is_absolute() {
        bail!("give the full path of the folder");
    }
    if !folder.is_dir() {
        bail!("{} is not a folder", folder.display());
    }
    Ok(())
}
