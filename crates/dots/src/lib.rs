use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use std::io;
use std::io::Write;
use std::os::unix::fs::DirEntryExt2;
use std::path::{Path, PathBuf};
use std::prelude::v1::*;
use std::process::Command;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use derive_builder::Builder;
use serde::Deserialize;
use serde::{
    de::{Error as SerdeError, MapAccess, SeqAccess, Visitor},
    Deserializer,
};
use toml::Table;

use shared::bos;

use crate::handlers::*;
use crate::staging::*;
use crate::trackfile::*;

mod handlers;
pub mod staging;
pub mod trackfile; // TEMPORARY (for staging changes as part of large refactors bc whynot)

#[derive(Deserialize, Debug, Clone)]
struct RawUseTable<'de> {
    // `when` can be absent, or a table.
    // Using a custom deserializer for Option ensures absence is None,
    // and presence (even `when={}`) deserializes TomlUseWhen.
    #[serde(default, deserialize_with = "deserialize_optional_raw_use_when")]
    when: Option<RawUseWhen<'de>>,

    #[serde(borrow)]
    target: Option<RawUseTargetType<'de>>,

    #[serde(borrow)]
    exclude: Option<RawExcludeType<'de>>,

    // Captures all other keys (like "name", "dir", etc.) into a map.
    // These will form the basis of `replace_map`.
    #[serde(flatten, borrow)]
    vars: HashMap<String, RawUseVarType<'de>>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
enum RawUseWhenType<'de> {
    Bool(&'de bool),
    Table(RawUseWhen<'de>),
}

// For the `when` field: { shell: string, if: string, command: string }
#[derive(Deserialize, Debug, Default, Clone, Copy)] // Copy if all fields are simple borrowed strs
struct RawUseWhen<'de> {
    #[serde(borrow)]
    shell: Option<&'de str>,
    #[serde(rename = "if", borrow)]
    test_if: Option<&'de str>,
    #[serde(borrow)]
    test_command: Option<&'de str>,
}

