//! Bounded, skill-relative text reads. These checks are not a sandbox.
use std::fs;
use std::io::Read;
use std::path::Path;

use super::MAX_FILE_BYTES;

pub(crate) fn valid_reference(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 1024
        && !path.contains(['\\', ':', '\0'])
        && path.split('/').all(|part| {
            // Win32 trims trailing spaces/dots: '.. ' must not become traversal.
            !part.is_empty() && !part.ends_with([' ', '.'])
        })
        && !path.chars().any(char::is_control)
}

pub(super) fn read(root: &Path, relative: &str) -> Result<String, String> {
    if !valid_reference(relative) {
        return Err(
            "file must be a relative path inside the skill directory, without '.' or '..'".into(),
        );
    }
    // Check every component on all platforms, including the skill directory.
    // macOS/Linux also refuse links during descriptor-relative opens below.
    let mut path = root.to_path_buf();
    for part in std::iter::once(None).chain(relative.split('/').map(Some)) {
        if let Some(part) = part {
            path.push(part);
        }
        let metadata = fs::symlink_metadata(&path).map_err(|_| format!("{relative} is missing"))?;
        if metadata.file_type().is_symlink() {
            return Err(format!("{relative} is a symbolic link"));
        }
    }
    if !fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.is_file()) {
        return Err(format!("{relative} is not a regular file"));
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    let file = ahi_unix::directory::Directory::open(root)
        .and_then(|directory| directory.open_file(Path::new(relative)));
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let file = fs::File::open(&path);
    let file = file.map_err(|err| format!("{relative}: cannot read: {err}"))?;
    let metadata = file
        .metadata()
        .map_err(|err| format!("{relative}: cannot stat: {err}"))?;
    if !metadata.is_file() {
        return Err(format!("{relative} is not a regular file"));
    }
    if metadata.len() > MAX_FILE_BYTES {
        return Err(format!(
            "{relative} is {} bytes; the cap is {MAX_FILE_BYTES}",
            metadata.len()
        ));
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|err| format!("{relative}: cannot read: {err}"))?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(format!(
            "{relative} exceeds the cap of {MAX_FILE_BYTES} bytes"
        ));
    }
    String::from_utf8(bytes).map_err(|_| format!("{relative} is not UTF-8"))
}
