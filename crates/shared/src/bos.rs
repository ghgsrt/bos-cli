use serde::Deserialize;
use std::{collections::HashSet, env::VarError, path::PathBuf};

//use shellexpand;
//
//pub mod env {
//    pub enum Env {
//        Var(&str),
//        BosDir,
//        CacheDir,
//        Home,
//        User,
//        OS,
//        Distro,
//        SystemName,
//        GuixHomeName,
//        NixHomeName,
//    }
//    pub struct Env {
//        pub bos_dir: PathBuf,
//        pub cache_dir: PathBuf,
//        pub home: PathBuf,
//        pub user: String,
//        pub os: String,
//        pub using_guix_system: bool,
//        pub using_nix_system: bool,
//        pub system_name: String,
//        pub guix_home_name: String,
//        pub nix_home_name: String,
//    }
//    impl Env {
//        pub fn detect() {
//            Self {
//                cache_dir: Self::get(Env::Home),
//                home: Self::get(Env::Home),
//            }
//        }
//
//        pub fn get(var: Env) -> Result<String, VarError> {
//            std::env::var(match var {
//                Env::CacheDir => "BOS_CACHE_DIR",
//                Env::Var(v) => v,
//                ..rest => match rest {},
//            })
//        }
//
//        pub fn expand() {}
//    }
//}

pub struct GeneralConfig {
    inherits: Option<HashSet<String>>, // determines whether and what to inherit from the current config state
    strict: Option<bool>,
}
impl GeneralConfig {
    pub fn extend(&mut self, with: Option<Self>) -> Self {
        if let None = with {
            return self;
        }
        let with = with.unwrap();

        match self.inherits {
            Some(inherits) => inherits.extend(with.inherits.iter()),
            None => self.inherits = with.inherits,
        }
        self.strict = with.strict;

        self
    }
}

//pub struct Bos {
//    //args:
//}