// For the `target` field: string OR table {key: value}
#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
enum RawUseTargetType<'de> {
    Str(&'de str),
    Table(HashMap<String, &'de str>),
}

// For the `exclude` field: string OR vec of strings
#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
enum RawExcludeType<'de> {
    Str(&'de str),
    Vec(Vec<&'de str>),
}

// For values in the `vars` map (e.g., "name" = "$BOS_OS" OR "name" = {env = "$BOS_OS"})
#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
enum RawUseVarType<'de> {
    Str(&'de str),         // Simple string value
    Table(RawUseVar<'de>), // Table like {env="...", value="...", shell="..."}
}

#[derive(Deserialize, Debug, Default, Clone, Copy)]
struct RawUseVar<'de> {
    #[serde(borrow)]
    shell: Option<&'de str>,
    #[serde(borrow)]
    env: Option<&'de str>,
    #[serde(borrow)]
    value: Option<&'de str>,
}

// This enum helps deserialize the varied values of the `dots.use` map
#[derive(Deserialize, Debug)]
#[serde(untagged)]
enum RawUseTableType<'de> {
    Str(&'de str),
    Bool(&'de bool),
    #[serde(borrow)]
    Table(RawUseTable<'de>), // A single UseTable
    #[serde(borrow)]
    Array(Vec<RawUseTable<'de>>), // An array of UseTables
}

fn deserialize_dots_use_map<'de, D>(
    deserializer: D,
) -> Result<Option<HashMap<PathBuf, Vec<DotsUse>>>, D::Error>
where
    D: Deserializer<'de>,
{
    // Deserialize the entire `dots.use` table into a map of keys to our intermediate source value
    let raw_map_option: Option<HashMap<PathBuf, RawUseTableType<'de>>> =
        Option::deserialize(deserializer)?;

    match raw_map_option {
        None => Ok(None), // If `dots.use` is not present at all
        Some(raw_map) => {
            let mut final_map = HashMap::new();

            for (key, source_value) in raw_map {
                let mut current_key_dots_use_list: Vec<DotsUse> = Vec::new();

                match source_value {
                    RawUseTableType::Str(s_val) => {
                        // Str is shorthand for a DotsUse with only 'target' set.
                        if s_val.is_empty() {
                            return Err(D::Error::custom("target cannot be empty"));
                        }
                        current_key_dots_use_list.push(DotsUse {
                            when: true,
                            target: s_val,
                        })
                    }
                    RawUseTableType::Bool(b_val) => {
                        // Boolean is shorthand for a DotsUse with only 'when' set.
                        // If 'when' is true, add it; otherwise, it's filtered.
                        if b_val {
                            current_key_dots_use_list.push(DotsUse {
                                when: true,
                                target: None,
                                exclude: None,
                                replace_map: HashMap::new(),
                            });
                        }
                        // If b_val is false, current_key_dots_use_list remains empty for this shorthand.
                    }
                    RawUseTableType::Table(raw_table) => {
                        // Convert the single RawUseTable.
                        // The convert function will handle internal 'when' evaluation.
                        match convert_raw_table_to_dots_use(raw_table) {
                            Ok(dots_use) => {
                                if dots_use.when {
                                    // Filter based on the evaluated 'when'
                                    current_key_dots_use_list.push(dots_use);
                                }
                            }
                            Err(e) => return Err(D::Error::custom(e)), // Propagate errors
                        }
                    }
                    RawUseTableType::Array(raw_table_array) => {
                        for raw_table in raw_table_array {
                            match convert_raw_table_to_dots_use(raw_table) {
                                Ok(dots_use) => {
                                    if dots_use.when {
                                        // Filter based on evaluated 'when'
                                        current_key_dots_use_list.push(dots_use);
                                    }
                                }
                                Err(e) => return Err(D::Error::custom(e)), // Propagate errors
                            }
                        }
                    }
                }

                // Only add the key to the final_map if there are any 'true' DotsUse entries.
                if !current_key_dots_use_list.is_empty() {
                    final_map.insert(key, current_key_dots_use_list);
                }
            }
            Ok(Some(final_map))
        }
    }
}

fn convert_raw_table_to_dots_use(raw_table: RawUseTable) -> Result<DotsUse> {
    // 1. Process `vars` (RawUseVarType) into `replace_map: HashMap<String, String>`
    let mut replace_map = HashMap::new();
    for (key, raw_var_type) in raw_table.vars {
        let value_str = match raw_var_type {
            RawUseVarType::Str(s) => {
                if s.starts_with("$") || s.starts_with("~") {
                    shell::echo(s.to_string(), None)?
                } else {
                    s
                }
            }
            RawUseVarType::Table(raw_use_var) => {
                let mut defined_fields = 0;
                let mut final_val: Option<String> = None;

                if let Some(v) = raw_use_var.value {
                    final_val = v;
                    defined_fields += 1;
                }
                if let Some(e) = raw_use_var.env {
                    final_val = shell::echo(e.to_string(), None)?;
                    defined_fields += 1;
                }
                if let Some(s) = raw_use_var.shell {
                    final_val = shell::run(s.to_string(), None)?;
                    defined_fields += 1;
                }

                if defined_fields > 1 {
                    return Err(format!(
                        "Multiple fields defined in UseVar for key '{}'",
                        key
                    ));
                }
                final_val.ok_or_else(|| format!("No value found in UseVar for key '{}'", key))?
            }
        };
        replace_map.insert(key, value_str);
    }

    // 2. Process `when: Option<RawUseWhen>` into `when: bool`
    let final_when = match raw_table.when {
        None => true, // `when` omitted defaults to true
        Some(RawUseWhenType::Bool(w)) => w,
        Some(RawUseWhenType::Table(w)) => {
            let shell = if let Some(s) = w.shell {
                shell::run_for_bool(s.to_string(), Some(&replace_map))?
            } else {
                true
            };

            let test_if = if let Some(i) = w.test_if {
                shell::test_if(i.to_string(), Some(&replace_map))?
            } else {
                true
            };

            let test_command = if let Some(c) = w.test_command {
                shell::test_command(c.to_string(), Some(&replace_map))?
            } else {
                true
            };

            shell && test_if && test_command
        }
    };

    // 3. Process `target: Option<RawUseTargetType>` into `Option<Vec<(PathBuf, PathBuf)>>`
    let final_target = match raw_table.target {
        None => None,
        Some(RawUseTargetType::Str(s)) => Some(vec![(PathBuf::from(""), PathBuf::from(s))]),
        Some(RawUseTargetType::Table(map)) => Some(
            map.into_iter()
                .map(|(k, v)| (PathBuf::from(k), PathBuf::from(v)))
                .collect(),
        ),
    };

    // 4. Process `exclude: Option<RawExcludeType>` into `Option<Vec<String>>`
    let final_exclude = match raw_table.exclude {
        None => None,
        Some(RawExcludeType::Str(s)) => Some(vec![s.to_string()]),
        Some(RawExcludeType::Vec(v)) => Some(v.into_iter().map(String::from).collect()),
    };

    Ok(DotsUse {
        when: final_when,
        target: final_target,
        exclude: final_exclude,
        replace_map,
    })
}

// ================================================================================================

#[derive(clap::Args)]
pub struct Args {
    #[command(subcommand)]
    command: Commands,
    dotfiles: Option<String>, // only optional if ran before
}

#[derive(Subcommand)]
pub enum Commands {
    Link(DotsLinkArgs),
    Unlink(DotsLinkArgs),
    Relink(DotsLinkArgs),
    Status(DotsStatusArgs),
    Clean(DotsCleanArgs),
}

#[derive(clap::Args)]
pub struct DotsLinkArgs {
    target: PathBuf,

    /// Include only these glob patterns
    #[arg(short, long)]
    include: Option<Vec<String>>,

    /// Exclude these glob patterns
    #[arg(short, long)]
    exclude: Option<Vec<String>>,

    /// Replace existing tracked symlinks only
    #[arg(short, long)]
    force_symlink: Option<bool>,

    /// Force overwrite/removal of tracked files/links (symlinks or regular files/dirs)
    #[arg(short, long)]
    force_file: Option<bool>,

    /// Force overwrite/removal of *any* destination path, tracked or not (USE WITH CAUTION)
    #[arg(short, long)]
    force_dangerously: Option<bool>,

    /// Perform a dry run, showing actions without modifying filesystem or trackfile
    #[arg(long)]
    dry_run: bool,

    /// Prompt for confirmation before potentially destructive actions
    #[arg(long)]
    interactive: bool,

    #[arg(short, long)]
    verbose: bool,
}

#[derive(clap::Args)]
pub struct DotsStatusArgs {}

#[derive(clap::Args)]
pub struct DotsCleanArgs {
    /// Perform a dry run, showing actions without modifying filesystem or trackfile
    #[arg(long)]
    dry_run: bool,

    /// Prompt for confirmation before potentially destructive actions
    #[arg(short, long)]
    interactive: bool,
}

pub enum Inheritable {
    Use,
    UseTarget,
    Exclude,
}

fn _resolve_path(
    base_path: &Path,
    path: &Path,
    parts: Components,
    env: &HashMap<String, String>,
) -> Result<Vec<(PathBuf, &HashMap<String, String>)>> {
    let mut new_path = PathBuf::new();

    if let Some(part) = parts.next() {
        match part {
            Component::Normal(os_str) => {
                if let Some(s) = os_str.to_str() {
                    if s == "*" {
                        let paths: Vec<(PathBuf, &HashMap<String, String>)> = Vec::new();

                        for entry in fs::read_dir(base_path.join(path))? {
                            let entry = entry?;
                            if !fs::metadata(entry.path())?.is_dir() {
                                continue;
                            }

                            paths.append(&mut _resolve_path(
                                base_path,
                                &path.join(entry.file_name_ref()),
                                parts,
                                env,
                            )?)
                        }

                        return Ok(paths);
                    }

                    if s.starts_with('<') && s.ends_with('>') {
                        let key = &s[1..s.len() - 1];
                        if let Some(value) = env.get(key) {
                            if value == "*" {
                                let paths: Vec<(PathBuf, &HashMap<String, String>)> = Vec::new();

                                for entry in fs::read_dir(base_path.join(path))? {
                                    let entry = entry?;
                                    if !fs::metadata(entry.path())?.is_dir() {
                                        continue;
                                    }

                                    let dir = entry.file_name_ref();

                                    // basically
                                    // copy env
                                    // change env value to the dir name
                                    // pass that new env in instead
                                    // then right before returning a given atomic string,
                                    // use that env to process target suffix (which may also hit
                                    // this branch and alter env if a certain key hadn't been set
                                    // by the primary path)
                                    // then use what should be a complete env to process the actual
                                    // target
                                    // Note that the actual target probably shouldn't ever allow *
                                    // And so basically the function should probably return the
                                    // current env ref along with a given pathbuf
                                    // then in the non_ variant, we append each suffix and process
                                    // each resulting primary path, then take the altered env and
                                    // process the target corresponding to each suffix
                                    //
                                    // crazy/<dir>/ball/home -> e.g., resolves env with dir = "test"
                                    // then process "~/<dir>/ball"
                                    // gives:
                                    // crazy/test/ball/home -> /home/usr/test/ball

                                    let new_env = env.clone();
                                    new_env.insert(key, dir.to_str());

                                    paths.append(&mut _resolve_path(
                                        base_path,
                                        &path.join(entry.file_name_ref()),
                                        parts,
                                        &new_env,
                                    ))
                                }

                                return Ok(paths);
                            } else {
                                _resolve_path(base_path, &path.join(value), parts, env)
                            }
                        } else {
                            _resolve_path(base_path, &path.join(s), parts, env)
                        }
                    } else if s.starts_with('$') || s.starts_with('~') {
                        let part = shell::echo(s, Some(env)).or(s);
                        _resolve_path(base_path, &path.join(part), parts, env)
                    } else {
                        _resolve_path(base_path, &path.join(s), parts, env)
                    }
                } else {
                    // Non-UTF8 component, push as is
                    _resolve_path(base_path, &path.join(os_str), parts, env)
                }
            }
            _ => _resolve_path(base_path, &path.join(part.as_os_str()), parts, env),
        }
    } else {
        Ok(vec![(path, env)])
    }
}
fn resolve_path(
    base_path: &Path,
    path: &Path,
    env: &HashMap<String, String>,
) -> Result<Vec<(PathBuf, &HashMap<String, String>)>> {
    _resolve_path(base_path, path, path.components(), env)
}

pub mod shell {
    pub fn run(command: String, env: Option<&HashMap<String, String>>) -> Result<&str> {
        let shell = Command::new(std::env::var("SHELL").or("sh")).arg("-c");
        if let Some(env) = env {
            shell.envs(env);
        }

        let output = shell.args(command.split(" ")).output()?;

        if !output.stderr.is_empty() {
            io::stderr().write_all(&output.stderr)?;
        }

        Ok(output.stdout)
    }
    pub fn run_for_bool(command: String, env: Option<&HashMap<String, String>>) -> Result<bool> {
        let output = run(command, env)?;

        match output {
            "0" | "false" => Ok(false),
            "1" | "true" => Ok(true),
            _ => Err("Value returned from shell was not a bool"), // CUSTOM ERROR PLS
        }
    }

    pub fn test_if(command: String, env: Option<&HashMap<String, String>>) -> Result<bool> {
        run_for_bool(
            format!("if [ {} ]; then echo true; else echo false; fi", command),
            env,
        )
    }
    pub fn test_command(command: String, env: Option<&HashMap<String, String>>) -> Result<bool> {
        test_if(format!("-x \"$(command -v {})\"", command), env)
    }

    pub fn echo(command: String, env: Option<&HashMap<String, String>>) -> Result<&str> {
        run(format!("echo {}", command), env)
    }
}

pub struct DotsUseWhen {
    shell: Option<String>,
    test_if: Option<String>,
    test_command: Option<String>,
}
impl DotsUseWhen {
    pub fn try_is_true(&self, env: HashMap<String, String>) -> Result<bool> {
        let shell = if let Some(s) = self.shell {
            shell::run_for_bool(s, None)?
        } else {
            true
        };

        let test_if = if let Some(i) = self.test_if {
            shell::test_if(i, None)?
        } else {
            true
        };

        let test_command = if let Some(c) = self.test_command {
            shell::test_command(i, None)?
        } else {
            true
        };

        Ok(shell && test_if && test_command)
    }
}
pub struct DotsUseVar {
    shell: Option<String>,
    env: Option<String>,
    value: Option<&str>,
}
impl DotsUseVar {
    pub fn try_get_value(&self) -> Result<str> {
        // only one member should ever not be None

        if let Some(v) = self.shell {
            shell::run(v, None)
        } else if let Some(v) = self.env {
            shell::echo(v, None)
        } else if let Some(v) = self.value {
            Ok(v)
        } else {
            Err("variable not supplied a value")
        }
    }
}

pub struct DotsUse {
    pub when: bool,
    pub target: Option<Vec<(PathBuf, PathBuf)>>,
    pub replace_map: HashMap<String, String>,
    pub exclude: Option<Vec<PathBuf>>,
}
impl DotsUse {
    pub fn try_use(
        &self,
        base_path: &PathBuf,
        use_path: &PathBuf,
        global_use_target: Option<Vec<(PathBuf, PathBuf)>>,
        global_exclude: Option<Vec<PathBuf>>,
    ) -> Result<Option<HashMap<PathBuf, PathBuf>>> {
        // What we have:
        // - source should be fully resolved
        // - env should be partially or fully resolved
        // Next:
        // - Go through targets
        //      - resolve suffix
        //      - this should fully fulfill env
        //      - but
        //      - weirdness
        //      - what if you have:
        //      "home/<dir>/baller" -> "~/<dir>"
        //      "root/<dir>/baller" -> "/<dir>"
        //      - we can either:
        //      - 1. run under the assumption order matters
        //          i.e., first resolution wins
        //      - 2. treat each target independently
        //          i.e., the value resolved in one suffix only affects its own target

        let partial_env = &self.replace_map;

        let use_target = self
            .target
            .unwrap_or(global_use_target.expect("No targets provided for 'use'"));
        let global_exclude = global_exclude.unwrap_or_default();
        let local_exclude = self.exclude.unwrap_or_default();

        let track_map: HashMap<PathBuf, PathBuf> = HashMap::new();
        for (suffix, target) in use_target.iter() {
            let sources = resolve_path(base_path, &use_path.join(suffix), partial_env)?;
            for (source, full_env) in sources.iter() {
                if global_exclude.iter().any(|e| source.starts_with(e)) {
                    continue;
                }

                for exc in local_exclude.iter() {
                    if resolve_path(base_path, exc, full_env)?.any(|e| source.starts_with(e)) {
                        continue;
                    }
                }

                // * not allowed, so target should only ever return a single path
                let target = resolve_path(base_path, target, full_env)?
                    .pop()
                    .expect("you done messed up target path resolution brother")
                    .0;

                track_map.insert(source, target);
            }
        }

        Ok(Some(track_map))
    }
}

pub struct DotsOptions {
    pub use_map: Option<HashMap<PathBuf, Vec<DotsUse>>>, // Option<HashMap<PathBuf, (map,exc)>>
    pub use_target: Option<Vec<(PathBuf, PathBuf)>>,
    pub exclude: Option<Vec<PathBuf>>,
    pub inherits: Option<HashSet<Inheritable>>,
}

impl DotsOptions {
    pub fn inherit(&mut self, from: &Self) -> &Self {
        if let None = from.inherits {
            return self;
        }
        let inherits = from.inherits.unwrap();

        for i in inherits.iter() {
            match i {
                Inheritable::Use => match self.use_map {
                    Some(self_use) => {
                        if let Some(from_use) = from.use_map {
                            for (k, v) in from_use {
                                self_use.entry(k).or_insert(v);
                            }
                        }
                    }
                    None => self.use_map = from.use_map,
                },
                Inheritable::UseTarget => match self.use_target {
                    Some(self_use_target) => {
                        if let Some(from_use_target) = from.use_target {
                            for (k, v) in from_use_target {
                                self_use_target
                                    .iter()
                                    .position(|(sk, _)| sk == k)
                                    .or_else(|| self_use_target.push((k, v)))
                            }
                        }
                    }
                    None => self.use_target = from.use_target,
                },
                Inheritable::Exclude => match self.exclude {
                    Some(self_exclude) => {
                        if let Some(from_exclude) = from.exclude {
                            self_exclude.extend(from_exclude.iter());
                        }
                    }
                    None => self.exclude = from.exclude,
                },
            }
        }
    }
}

#[derive(Deserialize)]
pub struct DotfileConfig {
    pub path: PathBuf,
    pub replace: Option<bool>,
    pub use_config: bool,

    pub options: DotsOptions,
}
impl DotfileConfig {
    // use from as base value to be extended by self

    pub fn extend(&mut self, with: Self) -> Self {
        if let None = with {
            return self;
        }
        let with = with.unwrap();

        match self.inherits {
            Some(inherits) => inherits.extend(with.inherits.iter()),
            None => self.inherits = with.inherits,
        }

        self
    }
}

#[derive(Deserialize)]
pub struct DotsConfig {
    options: DotsOptions,
}
impl DotsConfig {
    pub fn extend(&mut self, with: Self) -> Self {
        if let None = with {
            return self;
        }
        let with = with.unwrap();

        match self.inherits {
            Some(inherits) => inherits.extend(with.inherits.iter()),
            None => self.inherits = with.inherits,
        }

        self
    }
}

#[derive(Deserialize)]
pub struct Config {
    pub general: Option<bos::GeneralConfig>,
    pub dots: Option<DotsConfig>,
    pub dotfiles: Option<Vec<DotfileConfig>>, // does nothing in normal configs
}

const CONFIG_LOCS: Vec<&str> = vec![".config/dots/config", ".dots", ".config/bos/config", ".bos"];

impl Config {
    pub fn new() -> Self {
        let config = Self {
            general: None,
            dots: None,
            dotfiles: None,
        };

        // configure defaults

        config
    }

    pub fn inherited_by(&self, by: &Self) -> Self {}

    pub fn load(mut self, path: PathBuf) -> Self {
        self
    }

    // BAD NONO NOT GOOD
    // Be more careful with references
    // Probably will be simpler overall to
    // WAIT
    // Should prolly fix this but it also doesn't really matter right now
    // It might eventually
    // But right now we just care about inheritance among Dots and Dotfiles
    pub fn extend(&mut self, with: Option<Self>) -> &Self {
        if let None = with {
            return self;
        }
        let with = with.unwrap();

        match self.general {
            Some(general_config) => general_config.extend(with.general),
            None => self.general = with.general,
        }
        match self.dots {
            Some(dots_config) => dots_config.extend(with.dots),
            None => self.dots = with.dots,
        }
        match self.dotfiles {
            Some(dotfiles) => dotfiles.extend(with.dotfiles),
            None => self.dotfiles = with.dotfiles,
        }

        self
    }

    pub fn load_all(mut self, paths: Vec<PathBuf>) -> Self {
        for path in paths.into_iter() {
            self.load(path);
        }

        self
    }

    pub fn detect() -> Result<Self> {
        let home = PathBuf::from(std::env::var("HOME").unwrap());
        for loc in CONFIG_LOCS {
            loc = home.join(loc);
            if loc.try_exists() {
                let contents = fs::read_to_string(loc)?;
                return Ok(toml::from_str(&contents)?);
            }
        }

        Ok(Self::default())
    }
}

#[derive(Builder)]
#[builder(pattern = "owned")]
pub struct Dots {
    args: &Args,
    #[builder(default = "Toml::detect()")]
    config: &Config,
    #[builder(default = "bos::Env::detect()")]
    env: &bos::Env,
    #[builder(setter(skip), default = "Trackfile::detect()")]
    state: &Trackfile,
}

// ~~ TOML ~~

pub fn run(dots: Dots) -> Result<()> {
    let cache_dir = dirs::cache_dir()
        .ok_or_else(|| Err(anyhow!("Could not determine user cache directory")))?
        .join("bos");

    let trackfile_path = cache_dir.join("trackfile.toml");

    let mut trackfile_state =
        Trackfile::load(&trackfile_path, env).context("Failed to load trackfile state")?;

    let opts = LinkOptions {
        ..Default::default()
    };

    let dry_run_active = match &dots.args.command {
        Commands::Link(args) => {
            dots.link(args, &opts).context("Link operation failed")?;
            args.dry_run
        }
        Commands::Unlink(args) => {
            dots.unlink(args, &opts)
                .context("Unlink operation failed")?;
            args.dry_run
        }
        Commands::Relink(args) => {
            dots.relink(args, &mut opts)
                .context("Relink operation failed")?;
            args.dry_run
        }
        Commands::Status(args) => {
            dots.status(args, &opts)
                .context("Status operation failed")?;
            false
        }
        Commands::Clean(args) => {
            dots.clean(args, &opts).context("Clean operation failed")?;
            args.dry_run
        }
    };

    if trackfile_state.is_dirty() && !dry_run_active {
        trackfile_state
            .save(&trackfile_path, env)
            .context("Failed to save trackfile state")?;
        println!("Trackfile saved to {}", trackfile_path.display());
    } else if trackfile_state.is_dirty() && dry_run_active {
        println!("DRY RUN: Trackfile would have been saved.");
    }

    Ok(())
}
