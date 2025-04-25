use std::fs;
use std::collections::HashMap;
use std::prelude::v1;

use anyhow::{Context, Result};
use clap::{Parser, Args, Subcommand, ValueEnum};
use toml::{Table};
use serde::Deserialize;


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

#[derive(Args)]
struct DotsArgs {
    #[command(subcommand)]
    command: DotsCommands,
    dotfiles: Option<String>, // only optional if ran before
    #[arg(short, long)]
    include: Option<Vec<String>>, // dotfiles must be a directory if provided
    #[arg(short, long)]
    exclude: Option<Vec<String>>, // dotfiles must be a directory if provided
}

#[derive(Subcommand)]
enum DotsCommands {
    Link(DotsLinkArgs),
    Unlink(DotsLinkArgs),
    Relink(DotsLinkArgs),
}

#[derive(Args)]
struct DotsLinkArgs {
    dotfiles: Option<String>, // only optional if ran before
    #[arg(short, long)]
    include: Option<Vec<String>>, // dotfiles must be a directory if provided
    #[arg(short, long)]
    exclude: Option<Vec<String>>, // dotfiles must be a directory if provided
}

// ~~ TOML ~~

#[derive(Deserialize)]
struct DotfileConfig {
    path: String,
    includes: Option<Vec<String>>,
    excludes: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct BosConfig {
    dotfiles: Option<Vec<DotfileConfig>>
}


// ~~ TOML::Dotfiles ~~

fn gen_track_from_config(config: String, track: &HashMap<String, Vec<String>>) {
    let contents = fs::read_to_string(config).expect("Unable to read config");

    let value: BosConfig = toml::from_str(&contents).expect("oopsies");
    let dotfiles = value.dotfiles;

    if let None() = dotfiles {
        println!("No dotfiles provided...");
        return;
    }

    for dotfile in dotfiles.into_iter() {
        // 
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        CliCommands::Dots(dots_cli) => {
            println!("'bos-cli dots' was used");
            // build out the track file then use the switch to decide what to do with it
            let track = 
            if let Some(inc) = dots_cli.include {
                
            }
            // - 

            match &dots_cli.command {
                DotsCommands::Link(args) => {
                    println!("'bos-cli dots link' was used with: {:?}", args.dotfiles);
                }
                DotsCommands::Unlink(args) => {
                    println!("'bos-cli dots link' was used with: {:?}", args.dotfiles);
                }
                DotsCommands::Relink(args) => {
                    println!("'bos-cli dots link' was used with: {:?}", args.dotfiles);
                }
                _ => {
                    println!("hm")
                }
            }
        }
        CliCommands::Init(args) => {
            println!("'bos-cli init' was used with os '{:?}' and system '{:?}'", args.os, args.system);
        }
        _ => {
            println!("damg")
        }
    }

    Ok(())
}

