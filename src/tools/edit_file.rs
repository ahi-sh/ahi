//! `edit_file`: replace one exact occurrence of `old` with `new`, or every
//! occurrence with `replace_all`. No fuzzy matching, no whitespace
//! normalization: distinguish a missing match from an ambiguous one instead
//! of guessing which occurrence the operator intended.

use std::path::Path;

use super::write_file::{line_count, read_text, WriteReport, WriteTexts};
use super::ToolFailure;

pub(super) fn run(
    path: &Path,
    old: &str,
    new: &str,
    replace_all: bool,
) -> Result<(String, WriteReport, WriteTexts), ToolFailure> {
    if old.is_empty() {
        return Err("old must not be empty".into());
    }
    let (text, before) = read_text(path)?;
    let count = text.matches(old).count();
    let replaced = match (count, replace_all) {
        (0, _) => return Err("old text not found in the file (0 occurrences)".into()),
        (1, _) => text.replacen(old, new, 1),
        (_, true) => text.replace(old, new),
        (_, false) => {
            return Err(ToolFailure { message: format!("old text occurs more than once ({count} occurrences); include more context or set replace_all"), ran: None })
        }
    };
    let removed = line_count(old) * count;
    let added = line_count(new) * count;
    let report = WriteReport::replace(path, &before, replaced.as_bytes(), added, removed)?;
    let content = format!(
        "edited {}: {count} replacement{}, +{added} -{removed} lines",
        path.file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default(),
        if count == 1 { "" } else { "s" }
    );
    let texts = WriteTexts {
        before: Some(text),
        after: replaced,
    };
    Ok((content, report, texts))
}
