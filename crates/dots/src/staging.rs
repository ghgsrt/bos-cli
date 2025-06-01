use std::collections::{HashMap, HashSet};
use std::fs;
use std::iter::Peekable;
use std::ops::BitOrAssign;
use std::os::unix::fs::DirEntryExt2;
use std::path::{Components, Path, PathBuf};
use std::slice::Iter;

use anyhow::{anyhow, Context, Result};
use toml;

use crate::Config;
use shared::bos;
use shared::fs as sfs;

const ROOT_PREFIX: PathBuf = PathBuf::from("/");
const HOME_PREFIX: PathBuf = PathBuf::from("~");

pub struct TrackMap {
    content: HashMap<PathBuf, PathBuf>, // (source, dest) ((opposite of Trackfile order))
    exclusions: Option<Iter<'_, PathBuf>>, // key prefixes which should not be allowed to insert
    excluded: Vec<PathBuf>,
}
impl TrackMap {
    pub fn new(exclusions: Option<&Vec<PathBuf>>) -> Self {
        TrackMap {
            content: HashMap::new(),
            exclusions: exclusions.and_then(|excs| Some(excs.iter())),
            excluded: vec![],
        }
    }

    // assumes source is at least [(guix|nix)/][(os|user)/<child>/](root|home)
    // assumes source_prefix is [(guix|nix)/][(os|user)/<child>/]
    // assumes target is root|home
    pub fn insert_target(
        &self,
        source: &PathBuf,
        source_prefix: &PathBuf,
        target: str,
    ) -> Result<Option<PathBuf>> {
        if self
            .exclusions
            .is_some_and(|excs| excs.any(|exc_prefix| source.starts_with(exc_prefix)))
        {
            self.excluded.push(source);
            Ok(Some(source))
        } else {
            let dest_prefix = match target {
                "root" => Ok(ROOT_PREFIX),
                "home" => Ok(HOME_PREFIX),
                _ => Err(()),
            }?;

            let dest = change_file_prefix(&source_prefix.join(target), &dest_prefix, &source);
            self.content.insert(source, dest);
            Ok(None)
        }
    }

    // assumes source is [(guix|nix)/][(os|user)/<child>]
    pub fn insert_targets(&self, source: &PathBuf) -> Result<Vec<PathBuf>> {
        let excluded = vec![];

        if let Some(res) = self.insert_target(&source.join("root"), source, "root")? {
            excluded.push(res);
        }
        if let Some(res) = self.insert_target(&source.join("home"), source, "home")? {
            excluded.push(res);
        }

        self.excluded.extend(excluded);

        return Ok(excluded);
    }

    // assume base_path.join(source) is the intended abs path for source
    // assumes source is [(guix|nix)/](os|user)
    pub fn insert_targets_for_all_children(
        &self,
        base_path: &Path,
        source: &PathBuf,
    ) -> Result<Vec<PathBuf>> {
        let mut excluded = vec![];

        for entry in fs::read_dir(base_path.join(source))? {
            let entry = entry?;
            if !fs::metadata(entry.path())?.is_dir() {
                continue;
            }

            excluded.append(&mut self.insert_targets(&source.join(entry.file_name_ref()))?);
            // [(guix|nix)/](os|user)/<child>/(root&home)
        }

        self.excluded.extend(excluded);

        Ok(excluded)
    }
}

fn strip_first_char(s: &str) -> &str {
    s.chars().next().map_or("", |c| &s[c.len_utf8()..])
}

fn _handle_inclusions(
    inclusions: &Vec<PathBuf>,
    exclusions: Option<&Vec<PathBuf>>,
    base_path: &Path,
    env: &bos::Env,
) -> Result<TrackMap> {
}

fn process(base_path: &Path, source: &Path, parts: Components, env: &bos::Env) -> Vec<PathBuf> {
    if let Some(part) = parts.next() {
        if part == "*" {
            let paths: Vec<PathBuf> = Vec::new();

            for entry in fs::read_dir(base_path.join(source))? {
                let entry = entry?;
                if !fs::metadata(entry.path())?.is_dir() {
                    continue;
                }

                paths.append(&mut process(
                    base_path,
                    &source.join(entry.file_name_ref()),
                    parts,
                    env,
                ))
            }

            return paths;
        }

        let adj_part = match part {
            "@" => match source {
                "os" => env.os,
                "user" => env.user,
                "guix/os" | "nix/os" => env.system_name,
                "guix/user" => env.guix_home_name,
                "nix/user" => env.nix_home_name,
                _ => Err(()), //TODO: better error
            },
            "~" => env.get("HOME"),
            _ => {
                let part_as_str = part.as_os_str().to_str();
                if part_as_str?.starts_with("$") {
                    env.get(strip_first_char(part_as_str.unwrap()))
                } else {
                    part
                }
            }
        };

        process(base_path, &source.join(adj_part), parts, env)
    } else {
        vec![source]
    }
}

