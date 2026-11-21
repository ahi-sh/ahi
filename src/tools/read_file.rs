//! `read_file`: UTF-8 text, whole or a window of lines. Output is bounded by
//! bytes; when it does not fit, the tail names the next `offset` so the model
//! can continue without rereading.

use std::io::{BufRead, BufReader};

use super::helpers::Workspace;

pub(super) const MAX_BYTES: usize = 64 * 1024;
/// Bound the scan needed to count lines and validate UTF-8.
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// `offset` is a 1-based first line; `limit` a number of lines. `None`
/// means from the start, or to the end.
pub(super) fn run(
    workspace: &Workspace,
    relative: &str,
    offset: Option<usize>,
    limit: Option<usize>,
) -> Result<(String, usize), &'static str> {
    let opened = workspace.open_read(relative)?;
    if opened.size > MAX_FILE_BYTES {
        return Err("file is larger than 64 MiB");
    }
    let first = offset.unwrap_or(1).max(1);
    let wanted = limit.unwrap_or(usize::MAX).max(1);
    let paged = offset.is_some() || limit.is_some();

    let mut reader = BufReader::with_capacity(64 * 1024, opened.file);
    let mut body = String::new();
    let mut raw = Vec::new();
    let mut line_number = 0usize;
    let mut last_included = 0usize;
    let mut stopped_by_cap = false;
    loop {
        raw.clear();
        let read = reader
            .read_until(b'\n', &mut raw)
            .map_err(|_| "cannot read file")?;
        if read == 0 {
            break;
        }
        line_number += 1;
        if raw.contains(&0) {
            return Err("file is not UTF-8 text");
        }
        let text = std::str::from_utf8(&raw).map_err(|_| "file is not UTF-8 text")?;
        if line_number < first {
            continue;
        }
        if line_number - first >= wanted {
            // Past the window: keep counting lines for the header only.
            continue;
        }
        if stopped_by_cap {
            continue;
        }
        if body.len() + text.len() > MAX_BYTES {
            if body.is_empty() {
                return Err("requested line exceeds 64 KiB; skip it with a later offset or narrow the search with search_files");
            }
            stopped_by_cap = true;
            // Keep counting to report the total; nothing more is stored.
            continue;
        }
        body.push_str(text);
        last_included = line_number;
    }
    let total = line_number;
    if first > total.max(1) {
        return Err("offset is past the end of the file");
    }
    if total == 0 {
        return Ok((
            if paged {
                "[empty file]\n".into()
            } else {
                String::new()
            },
            0,
        ));
    }
    // The body is the file's bytes, verbatim. Only the header and the
    // continuation line are added, and only when the whole file is not
    // returned as is.
    let shown_all = last_included == total && first == 1;
    let mut out = String::with_capacity(body.len() + 64);
    if paged || !shown_all {
        out.push_str(&format!("[lines {first}-{last_included} of {total}]\n"));
    }
    out.push_str(&body);
    if last_included < total && (stopped_by_cap || paged) {
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&format!(
            "[{} more {}; continue with offset={}]",
            total - last_included,
            if total - last_included == 1 {
                "line"
            } else {
                "lines"
            },
            last_included + 1
        ));
    }
    Ok((out, last_included - first + 1))
}
