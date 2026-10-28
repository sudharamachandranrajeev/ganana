//! Incremental reading of append-only logs.

use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::Path;
use std::time::UNIX_EPOCH;

use crate::db::Cursor;

/// Size and modification time (Unix ms) of a file.
pub fn stat(path: &Path) -> std::io::Result<(u64, i64)> {
    let meta = std::fs::metadata(path)?;
    let mtime = meta.modified()?.duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
    Ok((meta.len(), mtime))
}

/// Calls `on_line` for each complete line added since `prev`.
///
/// Returns the new cursor, or `None` when the file is unchanged. A trailing
/// line without a newline is still being written, so it is left for the next
/// read. A file smaller than the old offset was replaced: callers pass
/// `prev = None` for it (see [`replaced`]).
pub fn read_new(path: &Path, prev: Option<&Cursor>, mut on_line: impl FnMut(&str)) -> std::io::Result<Option<Cursor>> {
    let (size, mtime) = stat(path)?;
    let mut offset = prev.map_or(0, |c| c.offset);
    if prev.is_some_and(|c| c.size == size && c.mtime == mtime) {
        return Ok(None);
    }

    let mut reader = BufReader::with_capacity(1 << 20, File::open(path)?);
    reader.seek(SeekFrom::Start(offset))?;
    let mut buf = Vec::new();
    loop {
        buf.clear();
        let n = reader.read_until(b'\n', &mut buf)?;
        if n == 0 || buf.last() != Some(&b'\n') {
            break;
        }
        offset += n as u64;
        let line = String::from_utf8_lossy(&buf);
        let line = line.trim();
        if !line.is_empty() {
            on_line(line);
        }
    }

    let state = prev.and_then(|c| c.state.clone());
    Ok(Some(Cursor { offset, size, mtime, state }))
}

/// True when the file shrank below the saved offset, so it was replaced and
/// must be read again from the start without the old parser state.
pub fn replaced(path: &Path, prev: &Cursor) -> std::io::Result<bool> {
    Ok(stat(path)?.0 < prev.offset)
}
