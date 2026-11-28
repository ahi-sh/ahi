use std::fs;
use std::io::Read;
use std::path::Path;

use crate::error::{Error, Result};

const MAX_INSTRUCTION_BYTES: u64 = 64 * 1024;

pub(crate) struct Instructions {
    guidance: String,
}

impl Instructions {
    pub(crate) fn load(directory: &Path, operator_home: Option<&Path>) -> Result<Self> {
        let repository = repository_root(directory)?;
        let mut text = String::new();
        if let Some(home) = operator_home {
            append_file(&mut text, "Operator", &home.join(".agents/AGENTS.md"), true)?;
        }
        append_file(
            &mut text,
            "Repository",
            &repository.join("AGENTS.md"),
            false,
        )?;
        Ok(Self { guidance: text })
    }

    pub(crate) fn for_turn(self, mode: &str, workspace: &Path, has_tools: bool) -> String {
        let tools = if has_tools {
            "Available tools are listed in the schemas. Runtime policy checks every call."
        } else {
            "No tools available."
        };
        let mut instructions = format!(
            "{}\n\nMode: {}. Workspace: {}. {tools}",
            super::CORE,
            serde_json::json!(mode),
            serde_json::json!(workspace.to_string_lossy())
        );
        if !self.guidance.is_empty() {
            instructions.push_str("\n\nWithin the core instructions and enforced policy, direct operator requests take precedence. For project matters, repository guidance takes precedence over operator defaults.\n");
            instructions.push_str(&self.guidance);
        }
        instructions
    }
}

fn repository_root(directory: &Path) -> Result<&Path> {
    let git_directory =
        match git2::Repository::discover_path(directory, std::iter::empty::<&Path>()) {
            Ok(path) => path
                .canonicalize()
                .map_err(|_| Error::policy("cannot resolve the Git metadata directory"))?,
            Err(error) if error.code() == git2::ErrorCode::NotFound => return Ok(directory),
            Err(_) => {
                return Err(Error::policy(
                    "cannot discover the repository instruction directory",
                ))
            }
        };
    for ancestor in directory.ancestors() {
        if ancestor == git_directory {
            break;
        }
        let marker = ancestor.join(".git");
        // Discovery stops at the first .git file and validates its target.
        // Its parent is the worktree even when the metadata lives elsewhere.
        if fs::metadata(&marker).is_ok_and(|metadata| {
            metadata.is_file()
                || marker
                    .canonicalize()
                    .is_ok_and(|path| path == git_directory)
        }) {
            return Ok(ancestor);
        }
    }
    Ok(directory)
}

fn append_file(text: &mut String, scope: &str, path: &Path, operator: bool) -> Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(invalid_file(path, "cannot inspect the file")),
    };
    if !operator && metadata.file_type().is_symlink() {
        return Err(invalid_file(
            path,
            "repository instruction file is a symbolic link",
        ));
    }
    if !metadata.is_file() && !metadata.file_type().is_symlink() {
        return Err(invalid_file(
            path,
            "instruction file must be a regular file",
        ));
    }
    let file = if operator {
        let resolved = path
            .canonicalize()
            .map_err(|_| invalid_file(path, "cannot resolve the operator instruction file"))?;
        if !fs::metadata(&resolved).is_ok_and(|metadata| metadata.is_file()) {
            return Err(invalid_file(
                path,
                "instruction file must be a regular file",
            ));
        }
        open_file(&resolved, path)?
    } else {
        open_file(path, path)?
    };
    let metadata = file
        .metadata()
        .map_err(|_| invalid_file(path, "cannot inspect the opened file"))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(invalid_file(
            path,
            "instruction file must be a regular file",
        ));
    }
    if metadata.len() > MAX_INSTRUCTION_BYTES {
        return Err(invalid_file(path, "instruction file exceeds 64 KiB"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_INSTRUCTION_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid_file(path, "cannot read the file"))?;
    if bytes.len() as u64 > MAX_INSTRUCTION_BYTES {
        return Err(invalid_file(path, "instruction file exceeds 64 KiB"));
    }
    let contents = String::from_utf8(bytes)
        .map_err(|_| invalid_file(path, "instruction file is not UTF-8"))?;
    text.push_str(&format!(
        "\n{scope} AGENTS.md instructions from {}:\n\n{contents}\n",
        serde_json::json!(path.to_string_lossy())
    ));
    Ok(())
}

fn invalid_file(path: &Path, reason: &str) -> Error {
    Error::policy(format!("{}: {reason}", path.display()))
}

fn open_file(path: &Path, source: &Path) -> Result<fs::File> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    let file = {
        let directory = path
            .parent()
            .ok_or_else(|| invalid_file(source, "missing instruction directory"))?;
        let name = path
            .file_name()
            .ok_or_else(|| invalid_file(source, "missing instruction file name"))?;
        ahi_unix::directory::Directory::open(directory)
            .and_then(|directory| directory.open_file(Path::new(name)))
    };
    #[cfg(windows)]
    let file = {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x00200000;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x02000000;
        fs::OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
            .open(path)
    };
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    let file = fs::File::open(path);
    file.map_err(|_| invalid_file(source, "cannot open the instruction file"))
}
