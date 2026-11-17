//! Descriptor-relative, no-follow reads on macOS and Linux.
//!
//! This prevents symlink substitution during descendant opens. It is not a
//! sandbox: hard links, mount changes, and renaming an already-open directory
//! are outside this guarantee. The caller selects a trusted root and enforces
//! its own path-based exclusions.

use std::ffi::CString;
use std::fs::{File, OpenOptions};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Component, Path};

#[cfg(target_os = "linux")]
mod flags {
    pub const DIRECTORY: i32 = 0o200000;
    pub const NOFOLLOW: i32 = 0o400000;
    pub const CLOEXEC: i32 = 0o2000000;
    pub const NONBLOCK: i32 = 0o4000;
}
#[cfg(target_os = "macos")]
mod flags {
    pub const DIRECTORY: i32 = 0x100000;
    pub const NOFOLLOW: i32 = 0x100;
    pub const CLOEXEC: i32 = 0x1000000;
    pub const NONBLOCK: i32 = 0x4;
}

#[cfg(test)]
mod tests;

extern "C" {
    fn openat(dirfd: i32, path: *const std::ffi::c_char, flags: i32, ...) -> i32;
}

/// An owned directory descriptor anchoring descendant reads.
#[derive(Debug)]
pub struct Directory(File);

impl Directory {
    /// Open a trusted root. The final component must not be a symbolic link.
    /// Ancestors of this root must be established by the caller.
    pub fn open(root: &Path) -> io::Result<Self> {
        OpenOptions::new()
            .read(true)
            .custom_flags(flags::DIRECTORY | flags::NOFOLLOW | flags::CLOEXEC)
            .open(root)
            .map(Self)
    }

    /// Open a descendant for reading, refusing symlinks at every component.
    /// The caller must inspect metadata before reading; this can return a
    /// directory or another non-regular file.
    /// Absolute paths, parent traversal, empty paths and NULs are rejected.
    /// Nonblocking open prevents a substituted FIFO from hanging before fstat.
    pub fn open_file(&self, relative: &Path) -> io::Result<File> {
        let names: Vec<_> = relative
            .components()
            .map(|component| {
                let Component::Normal(name) = component else {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "expected a relative descendant path",
                    ));
                };
                CString::new(name.as_bytes())
                    .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in path"))
            })
            .collect::<io::Result<_>>()?;
        if names.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "file path is empty",
            ));
        }
        let mut parent = None;
        for (index, name) in names.iter().enumerate() {
            let directory: &File = parent.as_ref().unwrap_or(&self.0);
            let last = index + 1 == names.len();
            let flags = flags::NOFOLLOW
                | flags::CLOEXEC
                | if last {
                    flags::NONBLOCK
                } else {
                    flags::DIRECTORY
                };
            // SAFETY: directory owns a live descriptor for this call; name is
            // NUL-terminated and lives through the call. Flags open read-only
            // and never include O_CREAT, so no variadic mode argument is used.
            let fd = unsafe { openat(directory.as_raw_fd(), name.as_ptr(), flags) };
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: successful openat returned a new, owned descriptor.
            // It is transferred exactly once to File, which closes it on drop.
            let file = unsafe { File::from_raw_fd(fd) };
            if last {
                return Ok(file);
            }
            parent = Some(file);
        }
        unreachable!("nonempty component list returns on its final component")
    }
}
