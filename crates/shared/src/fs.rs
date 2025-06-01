use std::fmt;
use std::path::{Path, PathBuf};

use anyhow;
use anyhow::Result;

#[derive(Debug, PartialEq, Eq)]
pub enum FilesystemStatus {
    NotFound,
    File,
    Directory,
    Symlink {
        points_to: Option<PathBuf>,
        dangling: bool,
    },
    Other, // sockets, block devices etc.
    Error(String),
}

impl fmt::Display for FilesystemStatus {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            FilesystemStatus::File => write!(f, "file"),
            FilesystemStatus::Directory => write!(f, "directory"),
            FilesystemStatus::Symlink { .. } => write!(f, "symlink"),
            FilesystemStatus::Other => write!(f, "other"),
            FilesystemStatus::NotFound => write!(f, "not found"),
            FilesystemStatus::Error(_) => write!(f, "error"),
        }
    }
}

pub fn symlink_metadata(path: &Path) -> Result<Option<std::fs::Metadata>> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) => Ok(Some(meta)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(anyhow::Error::new(e).context(format!(
            "Failed to get symlink metadata for {}",
            path.display()
        ))),
    }
}

pub fn metadata(path: &Path) -> Result<Option<std::fs::Metadata>> {
    match std::fs::metadata(path) {
        // follows links
        Ok(meta) => Ok(Some(meta)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => {
            Err(anyhow::Error::new(e)
                .context(format!("Failed to get metadata for {}", path.display())))
        }
    }
}

pub fn path_exists(path: &Path) -> bool {
    symlink_metadata(path).map_or(false, |meta_opt| meta_opt.is_some())
}

pub fn is_symlink(path: &Path) -> bool {
    symlink_metadata(path).map_or(false, |meta_opt| {
        meta_opt.map_or(false, |meta| meta.file_type().is_symlink())
    })
}

pub fn is_dir(path: &Path) -> bool {
    metadata(path) // follow links for is_dir check
        .map_or(false, |meta_opt| {
            meta_opt.map_or(false, |meta| meta.is_dir())
        })
}

pub fn is_file(path: &Path) -> bool {
    metadata(path) // follow links for is_file check
        .map_or(false, |meta_opt| {
            meta_opt.map_or(false, |meta| meta.is_file())
        })
}

pub fn read_link(path: &Path) -> Result<PathBuf> {
    std::fs::read_link(path).map_err(|e| {
        anyhow::Error::new(e).context(format!("Failed to read link {}", path.display()))
    })
}

pub fn create_symlink(source: &Path, link: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(source, link).map_err(|e| {
            anyhow::Error::new(e).context(format!(
                "Failed to create symlink from {} to {}",
                source.display(),
                link.display()
            ))
        })
    }
    #[cfg(windows)]
    {
        let source_meta = metadata(source).with_context(|| {
            format!(
                "Failed to get metadata for source path {} before creating symlink",
                source.display()
            )
        })?;

        let result = match source_meta {
            Some(meta) if meta.is_dir() => std::os::windows::fs::symlink_dir(source, link),
            Some(_) => {
                // assume file if not directory
                std::os::windows::fs::symlink_file(source, link)
            }
            None => {
                // source doesn't exist, attempt file symlink creation? Or error out?
                // let's error out for clarity, as Windows requires knowing the type.
                // alternatively, could attempt file link and let it potentially fail if source is later created as dir.
                Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("Source path {} does not exist", source.display()),
                ))
            }
        };

        result.map_err(|e| {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                // provide specific guidance for Windows permission error (OS error 1314 typically)
                handle_windows_symlink_error(e, source, link)
            } else {
                anyhow::Error::new(e).context(format!(
                    "Failed to create symlink from {} to {}",
                    source.display(),
                    link.display()
                ))
            }
        })
    }
    #[cfg(not(any(unix, windows)))]
    {
        Err(anyhow::anyhow!(
            "Symlink creation not supported on this platform"
        ))
    }
}

pub fn remove_file(path: &Path) -> Result<()> {
    std::fs::remove_file(path).map_err(|e| {
        anyhow::Error::new(e).context(format!("Failed to remove file/symlink {}", path.display()))
    })
}

pub fn remove_dir_all(path: &Path) -> Result<()> {
    std::fs::remove_dir_all(path).map_err(|e| {
        anyhow::Error::new(e).context(format!("Failed to remove directory {}", path.display()))
    })
}

pub fn create_dir_all(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path).map_err(|e| {
        anyhow::Error::new(e).context(format!("Failed to create directory {}", path.display()))
    })
}

pub fn write_file(path: &Path, content: &[u8]) -> Result<()> {
    std::fs::write(path, content).map_err(|e| {
        anyhow::Error::new(e).context(format!("Failed to write file {}", path.display()))
    })
}

pub fn read_to_string(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(|e| {
        anyhow::Error::new(e).context(format!("Failed to read file {}", path.display()))
    })
}

pub fn get_status(path: &Path) -> FilesystemStatus {
    match symlink_metadata(path) {
        Ok(Some(meta)) => {
            if meta.is_symlink() {
                match read_link(path) {
                    Ok(target_path) => {
                        // check if the target exists to determine if dangling
                        let dangling = !path_exists(&target_path);
                        FilesystemStatus::Symlink {
                            points_to: Some(target_path),
                            dangling,
                        }
                    }
                    Err(_) => {
                        // couldn't read link target (permissions? or truly dangling)
                        FilesystemStatus::Symlink {
                            points_to: None,
                            dangling: true,
                        }
                    }
                }
            } else if meta.is_dir() {
                FilesystemStatus::Directory
            } else if meta.is_file() {
                FilesystemStatus::File
            } else {
                FilesystemStatus::Other
            }
        }
        Ok(None) => FilesystemStatus::NotFound,
        Err(e) => FilesystemStatus::Error(format!(
            "Error checking status for {}: {}",
            path.display(),
            e
        )),
    }
}

pub fn handle_windows_symlink_error(
    error: std::io::Error,
    source: &Path,
    link: &Path,
) -> anyhow::Error {
    anyhow::Error::new(error).context(format!(
         "Failed to create symlink from {} to {}. This often requires administrator privileges or 'Developer Mode' enabled on Windows.",
         source.display(), link.display()
     ))
}
