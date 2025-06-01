use std::collections::HashMap;
use std::fs;
use std::iter::Peekable;
use std::path::{Components, Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use git2::Repository;
use toml;
use url::Url;

use crate::{DotfileConfig, TomlConfig};
use shared::bos;
use shared::fs as sfs;

const DOTFILE_START_DIRS: [&str; 6] = ["home", "root", "os", "user", "guix", "nix"];

pub fn change_file_prefix(old_prefix: &Path, new_prefix: &Path, path: &Path) -> PathBuf {
    new_prefix.join(path.strip_prefix(old_prefix)).to_path_buf()
}
//pub fn bc_i_want_to<P, F>(handle_part: P, handle_err: Err) -> F
//where
//    P: Fn(&Component) -> Result<Path>,
//    F: Fn(&Components) -> Result<Path>,
//{
//    |parts: &Components| -> Result<Path> {
//        match parts.next() {
//            Some(part) => handle_part(&part),
//            None => handle_err(None),
//        }
//    }
//}
//
// start by looking in root
//  then home
// then determine os, look in os folder
//      root then home
// then determine user
//      root then home
// then determine home manager(s)
//      root then home
// then use manual mappings

pub type TrackfileContent = HashMap<PathBuf, PathBuf>; // dest, source

#[derive(Debug, Default)]
pub struct Trackfile {
    content: TrackfileContent,
    dirty: bool,
}

// Context struct as a global singelton?
// come up with nice pattern for merging context from bos?
#[derive(Default)]
pub struct TrackfileGenOptions {
    verbose: bool,
}

#[derive(Default)]
pub struct TomlSingleton {
    global: TomlConfig,
    templates: HashMap<&str, TomlConfig>,
}
const TEST: TomlSingleton = TomlSingleton::default();

impl Trackfile {
    pub fn load(trackfile_path: &Path, env: &bos::Env) -> Result<Self> {
        if let Some(parent) = trackfile_path.parent() {
            sfs::create_dir_all(parent).with_context(|| {
                format!("Failed to create cache directory {}", parent.display())
            })?;
        } else {
            return Err(anyhow!(
                "Invalid trackfile path with no parent directory: {}",
                trackfile_path.display()
            ));
        }

        match sfs::read_to_string(trackfile_path) {
            Ok(toml_content) => {
                if toml_content.trim().is_empty() {
                    Ok(Trackfile::default())
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
            // Assuming sfs::read_to_string returns std::io::Error wrapped in anyhow::Error
            {
                match e.downcast_ref::<std::io::Error>() {
                    Some(io_err) if io_err.kind() == std::io::ErrorKind::NotFound => {
                        Ok(Trackfile::default())
                    }
                    _ => Err(e.context(format!(
                        "Failed to read trackfile {}",
                        trackfile_path.display()
                    ))),
                }
            }
        }
    }

    pub fn save(&mut self, trackfile_path: &Path, env: bos::Env) -> Result<()> {
        if !self.dirty {
            return Ok(());
        }

        let toml_string = toml::to_string_pretty(&self.content)
            .context("Failed to serialize trackfile content")?;

        if let Some(parent) = trackfile_path.parent() {
            sfs::create_dir_all(parent).with_context(|| {
                format!("Failed to create cache directory {}", parent.display())
            })?;
        } else {
            return Err(anyhow!(
                "Invalid trackfile path with no parent directory: {}",
                trackfile_path.display()
            ));
        }

        sfs::write_file(trackfile_path, toml_string.as_bytes())
            .with_context(|| format!("Failed to write trackfile {}", trackfile_path.display()))?;

        self.dirty = false;

        Ok(())
    }

    // config is either
    // 1. a url (git)
    // 2. a path to a file (config toml)
    // 3. a path to a directory (dotfiles atom)
    pub fn generate(target: String, env: &bos::Env, opts: Option<TrackfileGenOptions>) {
        let opts = opts
            .or_else(|| Some(TrackfileGenOptions::default()))
            .unwrap();

        // try git
        if let Ok(url) = Url::parse(target.as_str()) {
            Repository::clone_recurse(url, PathBuf::from(env.cache_dir).join("repos"))
        }
    }

    // Let's say we have a global config then are pointed at a directory:
    // Grab the global config as initial config state
    // enter dir
    // if !isolate, check if a dir config exists
    //  true:
    //      read dots section
    //      inherit from config state as specified
    //      go through dotfiles and do as specified
    //      Only for config dirs!: if none of the dotfiles specified "." as path:
    //          implicit "."
    //  false:
    //      bro gets pick o' da crop
    pub fn generate_from_config(
        loc: &PathBuf,
        env: &bos::Env,
        opts: Option<TrackfileGenOptions>,
    ) -> Result<Option<Self>> {
        let dir_config = Trackfile::detect_config(loc)?;
        if let None = dir_config {
            return Ok(None);
        }

        // handle inheritance
        // process dots.use to build out full includes list
        //   1. when
        //   2. self.excludes (path relative to use path)
        //   3. dots.excludes (path relative to dir path; can't use templating e.g., @, <name>)
        //   4. use dots.map for manual mapping
        // loop through dotfiles
        //   1. general generate on path, passing in the relevant opts
        //   2. take generated trackfile and apply dotfile.use
        //   3. use dotfile.map to essentially remap certain files
        //   - anything not found in the generated track is just skipped
        //
        //   So really, seems like the concicest approach is to simply (given a "dotfile"):
        //   1. handle inheritance
        //   2. generate track as if no config
        //   3. apply config-based transformations or otherwise return track as is to caller
        //   - we can figure out a way to improve performance after we see how this looks (prolly
        //   easier to figure out then)
        //   Might wanna try to figure out if having a callee config inherit from the caller would
        //   actually be equivalent to use_config on callee then processed by caller again
        //   - I think only difference should be items which may not have been accessible could
        //   become accessible
        //   - Doing a strict inheritance where the caller can only extend use for paths that are
        //   already specified in the callee may be sufficient
        //   - Maybe to just avoid having to care, just always do two steps
        //     Unless not using callee config, in which case we could supply the caller config in
        //     its place
    }

    // expects config.dotfiles to be empty (MAKE SURE THIS IS ACTUALLY WHAT WE WANT)
    pub fn generate_from_dir(
        loc: &PathBuf,
        env: &bos::Env,
        opts: Option<TrackfileGenOptions>,
    ) -> Result<Self> {
        if !loc.try_exists()? {
            return Err(anyhow!("target directory {} does not exist", loc.display()));
        }
        let opts = opts.unwrap_or_else(|| TrackfileGenOptions::default());

        Ok(
            Trackfile::generate_from_config(loc, env, Some(opts))?.or_else(|| {
                // da whole crop
            }),
        )

        //let state = if opts.use_config {
        //    let dir_config = Trackfile::detect_config(&loc)?;
        //    if opts.isolate || dir_config.is_some_and(|c| c.dots.is_some_and(|d| d.inherits == )) {
        //        Toml::default().extend(dir_config)
        //    } else {
        //        config.clone().extend(dir_config) // extend should handle the inherit
        //    }
        //} else {
        //    config
        //};
        //
    }
    pub fn process_dotfile(state: &TomlConfig, dotfile_config: &DotfileConfig) -> Self {}
    pub fn detect_config(target_dir: &PathBuf) -> Result<Option<TomlConfig>> {
        let config_names = vec![
            "dots.toml",
            "config.dots",
            ".dots",
            "bos.toml",
            "config.bos",
            ".bos",
        ];

        for name in config_names.into_iter() {
            let target = target_dir.join(name);
            if target.try_exists()? {
                return Ok(Some(TomlConfig::read(target)?));
            }
        }

        Ok(None)
    }

    pub fn insert(&mut self, dest: PathBuf, source: PathBuf) {
        self.content.insert(dest, source);
        self.dirty = true;
    }

    pub fn remove(&mut self, dest: &Path) -> Option<PathBuf> {
        let removed = self.content.remove(dest);
        if removed.is_some() {
            self.dirty = true;
        }
        removed
    }

    pub fn get_source(&self, dest: &Path) -> Option<&PathBuf> {
        self.content.get(dest)
    }

    pub fn contains_dest(&self, dest: &PathBuf) -> bool {
        self.content.contains_key(dest)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&PathBuf, &PathBuf)> {
        self.content.iter()
    }

    pub fn into_iter(self) -> impl Iterator<Item = (PathBuf, PathBuf)> {
        self.content.into_iter()
    }

    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }

    pub fn len(&self) -> usize {
        self.content.len()
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }
}

//fn traverse_all<F>(path: &Path, process_leaf: F) -> io::Result<()>
//where
//    F: Fn(&Path),
//{
//    let metadata = fs::metadata(path)?;
//    if metadata.is_file() {
//        process_leaf(path);
//    } else if metadata.is_dir() {
//        for entry_result in fs::read_dir(path)? {
//            traverse_all(&entry_result?.path(), process_leaf)?;
//        }
//    }
//    // Implicitly ignore symlinks, other types
//
//    Ok(())
//}

fn traverse<F>(
    path: &Path,
    process_leaf: F,
    exclusions: &Vec<PathBuf>,
    verbose: bool,
) -> io::Result<()>
where
    F: Fn(&Path),
{
    if let Some(exc) = exclusions
        && exc.contains(&path.to_path_buf())
    {
        if verbose {
            println!("[ EXCLUDED ] Skipping: {}", path.display());
        }
        return Ok(());
    }

    if metadata.is_file() {
        process_leaf(&path);
    } else if metadata.is_dir() {
        for entry_result in fs::read_dir(path)? {
            traverse(&entry_reult?.path(), exclusions, &process_leaf, verbose)?;
        }
    }

    Ok(())
}
