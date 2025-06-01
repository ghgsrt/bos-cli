use std::fmt::{self, Display};

use anyhow::{self, anyhow};

use crate::*;
use shared::fs::FilesystemStatus;
use shared::fs::{self as sfs, is_symlink};

type Flags = DotsLinkArgs;

pub enum Choice {
    Yes { all: bool },
    No { all: bool },
    Quit,
}

pub enum ChoiceState {
    Unset,
    Never,
    Always,
}
use ChoiceState::*;

impl Default for ChoiceState {
    fn default() -> Self {
        Unset
    }
}

#[derive(Default)]
pub struct UserChoiceState {
    force_correct_symlink: ChoiceState,
    force_symlink: ChoiceState,
    force_file: ChoiceState,
    force_dangerously: ChoiceState,
}

impl UserChoiceState {
    pub fn get(&self, reason: Reason) -> Option<u8> {
        Some(match reason {
            ForceDangerously => self.force_dangerously,
            ForceFile => self.force_file,
            ForceSymlink => self.force_symlink,
            ForceCorrectSymlink => self.force_correct_symlink,
            _ => return None,
        })
    }

    pub fn set(&mut self, reason: Reason, value: ChoiceState) -> Result<()> {
        match reason {
            ForceDangerously => self.force_dangerously = value,
            ForceFile => self.force_file = value,
            ForceSymlink => self.force_symlink = value,
            ForceCorrectSymlink => self.force_correct_symlink = value,
            _ => return Err(anyhow!("Invalid reason")),
        }
        Ok(())
    }
    pub fn unset(&mut self, reason: Reason) -> Result<()> {
        self.set(reason, Unset)
    }
    pub fn set_never(&mut self, reason: Reason) -> Result<()> {
        self.set(reason, Never)
    }
    pub fn set_always(&mut self, reason: Reason) -> Result<()> {
        self.set(reason, Always)
    }
}

pub enum Opts {
    YesNo,
    YesNoAll,
    All,
}

impl Opts {
    pub fn get() -> &str {
        match opts {
            Opts::YesNo => "[Y]es/[N]o",
            Opts::YesNoAll => "[Y]es/[N]o/[Y]es[A]all/[N]o[All]",
            Opts::All => "[Y]es/[N]o/[Y]es[A]all/[N]o[A]ll/[I]nfo/[Q]uit",
        }
    }

    pub fn process(&self, input: &str) -> Option<Choice> {
        match self {
            Opts::YesNo => match input {
                "y" | "yes" => Some(Choice::Yes { all: false }),
                "n" | "no" => Some(Choice::No { all: false }),
                _ => None,
            },
            Opts::YesNoAll => process_options(Opts::YesNo, input).or_else(|| match input {
                "ya" | "yesall" => Some(Choice::Yes { all: true }),
                "na" | "noall" => Some(Choice::No { all: true }),
                _ => None,
            }),
            Opts::All => process_options(Opts::YesNoAll, input).or_else(|| match input {
                "i" | "info" => Some(println!(reason.info())),
                "q" | "quit" | "cancel" => return Some(Choice::Quit),
                _ => None,
            }),
        }
    }
}

pub fn prompt_user_choice(prompt: String, reason: Reason, opts: Opts) -> Choice {
    let choices_help = opts.get();
    let full_prompt = format!("{}\n{}: ", prompt_message, choices_help);

    loop {
        print!("{}", full_prompt);

        if let Err(e) = io::stdout().flush() {
            eprintln!("Error flushing stdout: {}. Retrying...", e);
            continue;
        }

        let mut input = String::new();
        match io::stdin().read_line(&mut input) {
            Ok(_) => {
                let processed_input = opts.process(input.trim().to_lowercase().as_str());

                if let Some(choice) = processed_input {
                    return choice;
                } else {
                    println!("Invalid input. Please choose from {}.", choices_help);
                }
            }
            Err(error) => {
                println!("Error reading input: {}. Please try again.", error);
            }
        }
    }
}

