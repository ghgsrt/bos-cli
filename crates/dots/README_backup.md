# DotsCLI

A CLI tool for facilitating dotfiles management to a completely unnecessary degree because why not. You may define a toml file to mix and match a variety of dotfiles from a variety of directories, and even have it automatically link certain files depending on your active operating system, user, or home manager (Guix/Nix).

### Dotfile Directory Structure
Your dots are expected to be in the following structure to take advantage of the opinionated (automatic) linking:
```
|- root (global)
|- home (global)
|- os (per-system)
|   |- arch
|   |   |- root
|   |   |- home
|- user (per-user)
|   |- myuser
|   |   |- root
|   |   |- home
|- guix
|   |- root
|   |- home
|   |- os (per guix operating-system)
|   |   |- system-name
|   |   |   |- root
|   |   |   |- home
|   |- user (per guix home-environment)
|   |   |- home-env-name
|   |   |   |- root
|   |   |   |- home
|- nix (same as guix)

```
Any dotfiles placed in a `root` or `home` directory according to the above structure will be linked to your file system matching its subpath relative to `/` or `home/$USER/`, respectively. For example:
```
os/guix/home/.config/zsh/.guix-zshrc -> /home/$USER/.config/zsh/.guix-zshrc
```
The `os` and `user` directories are the same, but apply only the dots which correspond with your current operating system and the user which calls the dots tool.

The dedicated `guix` and `nix` directories function a bit differently and only really exist as a convenience (i.e., to allow you to co-locate with the rest of your dots files that realistically could just be a part of your Guix/Nix config). The following explanation will be in regards to Guix to keep things simple, but the Nix dir should function identically.
This directory is primarily for when you're using Guix as a home manager, so If you want certain dots to be linked when running Guix at the OS level, regardless of the specific operating-system definition you're using, you should still use `os/guix`.
The top-level `guix/root` and `guix/home` will only be linked if guix is detected in a home manager capacity (i.e., when `guix home` is a valid command; this distinction is more meaningful for Nix). `os` and `user` expect their child directories to correspond to identifiers that you will have to set manually (or use [BosCLI](https://github.com/ghgsrt/boscli)) in `$BOS_SYSTEM_NAME` and `$BOS_HOME_NAME`, respectively, for this tool to know what to link. 

### Usage
Note that this tool will create a trackfile to track "ownership" over linked files. A link listed in the trackfile that exists in the filesystem and points to the expected source is considered "correct" and "owned" by this tool, and thus (usually) won't require user input for potentially destructive operations. Dangling symlinks are similarly considered safe to destroy if necessary.

*Nomenclature:*
- "correct symlink": A symlink that exists in the trackfile and points to the source specified in the trackfile (i.e., the "expected source").
- "intended symlink": A symlink that exists in the trackfile and already points to the source expected by the given command (i.e., the "intended source"), regardless of whether or not that matches the source specified in the track file.

#### Basic Command Structure
```bash
dots <command> [command options] [arguments]
```
#### Commands
---
`dots link|unlink|relink [<target>]`
- *Arguments:*
    - `<target>`: A path to a directory, a toml file, or a link to an external repository. Optional if a default set of dotfiles was specified in your global configuration.
These commands share the following options:
- `-i, --include`: A file or directory to explicitly include. This flag can be used multiple times for multiple files/directories. If this flag is detected, ONLY the specified files/directories will be targeted.
- `-e, --exclude`: A file or directory to explicitly exclude. This flag can be used multiple times for multiple files/directories.
- `-v, --verbose`: Enable verbose output, providing more details about the operation.
- `--dry-run`: Perform a dry run, showing actions without modifying the filesystem.
- `--bail`: By default, potentially destructive actions that are not handled by one of the below flags will simply be skipped. Passing this flag will cause it to instead throw an error and stop execution.
- `--interactive`: Prompt the user for confirmation before potentially destructive actions (except when also specifying one of the below flags).
- `-fc, --force-correct-symlink`: Only relevant for the `unlink` command. If you call `unlink` on a given file from a given source set of dotfiles, the symlink being removed must currently point to that file from that set of dotfiles (i.e., be an intended symlink). Passing this flag allows correct symlinks to also be valid for removal. 
- `-fs, --force-symlink`: Potentially destructive actions may apply to any symlink encountered, regardless of whether it is correct or intended, as long as it exists in the trackfile.
- `-ff, --force-file`: Potentially destructive actions may apply to any file or symlink encountered as long as it exists in the trackfile.
- `--force-dangerously`: Potentially destructive actions may apply to any file or symlink enountered no matter what.

---
##### Link
This command will attempt to create symlinks for all of the files resolved from target. By default, if it encounters an existing symlink that is either dangling, intended, or correct, then the symlink will be replaced.
All calls to this command will be performed additively on top of any preexisting trackfile.
If a default target is not specified in your configuration, then the `<target>` argument is required.

---
##### Unlink
This command will attempt to remove all symlinks for all files resolved from target that currently point to the corresponding file in target (i.e., all intended symlinks).

You may specify the following additional option:
- `--hard`: This flag should be provided **in lieu** of `<target>`. The target will be the currently existing trackfile, meaning, by default, all correct symlinks will be removed. The trackfile will be destroyed after this operation.

---
##### Relink
This command will call unlink then link on a given target.

You may specify the following addition option:
- `--hard`: This flag should be provided **in addition** to `<target>`. This is equivalent to calling `dots unlink --hard` then `dots link <target>` for a way to completely swap out the dotfiles in use.


### Configuration
There are two points of configuration which may exist within the same file:
1. The tool itself
2. The dotfiles
The tool portion should only apply when encountered in:
- `$HOME/.config/[bos|dots]/config`
- `$HOME/[.dots|.bos]`
The dotfiles portion will always apply, acting as the default target if specified in one of the above paths.

#### DotsCLI Configuration
```toml
[dots] # not using the general table
# nothing yet
```

#### Dotfiles Configuration
For those that would prefer a declarative approach, you may use a toml file(s) to define your assortment of dotfiles.
There are currently no guards against circularity, but there may be in the future.
```toml
[[dotfiles]] # table array

# [required] a dotfiles directory, an external git repo url, or another toml
path = string

# [optional = true] if true, and if both this [[dotfiles]] and a prior [[dotfiles]] within this config specified a file be linked to the same conflicting path, then this [[dotfiles]] file takes precedence (last wins).
# note: this means order may matter for [[dotfiles]]!
replace = bool

# [optional] a list of paths for files or directories within `path` to explicitly include.
# if this field is not empty, then ONLY the files/directories specified will be included.
# note: paths provided to this field MUST be descendents of one of the top-level directories specified in the section "Dotfiles Directory Structure" above. For other paths, use dotfiles.map below.
includes = string[]

# [optional] a list of paths for files or directories within `path` to explicitly exclude.
# note: paths provided to this field MUST be descendents of one of the top-level directories specified in the section "Dotfiles Directory Structure" above.
excludes = string[]

# [optional] manually map files or directories from within `path` to the filesystem.
# If used with `includes`, these values will also be included.
# If used with `excludes`, any applicable values will still be excluded.
[dotfiles.map]
"from" = "to"
```
##### Examples:
---
TODO

### LSP Support
Eventually, I plan on making an LSP for the configuration files because why not. The LSP will expect the following file extensions:
- `.[bos|dots][.toml]`
