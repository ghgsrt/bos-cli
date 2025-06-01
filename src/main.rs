use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::prelude::v1::*;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::Deserialize;
use toml::Table;

struct Test {
    ball: Option<Table>,
}

// ~~ CLI ~~

#[derive(Parser)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: CliCommands,
}

#[derive(Subcommand)]
enum CliCommands {
    Init(InitArgs),
    Dots(DotsArgs),
}

#[derive(Args)]
struct InitArgs {
    #[arg(short, long)]
    config: Option<String>,
    #[arg(short, long)]
    os: String,
    #[arg(short, long)]
    system: Option<String>, // only relevant if 'os' is 'guix' or 'nix[os]'
    #[arg(short, long)]
    home: Option<String>, // only relevant if using Guix or Nix
}


#[derive(Deserialize)]
struct BosConfig {
    dotfiles: Option<Vec<DotfileConfig>>,
}

struct BosEnv {
    bos_dir: String,
}

// ~~ DOTS ~~

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

pub trait FileSystemOps: Send + Sync + std::fmt::Debug {
    fn symlink_metadata(&self, path: &Path) -> Result<Option<std::fs::Metadata>>;
    fn metadata(&self, path: &Path) -> Result<Option<std::fs::Metadata>>;
    fn path_exists(&self, path: &Path) -> bool;
    fn is_symlink(&self, path: &Path) -> bool;
    fn is_dir(&self, path: &Path) -> bool;
    fn is_file(&self, path: &Path) -> bool;

    fn read_link(&self, path: &Path) -> Result<PathBuf>;
    fn create_symlink(&self, source: &Path, link: &Path) -> Result<()>;

    fn remove_file(&self, path: &Path) -> Result<()>;
    fn remove_dir_all(&self, path: &Path) -> Result<()>;

    fn create_dir_all(&self, path: &Path) -> Result<()>;
    fn write_file(&self, path: &Path, content: &[u8]) -> Result<()>;
    fn read_to_string(&self, path: &Path) -> Result<String>;

    // Optional: Higher-level status check
    fn get_status(&self, path: &Path) -> FilesystemStatus;
}

pub mod bfs {
    use std::fs::{Path, PathBuf};
    use anyhow::{Result};
    use super::FilesystemStatus;