pub fn prompt_user(
    user_choice_state: &UserChoiceState,
    reason: Reason,
    dest_path: &PathBuf,
    points_to: Option<&PathBuf>,
) -> Op {
    let user_choice = match user_choice_state.get(reason).unwrap() {
        Never => Choice::No,
        Always => Choice::Yes,
        _ => prompt_user_choice(
            if let Some(source_path) = points_to {
                format!(
                    "[ {} ] Remove symlink at {} (points to: {})",
                    reason.short_flag(),
                    dest_path.display(),
                    source_path.display()
                )
            } else {
                format!(
                    "[ {} ] Remove file (not a symlink!) at {}",
                    reason.short_flag(),
                    dest_path.display()
                )
            },
            reason,
            Opts::All,
        ),
    };

    match user_choice {
        Choice::No { all } => {
            if all {
                user_choice_state.set_never(reason);
            }
            Denied(reason)
        }
        Choice::Yes { all } => {
            if all {
                user_choice_state.set_always(reason);
            }
            Confirmed(reason)
        }
        Choice::Quit => Denied(UserQuit),
    }
}

pub enum Reason {
    ForceDangerously,
    ForceFile,
    ForceSymlink,
    ForceCorrectSymlink,
    DanglingSymlink,
    CorrectSymlink,  // think: this is where we want it pointing *before* op
    IntendedSymlink, // think: this is where we want it pointing *after* op
    NotFound,
    StatusInvalid,
    StatusError(String),
    UserQuit,
}
use Reason::*;

impl Reason {
    pub fn info(&self) -> str {
        match self {
            ForceDangerously => "destination is not tracked",
            ForceFile => "destination is tracked but is a file",
            ForceSymlink => {
                "destination is tracked and is a symlink but points to neither the intended nor the expected source"
            }
            ForceCorrectSymlink => "destination is tracked and is a symlink but doesn't point to the expected source",
            DanglingSymlink => "destination is a dangling symlink",
            CorrectSymlink => "destination is a symlink that points to the expected source",
            IntendedSymlink => "destination is a symlink that points to the intended source",
            NotFound => "destination not found (nothing to remove)",
            StatusInvalid => "destination is not a symlink or file (nothing to remove)",
            StatusError(e) => format!("error checking destination type: {}", e),
            UserQuit => "the user canceled the operation",
        }
    }

    pub fn short_flag(&self) -> str {
        match self {
            ForceDangerously => "--force-dangerously",
            ForceFile => "-ff",
            ForceSymlink => "-fs",
            ForceCorrectSymlink => "-fc",
            _ => "",
        }
    }

    pub fn flags(&self) -> str {
        match self {
            ForceDangerously => "--force-dangerously",
            ForceFile => "-ff or --force-dangerously",
            ForceSymlink => "-fs, -ff, or --force-dangerously",
            ForceCorrectSymlink => "-fc, -fs, -ff, or --force-dangerously",
            _ => "",
        }
    }

    // given a set of flags, could this reason be valid?
    pub fn test_flags(&self, flags: &Flags) -> bool {
        match self {
            ForceDangerously => flags.force_dangerously,
            ForceFile => flags.force_file || flags.force_dangerously,
            ForceSymlink => flags.force_symlink || flags.force_file || flags.force_dangerously,
            ForceCorrectSymlink => {
                flags.force_correct_symlink
                    || flags.force_symlink
                    || flags.force_file
                    || flags.force_dangerously
            }
            _ => true,
        }
    }

    pub fn consult_user(
        &self,
        flags: &Flags,
        user_choices: &UserChoiceState,
        dest_path: &PathBuf,
        points_to: Option<&PathBuf>,
    ) -> Op {
        if !self.short_flag().is_empty() {
            if flags.interactive {
                prompt_user(user_choices, self, dest_path, points_to)
            } else {
                Op::verify(self.test_flags(flags), self)
            }
        } else {
            Denied(UserQuit) // get fucked idiot (don't need a flag, don't need a user)
        }
    }
}