fn inclusion_err(part: Option<String>, source: &PathBuf) -> Err {
    Err(anyhow!(match part {
        Some(p) => format!("Invalid inclusion path: {} in {}", p, source.display()),
        None => format!("Invalid inclusion path: {}", source.display()),
    }))
}

// produce a map to be extended by the manual mappings
fn handle_inclusions(
    inclusions: &Vec<PathBuf>,
    exclusions: Option<&Vec<PathBuf>>,
    base_path: &Path,
    env: &bos::Env,
) -> Result<TrackMap> {
    let map = TrackMap::new(exclusions);

    for inc in inclusions.into_iter() {
        for source in process(base_path, inc, inc.components(), env) {
            let excluded = vec![];
            let parts = source.components().peekable();

            if let None = parts.peek() {
                // prefix is empty
                return inclusion_err(None, &source);
            }
            let first = parts.next().unwrap(); // expects guix|nix|os|user|root|home

            let mut source_prefix = PathBuf::from(first);

            // expects source_prefix is at least [(guix|nix)/](os|user)
            let handle_osuser = || -> Result<Vec<PathBuf>> {
                if let None = parts.peek() {
                    // prefix is [(guix|nix)/](os|user)
                    return map.insert_targets_for_all_children(base_path, &source_prefix);
                    // [(guix|nix)/](os|user)/<child>/(root&home)
                }
                let mut name = parts.next().unwrap(); // expects String|None

                source_prefix = source_prefix.join(name); // prefix is [(guix|nix)/](os|user)/<name>

                if let None = parts.peek() {
                    return map.insert_targets(&source_prefix); // [(guix|nix)/](os|user)/<name>/(root&home)
                }
                let target = parts.next().unwrap(); // expects root|home

                map.insert_target(&source, &source_prefix, target) // [(guix|nix)/](os|user)/<name>/<target>
                    .or_else(|| inclusion_err(Some(target), &source))
            };

            match first {
                "guix" | "nix" => {
                    if let None = parts.peek() {
                        // prefix is (guix|nix)
                        map.insert_targets(&source_prefix)?; // (guix|nix)/(root&home)

                        let os_path = source_prefix.join("os");
                        map.insert_targets_for_all_children(base_path, &os_path)?; // (guix|nix)/os/<children>/(root&home)

                        let user_path = source_prefix.join("user");
                        map.insert_targets_for_all_children(base_path, &user_path)?; // (guix|nix)/user/<children>/(root&home)

                        continue;
                    }
                    let second = parts.next().unwrap(); // expects os|user|root|home

                    match second {
                        // prefix is at least (guix|nix)/(os|user)
                        "os" | "user" => handle_osuser()?,
                        // prefix is (guix|nix)/(root|home)
                        "root" | "home" => map.insert_target(&source, &source_prefix, second)?, // (guix|nix)/(root|home)
                        _ => return inclusion_err(Some(second), &source),
                    }
                }
                "os" | "user" => handle_osuser()?,
                "home" | "root" => map.insert_target(&source, "", first)?, // (home|root)
                _ => return inclusion_err(None, &source),
            }
        }
    }

    Ok(map)
}

fn handle_base(
    exclusions: Option<&Vec<PathBuf>>,
    base_path: &Path,
    env: &bos::Env,
) -> Result<TrackMap> {
    let map = TrackMap::new(exclusions);

    map.insert_targets("")?;
    map.insert_targets_for_all_children(base_path, "os")?;
    map.insert_targets_for_all_children(base_path, "user")?;

    if env.using_guix_system {
        map.insert_targets(&PathBuf::from("guix/os").join(env.system_name))?;
    } else if env.using_nix_system {
        map.insert_targets(&PathBuf::from("nix/os").join(env.system_name))?;
    }

    if env.using_guix_home {
        map.insert_targets("guix")?;
        map.insert_targets(&PathBuf::from("guix/user").join(env.guix_home_name))?;
    }
    if env.using_nix_home {
        map.insert_targets("nix")?;
        map.insert_targets(&PathBuf::from("nix/user").join(env.nix_home_name))?;
    }

    Ok(map)
}

pub struct GeneralConfig {
    inherits: Option<HashSet<String>>, // determines whether and what to inherit from the current
    // config state
    strict: Option<bool>,
}
impl GeneralConfig {
    pub fn extend(&mut self, with: Option<Self>) -> Self {
        if let None = with {
            return self;
        }
        let with = with.unwrap();

        match self.inherits {
            Some(inherits) => inherits.extend(&mut with.inherits),
            None => self.inherits = with.inherits,
        }
        self.strict = with.strict;

        self
    }
}