    fn symlink_metadata(path: &Path) -> Result<Option<std::fs::Metadata>> {
        match std::fs::symlink_metadata(path) {
            Ok(meta) => Ok(Some(meta)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(anyhow::Error::new(e).context(format!(
                "Failed to get symlink metadata for {}",
                path.display()
            ))),
        }
    }

    fn metadata(path: &Path) -> Result<Option<std::fs::Metadata>> {
        match std::fs::metadata(path) {
            // follows links
            Ok(meta) => Ok(Some(meta)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(anyhow::Error::new(e)
                .context(format!("Failed to get metadata for {}", path.display()))),
        }
    }

    fn path_exists(path: &Path) -> bool {
        symlink_metadata(path)
            .map_or(false, |meta_opt| meta_opt.is_some())
    }

    fn is_symlink(path: &Path) -> bool {
        symlink_metadata(path).map_or(false, |meta_opt| {
            meta_opt.map_or(false, |meta| meta.file_type().is_symlink())
        })
    }

    fn is_dir(path: &Path) -> bool {
        metadata(path) // follow links for is_dir check
            .map_or(false, |meta_opt| {
                meta_opt.map_or(false, |meta| meta.is_dir())
            })
    }

    fn is_file(path: &Path) -> bool {
        metadata(path) // follow links for is_file check
            .map_or(false, |meta_opt| {
                meta_opt.map_or(false, |meta| meta.is_file())
            })
    }

    fn read_link(path: &Path) -> Result<PathBuf> {
        std::fs::read_link(path).map_err(|e| {
            anyhow::Error::new(e).context(format!("Failed to read link {}", path.display()))
        })
    }

    fn create_symlink(source: &Path, link: &Path) -> Result<()> {
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

    fn remove_file(path: &Path) -> Result<()> {
        std::fs::remove_file(path).map_err(|e| {
            anyhow::Error::new(e)
                .context(format!("Failed to remove file/symlink {}", path.display()))
        })
    }

    fn remove_dir_all(path: &Path) -> Result<()> {
        std::fs::remove_dir_all(path).map_err(|e| {
            anyhow::Error::new(e).context(format!("Failed to remove directory {}", path.display()))
        })
    }

    fn create_dir_all(path: &Path) -> Result<()> {
        std::fs::create_dir_all(path).map_err(|e| {
            anyhow::Error::new(e).context(format!("Failed to create directory {}", path.display()))
        })
    }

    fn write_file(path: &Path, content: &[u8]) -> Result<()> {
        std::fs::write(path, content).map_err(|e| {
            anyhow::Error::new(e).context(format!("Failed to write file {}", path.display()))
        })
    }

    fn read_to_string(path: &Path) -> Result<String> {
        std::fs::read_to_string(path).map_err(|e| {
            anyhow::Error::new(e).context(format!("Failed to read file {}", path.display()))
        })
    }

    fn get_status(path: &Path) -> FilesystemStatus {
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
}

fn handle_windows_symlink_error(
    error: std::io::Error,
    source: &Path,
    link: &Path,
) -> anyhow::Error {
    anyhow::Error::new(error).context(format!(
         "Failed to create symlink from {} to {}. This often requires administrator privileges or 'Developer Mode' enabled on Windows.",
         source.display(), link.display()
     ))
}

type TrackfileContent = BTreeMap<PathBuf, PathBuf>;

#[derive(Debug, Default)]
pub struct TrackfileState {
    content: TrackfileContent,
    dirty: bool,
}

impl TrackfileState {
    pub fn load(trackfile_path: &Path, fs_ops: &dyn FileSystemOps) -> Result<Self> {
        if let Some(parent) = trackfile_path.parent() {
            fs_ops.create_dir_all(parent).with_context(|| {
                format!("Failed to create cache directory {}", parent.display())
            })?;
        } else {
            return Err(anyhow::anyhow!(
                "Invalid trackfile path with no parent directory: {}",
                trackfile_path.display()
            ));
        }

        match fs_ops.read_to_string(trackfile_path) {
            Ok(toml_content) => {
                if toml_content.trim().is_empty() {
                    Ok(TrackfileState::default())
                } else {
                    let content: TrackfileContent =
                        toml::from_str(&toml_content).with_context(|| {
                            format!("Failed to parse trackfile {}", trackfile_path.display())
                        })?;
                    Ok(Self {
                        content,
                        dirty: false,
                    })
                }
            }
            Err(e) =>
            // Check if the error is NotFound, treat as default state. Propagate other errors.
            // Need to downcast anyhow::Error or match on the underlying io::Error kind if possible.
            // Assuming fs_ops.read_to_string returns std::io::Error wrapped in anyhow::Error
            {
                match e.downcast_ref::<std::io::Error>() {
                    Some(io_err) if io_err.kind() == std::io::ErrorKind::NotFound => {
                        Ok(TrackfileState::default())
                    }
                    _ => Err(e.context(format!(
                        "Failed to read trackfile {}",
                        trackfile_path.display()
                    ))),
                }
            }
        }
    }

    pub fn save(&self, trackfile_path: &Path, fs_ops: &dyn FileSystemOps) -> Result<()> {
        if !self.dirty {
            return Ok(());
        }

        let toml_string = toml::to_string_pretty(&self.content) // [71, 74, 11, 76, 12, 79, 80]
            .context("Failed to serialize trackfile content")?;

        if let Some(parent) = trackfile_path.parent() {
            fs_ops.create_dir_all(parent).with_context(|| {
                format!("Failed to create cache directory {}", parent.display())
            })?;
        } else {
            return Err(anyhow::anyhow!(
                "Invalid trackfile path with no parent directory: {}",
                trackfile_path.display()
            ));
        }

        fs_ops
            .write_file(trackfile_path, toml_string.as_bytes()) // [56, 57, 58, 59, 60]
            .with_context(|| format!("Failed to write trackfile {}", trackfile_path.display()))?;

        // Note: Ideally, 'dirty' should be reset here, requiring &mut self.
        // If save is only called at the end, resetting might not be strictly necessary.
        // For robustness, consider changing signature to `save(&mut self,...)` and setting `self.dirty = false;` here.

        Ok(())
    }

    pub fn add_entry(&mut self, dest: PathBuf, source: PathBuf) {
        self.content.links.insert(dest, source);
        self.dirty = true;
    }

    pub fn remove_entry(&mut self, dest: &Path) -> Option<PathBuf> {
        let removed = self.content.links.remove(dest);
        if removed.is_some() {
            self.dirty = true;
        }
        removed
    }

    pub fn get_source(&self, dest: &Path) -> Option<&PathBuf> {
        self.content.links.get(dest)
    }

    pub fn contains_dest(&self, dest: &Path) -> bool {
        self.content.links.contains_key(dest)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&PathBuf, &PathBuf)> {
        self.content.links.iter()
    }

    // Expose the dirty flag if needed externally (e.g., in main to decide whether to save)
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }
}

pub struct AppContext<'a> {
    args: &'a DotsArgs,
    fs_ops: &'a dyn FileSystemOps,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OperationTarget {
    pub source: PathBuf, // Absolute path to the dotfile
    pub dest: PathBuf,   // Absolute path to the symlink destination
}

fn handle_link(args: &DotsArgs, trackfile: &mut TrackfileState) -> Result<()> {
     let targets = generate_trackfile(, fs_ops).context("Failed to resolve link targets")?;
     if targets.is_empty() {
         println!("No dotfiles found to link based on the provided target and filters.");
         return Ok(());
     }

     println!("Preparing to link {} dotfiles...", targets.len());

     for target in targets {
         let source_path = &target.source;
         let dest_path = &target.dest;

         let fs_status = fs_ops.get_status(dest_path);
         let tracked_source = trackfile.get_source(dest_path);

         // --- Determine Action based on Flags and State ---
         let mut should_remove = false;
         let mut remove_reason = "";
         let mut should_link = false;
         let mut skip_reason = "";

         match fs_status {
             FilesystemStatus::NotFound => {
                 should_link = true;
             }
             FilesystemStatus::File | FilesystemStatus::Directory => {
                 if args.dangerous_force {
                     should_remove = true;
                     remove_reason = "destination exists (--dangerous-force)";
                     should_link = true;
                 } else if args.force && tracked_source.is_some() {
                     should_remove = true;
                     remove_reason = "tracked destination exists (--force)";
                     should_link = true;
                 } else {
                     skip_reason = "destination exists and is not a symlink (use -f or -F to overwrite)";
                 }
             }
             FilesystemStatus::Symlink { points_to: _, dangling: _ } => {
                 match tracked_source {
                     Some(tracked_src) => {
                         // Compare paths carefully (canonicalize if possible, or normalize)
                         // Simple comparison for outline:
                         if tracked_src == source_path {
                             skip_reason = "already linked correctly";
                         } else if args.dangerous_force {
                             should_remove = true;
                             remove_reason = "destination is incorrect symlink (--dangerous-force)";
                             should_link = true;
                         } else if args.force |
| args.replace {
                             should_remove = true;
                             remove_reason = "tracked destination is incorrect symlink (--force or --replace)";
                             should_link = true;
                         } else {
                              skip_reason = "tracked destination is incorrect symlink (use -f, -F, or -r to replace)";
                         }
                     }
                     None => { // Untracked symlink
                         if args.dangerous_force {
                             should_remove = true;
                             remove_reason = "untracked symlink exists (--dangerous-force)";
                             should_link = true;
                         } else {
                             skip_reason = "untracked symlink exists (use -F to overwrite)";
                         }
                     }
                 }
             }
             FilesystemStatus::Other => {
                 if args.dangerous_force {
                    should_remove = true; // Risky! Assumes remove_file/remove_dir_all might work
                    remove_reason = "destination is other file type (--dangerous-force)";
                    should_link = true;
                 } else {
                    skip_reason = "destination exists and is not a regular file/dir/symlink (use -F to overwrite)";
                 }
             }
             FilesystemStatus::Error(e) => {
                 eprintln!("Warning: Could not determine status of {}: {}", dest_path.display(), e);
                 skip_reason = "error checking destination status";
             }
         }

         // --- Apply Skip Logic ---
         if!skip_reason.is_empty() {
             if args.dry_run {
                 println!("DRY RUN: Skip {}: {}", dest_path.display(), skip_reason);
             } else {
                 // Optionally print skip reason verbosely
                 // println!("Skip {}: {}", dest_path.display(), skip_reason);
             }
             continue; // Move to the next target
         }

         // --- Interactive Confirmation ---
         let mut confirmed = true; // Default to true if not interactive
         if (should_remove |
| (should_link && fs_status!= FilesystemStatus::NotFound)) && args.interactive {
             let prompt_msg = if should_remove {
                 format!("Remove existing {} at {}?",
                     match fs_status {
                         FilesystemStatus::Directory => "directory",
                         FilesystemStatus::Symlink{..} => "symlink",
                         _ => "file",
                     },
                     dest_path.display()
                 )
             } else {
                 format!("Overwrite existing item at {} with new link?", dest_path.display())
             };
             confirmed = prompt_confirmation(&prompt_msg)?;
             if!confirmed {
                 println!("Skipping {}", dest_path.display());
                 continue; // Skip this target if user declines
             }
         }

         // --- Dry Run Output ---
         if args.dry_run {
             if should_remove {
                 println!("DRY RUN: Would remove {} at {}", match fs_status {
                         FilesystemStatus::Directory => "directory",
                         FilesystemStatus::Symlink{..} => "symlink",
                         _ => "file",
                     }, dest_path.display());
             }
             if should_link {
                 println!("DRY RUN: Would create symlink {} -> {}", dest_path.display(), source_path.display());
                 println!("DRY RUN: Would add entry to trackfile: {} -> {}", dest_path.display(), source_path.display());
             }
             continue; // Move to the next target
         }

         // --- Perform Actions ---
         if confirmed {
             // 1. Removal
             if should_remove {
                 match fs_status {
                     FilesystemStatus::File | FilesystemStatus::Symlink {..} | FilesystemStatus::Other => {
                         fs_ops.remove_file(dest_path)
                            .with_context(|| format!("Failed to remove existing item at {}", dest_path.display()))?;
                         println!("Removed existing item at {}", dest_path.display());
                     }
                     FilesystemStatus::Directory => {
                         fs_ops.remove_dir_all(dest_path)
                            .with_context(|| format!("Failed to remove existing directory at {}", dest_path.display()))?;
                         println!("Removed existing directory at {}", dest_path.display());
                     }
                     _ => {} // NotFound or Error cases already handled or skipped
                 }
             }

             // 2. Link Creation
             if should_link {
                 // Ensure parent directory exists
                 if let Some(parent) = dest_path.parent() {
                     fs_ops.create_dir_all(parent)
                        .with_context(|| format!("Failed to create parent directory for {}", dest_path.display()))?;
                 }

                 // Create the symlink
                 fs_ops.create_symlink(source_path, dest_path)
                    .with_context(|| format!("Failed to create symlink {} -> {}", dest_path.display(), source_path.display()))?;
                 println!("Linked {} -> {}", dest_path.display(), source_path.display());

                 // 3. Update Trackfile
                 trackfile.add_entry(dest_path.to_path_buf(), source_path.to_path_buf());
             }
         }
     }

     Ok(())
 }

fn traverse_all<F>(current_dir: &Path, process_leaf: F) -> io::Result<()>
where
    F: Fn(&Path) -> Option<PathBuf>,
{
    for entry_result in fs::read_dir(current_dir)? {
        let entry = entry_result?;
        let path = entry.path();
        let metadata = fs::metadata(&path)?;

        if metadata.is_file() {
            process_leaf(&path);
        } else if metadata.is_dir() {
            traverse_all(&path, &process_leaf)?;
        }
        // Implicitly ignore symlinks, other types
    }

    Ok(())
}

fn traverse_with_exclusions<F>(
    current_dir: &Path,
    exclusions: &Vec<PathBuf>,
    process_leaf: F,
) -> io::Result<()>
where
    F: Fn(&Path) -> Option<PathBuf>,
{
    if exclusions.contains(&current_dir.to_path_buf()) {
        println!("  - Skipping excluded directory: {}", current_dir.display());
        return Ok(());
    }

    for entry_result in fs::read_dir(current_dir)? {
        let entry = entry_result?;
        let path = entry.path();
        let metadata = fs::metadata(&path)?;

        if metadata.is_file() {
            process_leaf(&path);
        } else if metadata.is_dir() {
            traverse_with_exclusions(&path, exclusions, &process_leaf)?;
        }
    }

    Ok(())
}

const DOTFILE_START_DIRS: [&str; 6] = ["home", "root", "os", "user", "guix", "nix"];

fn generate_trackfile(config: String, env: BosEnv) -> Result<DotfileTrackfile> {
    let contents = fs::read_to_string(config).expect("Unable to read config");

    let value: BosConfig = toml::from_str(&contents).expect("oopsies");
    let dotfiles = value.dotfiles;

    if let None = dotfiles {
        println!("No dotfiles provided...");
        return None;
    }

    let trackfile: DotfileTrackfile = HashMap::new();

    for dotfile in dotfiles.unwrap().into_iter() {
        let base_path = Path::new(&dotfile.path);
        let path = if base_path.is_absolute() {
            base_path.to_path_buf()
        } else {
            Path::new(&env.bos_dir).join(base_path)
        };

        let replace_when_merge = dotfile.replace.unwrap_or(true);

        let process_leaf = |leaf: &Path| trackfile.insert(leaf.to_path_buf(), path.to_path_buf());

        let start_dirs = DOTFILE_START_DIRS.map(|dir| path.join(dir));

        let path_meta = fs::metadata(path).unwrap();

        if path_meta.is_dir() {
            // build trackfile
            if let Some(inc) = dotfile.includes {
                for item in inc.into_iter() {
                    let path_to_include = path.join(item);
                    if path_to_include.is_dir() {
                        traverse_all(&path_to_include, &process_leaf)?;
                    } else if path_to_include.is_file() {
                        process_leaf(&path_to_include)
                    }
                }
            } else if let Some(exc) = dotfile.excludes {
                let full_exc = exc.into_iter().map(|e| path.join(e)).collect();
                for item in start_dirs.iter() {
                    traverse_with_exclusions(item, full_exc, &process_leaf)?;
                }
            } else {
                for item in start_dirs.iter() {
                    traverse_all(item, &process_leaf)?;
                }
            }
        } else {
            // merge trackfiles
            let sub_trackfile = generate_trackfile(dotfile.path, env)?;

            if let Some(inc) = dotfile.includes {
                for key in inc.into_iter() {
                    if sub_trackfile.contains_key(&key)
                        && (replace_when_merge || !trackfile.contains_key(&key))
                    {
                        trackfile.insert(key, sub_trackfile.get(&key));
                    }
                }
            } else if let Some(exc) = dotfile.excludes {
                for (key, value) in sub_trackfile {
                    if !exc.any(|prefix| key.starts_with(prefix))
                        && (replace_when_merge || !trackfile.contains_key(&key))
                    {
                        trackfile.insert(key, value)
                    }
                }
            } else {
                for (key, value) in sub_trackfile {
                    if dotfile.replace || !trackfile.contains_key(&key) {
                        trackfile.insert(key, value);
                    }
                }
            }
        }
    }

    Some(trackfile)
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let cache_dir =
        dirs::cache_dir() // [15]
            .ok_or_else(|| anyhow::anyhow!("Could not determine user cache directory"))?
            .join("dots_tool"); // Tool-specific subdirectory

    let trackfile_path = cache_dir.join("trackfile.toml");

    let fs_ops = RealFileSystem::default();

    let mut trackfile_state =
        TrackfileState::load(&trackfile_path, &fs_ops).context("Failed to load trackfile state")?;

    let dry_run_active = match &cli.command {
        DotsCommands::Link(args) => {
            handle_link(args, &fs_ops, &mut trackfile_state).context("Link operation failed")?;
            args.dry_run
        }
        DotsCommands::Unlink(args) => {
            handle_unlink(args, &fs_ops, &mut trackfile_state)
                .context("Unlink operation failed")?;
            args.dry_run
        }
        DotsCommands::Relink(args) => {
            handle_relink(args, &fs_ops, &mut trackfile_state)
                .context("Relink operation failed")?;
            args.dry_run
        }
        DotsCommands::Status(args) => {
            handle_status(args, &fs_ops, &trackfile_state).context("Status operation failed")?;
            false // Status is never dry-run in the modifying sense
        }
        DotsCommands::Clean(args) => {
            handle_clean(args, &fs_ops, &mut trackfile_state).context("Clean operation failed")?;
            args.dry_run
        }
    };

    if trackfile_state.is_dirty() && !dry_run_active {
        trackfile_state
            .save(&trackfile_path, &fs_ops)
            .context("Failed to save trackfile state")?;
        println!("Trackfile saved to {}", trackfile_path.display());
    } else if trackfile_state.is_dirty() && dry_run_active {
        println!("DRY RUN: Trackfile would have been saved.");
    }

    Ok(())
}