pub enum Op {
    Confirmed(Reason),
    Denied(Reason),
}
use Op::*;

impl fmt::Display for Op {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let (reason, format_string) = match self {
            Confirmed(reason) => (reason, "({} was used)"),
            Denied(reason) => (reason, "(use {} to remove)"),
        };

        let parts = vec![
            reason.info(),
            if !reason.flags().is_empty() {
                format!(format_string, reason.flags())
            } else {
                ""
            },
        ];

        write!(f, parts.join(" "))
    }
}

impl Op {
    #[inline]
    pub fn or(self, opb: Self) -> Self {
        match self {
            x @ Confirmed(_) => x,
            Denied(_) => opb,
        }
    }

    #[inline]
    pub fn or_else<F>(self, f: F) -> Self
    where
        F: FnOnce() -> Self,
    {
        match self {
            x @ Confirmed(_) => x,
            Denied(reason) => f(reason),
        }
    }

    #[inline]
    pub fn or_else_if<F>(self, cond: bool, f: F) -> Self
    where
        F: FnOnce() -> Self,
    {
        match self {
            x @ Confirmed(_) => x,
            Denied(reason) => f(reason),
        }
    }

    pub fn reason(&self) -> &Reason {
        match self {
            Confirmed(reason) => reason,
            Denied(reason) => reason,
        }
    }

    pub fn was_confirmed(&self) -> bool {
        match self {
            Confirmed(_) => true,
            _ => false,
        }
    }

    pub fn was_denied(&self) -> bool {
        match self {
            Denied(_) => true,
            _ => false,
        }
    }

    pub fn info(&self) -> &str {
        &self.reason().info()
    }

    pub fn verify<R: Reason>(cond: bool, reason: R) -> Op<R> {
        if cond {
            Confirmed(reason)
        } else {
            Denied(reason)
        }
    }
}

pub fn try_bail(flags: &Flags, e: String, bail_msg: String) -> Result<bool> {
    let err = Err(anyhow::Error::new(e).context(bail_msg));

    println!("[ BAIL ] {}", e);
    if args.interactive {
        match prompt_user_choice(
            "Continue execution?".to_string(),
            StatusError(e),
            Opts::YesNoAll,
        ) {
            Choice::No { .. } => err,
            Choice::Yes { all } => Ok(all),
            _ => {} // Quit not offered
        }
    } else {
        err
    }
}

#[derive(Default)]
pub struct LinkStats {
    errors: i32,
    targets: i32,
    symlinks_added: i32,
    symlinks_removed: i32,
    files_removed: i32,
    targets_skipped: i32,
}

impl LinkStats {
    pub fn new(targets: i32) -> Self {
        let stats = &mut Self::default();
        stats.targets = targets;
        stats
    }

    pub fn display(&self, dry_run: bool) {}
}

#[derive(Default)]
pub struct UnlinkStats {
    errors: i32,
    targets: i32,
    symlinks_removed: i32,
    files_removed: i32,
    targets_skipped: i32,
}

impl UnlinkStats {
    pub fn new(targets: i32) -> Self {
        let stats = &mut Self::default();
        stats.targets = targets;
        stats
    }

    pub fn display(&self, dry_run: bool) {
        let targets_removed = self.symlinks_removed + self.files_removed;
        if targets_removed > 0 {
            if dry_run {
                println!(
                    "Would have successfully unlinked {}/{} potential entries.",
                    targets_removed,
                    self.targets.len()
                );
            } else {
                println!(
                    "Successfully unlinked {}/{} potential entries.",
                    targets_removed,
                    self.targets.len()
                );
            }
            println!("Symlinks removed: {}", self.symlinks_removed);
            println!("Files removed:    {}", self.files_removed);
        } else {
            if dry_run {
                println!(
                    "No entries would have been unlinked (skipped {}).",
                    self.targets_skipped
                );
            } else {
                println!(
                    "No entries were unlinked (skipped {}).",
                    self.targets_skipped
                );
            }
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct RelinkStats {
    errors: i32,
    targets: i32,
    symlinks_added: i32,
    symlinks_removed: i32,
    files_removed: i32,
    targets_skipped: i32,
}

impl RelinkStats {
    pub fn new(unlink_stats: UnlinkStats, link_stats: LinkStats) -> Self {
        //TODO actually do it lmao
        unlink_stats
    }
}

#[derive(Default)]
pub struct LinkOptions {
    trackfile: Option<&Trackfile>,
    silent: bool,
}

impl Dots {
    pub fn try_perform_link(
        &mut self,
        args: &DotsLinkArgs,
        stats: &LinkStats,
        user_choices: &UserChoiceState,
        target_dest: &PathBuf,
        target_source: &PathBuf,
    ) -> Result<()> {
        let dest_status = sfs::get_status(target_dest);
        let is_tracked = self.state.contains_dest(target_dest);
        let tracked_source = self.state.get_source(target_dest);

        let operation = match dest_status {
            FilesystemStatus::NotFound => Confirmed(NotFound),
            FilesystemStatus::Error(e) => Denied(StatusError(e)),
            FilesystemStatus::Symlink {
                points_to,
                dangling,
            } => {
                if dangling {
                    Confirmed(DanglingSymlink)
                } else if points_to.unwrap() == target_source {
                    Denied(IntendedSymlink)
                } else if is_tracked {
                    if points_to.unwrap() == tracked_source.unwrap() {
                        ForceCorrectSymlink.consult_user(args, user_choices, target_dest, points_to)
                    } else {
                        ForceSymlink.consult_user(args, user_choices, target_dest, points_to)
                    }
                } else {
                    ForceDangerously.consult_user(args, user_choices, target_dest, points_to)
                }
            }
            FilesystemStatus::File => {
                if is_tracked {
                    ForceFile.consult_user(args, user_choices, target_dest, None)
                } else {
                    ForceDangerously.consult_user(args, user_choices, target_dest, None)
                }
            }
            _ => Confirmed(StatusInvalid), // shouldn't conflict as we only ever link files ??
        };

        // --- Dry Run ---
        if args.dry_run {
            println!("{} -> {}", target_dest.display(), target_source.display());
            match operation {
                Confirmed(reason) => {
                    stats.symlinks_added += 1;
                    match reason {
                        NotFound | StatusInvalid | IntendedSymlink => {
                            println!("[ DRY RUN --- Link ] {}", operation)
                        }
                        DanglingSymlink | ForceCorrectSymlink | ForceSymlink => {
                            stats.symlinks_removed += 1;
                            println!("[ DRY RUN --- Remove+Link ] {}", operation);
                        }
                        _ => {
                            stats.files_removed += 1;
                            println!("[ DRY RUN --- Remove+Link ] {}", operation)
                        }
                    }
                }
                Denied(reason) => match reason {
                    StatusError(e) => return Err(e),
                    _ => {
                        stats.targets_skipped += 1;
                        println!("[ DRY RUN --- Skip ] {}", operation)
                    }
                },
            }
            return Ok(());
        }

        // --- Perform Link ---
        match operation {
            Confirmed(reason) => {
                match reason {
                    NotFound | StatusInvalid => {} // shouldn't be any conflicts to remove ??
                    _ => {
                        // only symlinks and files are valid for removal
                        sfs::remove_file(&target_dest).with_context(|| {
                            format!(
                                "Failed to remove {} at {}",
                                dest_status,
                                target_dest.display()
                            )
                        })?;
                        if let FilesystemStatus::Symlink { .. } = dest_status {
                            stats.symlinks_removed += 1;
                        } else {
                            stats.files_removed += 1;
                        }

                        if args.verbose {
                            println!("Removed {} at {}", dest_status, target_dest.display());
                        }
                    }
                }

                // should probably only be a necessary check for NotFound ?? but whatever
                if let Some(parent) = target_dest.parent() {
                    sfs::create_dir_all(parent).with_context(|| {
                        format!(
                            "Failed to create parent directory for {}",
                            target_dest.display()
                        )
                    })?;
                }

                sfs::create_symlink(&target_source, &target_dest).with_context(|| {
                    format!(
                        "Failed to create symlink {} -> {}",
                        target_dest.display(),
                        target_source.display()
                    )
                })?;
                stats.symlinks_added += 1;

                if args.verbose {
                    println!(
                        "Linked {} -> {}",
                        target_dest.display(),
                        target_source.display()
                    );
                }

                // update trackfile
                self.state.insert(target_dest, target_source);

                Ok(())
            }
            Denied(reason) => match reason {
                StatusError(e) => Err(e),
                _ => {
                    stats.targets_skipped += 1;

                    if args.verbose {
                        println!("Skipping link for {}: {}", target_dest.display(), operation);
                    }

                    Ok(())
                }
            },
        }
    }

    // source of truth (correctness of symlink) is from CURRENT trackfile state
    pub fn link(&mut self, args: &DotsLinkArgs, opts: &LinkOptions) -> Result<LinkStats> {
        let targets = match opts.trackfile {
            Some(tf) => tf,
            None => Trackfile::generate(args.target, self.env)
                .context("Failed to resolve link targets")?,
        };

        if targets.is_empty() {
            println!("No dotfiles found to link based on the provided target and filters.");
            return Ok(());
        }

        println!("Preparing to link {} dotfiles...", targets.len());

        let stats = LinkStats::new(targets.len());

        let mut user_never_bail = false;
        let user_choices = UserChoiceState::default();

        for (target_dest, target_source) in targets.into_iter() {
            let bail_msg = format!(
                "User bailed link operation from {} to {}",
                target_dest.display(),
                target_source.display()
            );

            if let Err(e) =
                self.try_perform_link(args, &stats, &user_choices, &target_dest, &target_source)
            {
                stats.errors += 1;
                if args.bail && !user_never_bail {
                    user_never_bail = try_bail(args, e, bail_msg)?;
                }
            }
        }

        if !opts.silent {
            stats.display(args);
        }

        Ok(stats)
    }

    pub fn try_perform_unlink(
        &mut self,
        args: &DotsLinkArgs,
        stats: &UnlinkStats,
        user_choices: &UserChoiceState,
        target_dest: &PathBuf,
        target_source: &PathBuf,
    ) -> Result<()> {
        let dest_status = sfs::get_status(target_dest);
        let is_tracked = self.state.contains_dest(target_dest);
        let tracked_source = self.state.get_source(target_dest);

        let operation = match dest_status {
            FilesystemStatus::NotFound => Denied(NotFound),
            FilesystemStatus::Error(e) => Denied(StatusError(e)),
            FilesystemStatus::Symlink {
                points_to,
                dangling,
            } => {
                if dangling {
                    Confirmed(DanglingSymlink)
                } else if dest_points_to.unwrap() == target_source {
                    Confirmed(IntendedSymlink)
                } else if is_tracked {
                    if points_to.unwrap() == tracked_source.unwrap() {
                        ForceCorrectSymlink.consult_user(args, user_choices, target_dest, points_to)
                    } else {
                        ForceSymlink.consult_user(args, user_choices, target_dest, points_to)
                    }
                } else {
                    ForceDangerously.consult_user(args, user_choices, target_dest, points_to)
                }
            }
            FilesystemStatus::File => {
                if is_tracked {
                    ForceFile.consult_user(args, user_choices, target_dest, None)
                } else {
                    ForceDangerously.consult_user(args, user_choices, target_dest, None)
                }
            }
            _ => Denied(StatusInvalid),
        };

        // --- Dry Run ---
        if args.dry_run {
            println!("Unlink {}", target_dest.display());
            match operation {
                Confirmed(reason) => {
                    println!("[ DRY RUN --- Remove ] {}", operation);
                    match reason {
                        DanglingSymlink | IntendedSymlink | ForceCorrectSymlink | ForceSymlink => {
                            stats.symlinks_removed += 1
                        }
                        _ => stats.files_removed += 1,
                    }
                }
                Denied(reason) => match reason {
                    StatusError(e) => return Err(e),
                    _ => {
                        println!("[ DRY RUN --- Skip ] {}", operation);
                        stats.targets_skipped += 1;
                    }
                },
            }
            return Ok(());
        }

        // --- Perform Unlink ---
        match operation {
            Confirmed(reason) => {
                // only symlinks and files are valid for removal
                sfs::remove_file(target_dest).with_context(|| {
                    format!(
                        "Failed to remove {} at {}",
                        dest_status,
                        target_dest.display()
                    )
                })?;
                if let FilesystemStatus::Symlink { .. } = dest_status {
                    stats.symlinks_removed += 1;
                } else {
                    stats.files_removed += 1;
                }

                if args.verbose {
                    println!("Removed {} at {}", dest_status, target_dest.display());
                }

                // update trackfile
                self.state.remove(target_dest);

                Ok(())
            }
            Denied(reason) => match reason {
                StatusError(e) => Err(e),
                _ => {
                    stats.targets_skipped += 1;

                    if args.verbose {
                        println!(
                            "Skipping unlink for {}: {}",
                            target_dest.display(),
                            operation
                        );
                    }
                    Ok(())
                }
            },
        }
    }

    //TODO
    // - handle converting target_dest to proper paths (THIS SHOULDB E HANDED BY GENEREATE??)
    // - anytime comparing a path with target_source, probably need to do a prefix/parent match as
    // it should just be a parent directory to anything it's being compared to

    // source of truth (correctness of symlink) is from GENERATED trackfile state
    pub fn unlink(&mut self, args: &DotsLinkArgs, opts: &LinkOptions) -> Result<UnlinkStats> {
        let targets = match opts.trackfile {
            Some(tf) => tf,
            None => Trackfile::generate(self, args.target)
                .context("Failed to resolve unlink targets")?,
        };

        if targets.is_empty() {
            println!("No dotfiles found to unlink based on the provided target and filters.");
            return Ok(());
        }

        println!(
            "Preparing to unlink {} specified dotfiles...",
            targets.len()
        );

        let stats = UnlinkStats::new(targets.len());

        let mut user_never_bail = false;
        let user_choices = UserChoiceState::default();

        for (target_dest, target_source) in targets.into_iter() {
            let bail_msg = format!(
                "User bailed unlink operation on {} (from {})",
                target_dest.display(),
                target_source.display()
            );

            if let Err(e) =
                self.try_perform_unlink(args, &stats, &user_choices, &target_dest, &target_source)
            {
                stats.errors += 1;
                if args.bail && !user_never_bail {
                    user_never_bail = try_bail(args, e, bail_msg)?;
                }
            }
        }

        if !opts.silent {
            stats.display(args);
        }

        Ok(stats)
    }

    pub fn relink(&mut self, args: &DotsLinkArgs, opts: &mut LinkOptions) -> Result<RelinkStats> {
        match opts.trackfile {
            Some(tf) => {}
            None => {
                opts.trackfile = Some(
                    Trackfile::generate(args.target, self.env)
                        .context("Failed to resolve relink targets")?,
                )
            }
        }
        let targets = opts.trackfile.unwrap();

        if targets.is_empty() {
            println!("No dotfiles found to relink based on the provided target and filters.");
            return Ok(());
        }

        println!(
            "Preparing to relink {} specified dotfiles...",
            targets.len()
        );

        let unlink_stats = self.unlink(args, opts)?;
        let link_stats = self.link(args, opts)?;

        Ok(RelinkStats::new(unlink_stats, link_stats))
    }

    pub fn status(&self, args: &DotsStatusArgs, opts: &LinkOptions) -> Result<()> {
        Err(anyhow!("[ STATUS ] Not implemented yet :p"))
    }
    pub fn clean(&mut self, args: &DotsCleanArgs, opts: &LinkOptions) -> Result<()> {
        Err(anyhow!("[ CLEAN ] Not implemented yet :p"))
    }
}
