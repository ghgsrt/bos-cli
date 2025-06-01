# **DotsCLI: Advanced Dotfile Management**

([https://img.shields.io/travis/user/repo.svg](https://img.shields.io/travis/user/repo.svg))\]([https://travis-ci.org/user/repo)(https://img.shields.io/badge/License-MIT-yellow.svg](https://travis-ci.org/user/repo)(https://img.shields.io/badge/License-MIT-yellow.svg))\]([https://opensource.org/licenses/MIT](https://opensource.org/licenses/MIT)) ([https://img.shields.io/badge/PRs-welcome-brightgreen.svg?style=flat-square](https://img.shields.io/badge/PRs-welcome-brightgreen.svg?style=flat-square))\]([http://makeapullrequest.com](http://makeapullrequest.com)) \#\# Overview

DotsCLI is a command-line interface (CLI) tool engineered for meticulous and highly granular management of dotfiles. It empowers users to define sophisticated configurations, enabling the mixing and matching of dotfiles from diverse local or remote sources. The tool offers automated, context-aware linking of files based on operating system, active user, or the presence of home environment managers like Guix or Nix.

## **Table of Contents**

* (\#why-dotscli)  
* Core Features  
* (\#dotfile-directory-structure)  
* Installation  
  * Prerequisites  
* Usage  
  * Nomenclature  
  * (\#basic-command-structure)  
  * (\#shared-command-options)  
  * Commands  
    * link  
    * unlink  
    * relink  
* (\#configuration-toml)  
  * Configuration File Locations  
  * \`)\](\#dotscli-tool-configuration-dots)  
  * \]\`)\](\#dotfiles-definition-dotfiles)  
    * \]\`)\](\#dotfiles-configuration-keys-dotfiles)  
  * (\#toml-configuration-examples)  
* Advanced Examples & Use Cases  
* (\#lsp-support-future)  
* Contributing  
* License

## **Why DotsCLI?**

Managing configuration files (dotfiles) across multiple systems, users, or operating environments can become complex. Standard approaches often lack the flexibility to handle conditional linking, diverse file sources, or fine-grained control over which files are managed. DotsCLI addresses these challenges by providing a robust framework for declaring how dotfiles should be organized and deployed. While the level of control offered might seem extensive, it is designed for users who require precision and automation in their dotfile management strategy, particularly in heterogeneous computing environments. This tool aims to transform a potentially tedious task into a declarative and reproducible process.

## **Core Features**

DotsCLI offers a suite of features designed for comprehensive dotfile management:

* **Declarative Configuration:** Define dotfile sources and linking behavior using simple TOML configuration files.  
* **Multi-Source Aggregation:** Combine dotfiles from various local directories, remote Git repositories, or even other TOML configuration files.  
* **Context-Aware Linking:** Automatically link specific dotfiles based on:  
  * Operating System (e.g., Linux, macOS, Windows)  
  * Current User  
  * Active Home Manager (Guix Home Environment, Nix Profile/Home-Manager)  
* **Opinionated Directory Structure:** A predefined directory layout enables automatic linking to standard system locations (/ or $HOME).  
* **Manual Mapping:** Flexibility to map any file or directory from a source to any arbitrary location on the filesystem.  
* **Trackfile System:** Maintains a record of linked files ("trackfile") to manage ownership and facilitate safe operations on symlinks.  
* **Granular Control over Operations:**  
  * Include or exclude specific files/directories.  
  * Force flags for handling existing files or symlinks with varying degrees of assertiveness.  
  * Interactive mode for user confirmation on potentially destructive actions.  
  * Dry-run capability to preview changes before applying them. 1  
* **Atomic Swapping:** Relink functionality allows for complete replacement of one set of dotfiles with another.

The combination of these features allows for a highly adaptable and powerful dotfile management system. The trackfile, for instance, is a key component for ensuring that DotsCLI operates predictably, especially when dealing with pre-existing files or symlinks not managed by the tool. It provides a basis for the tool to make informed decisions about which files it "owns" and can therefore modify or remove more freely.

## **Dotfile Directory Structure**

DotsCLI employs an opinionated directory structure within your dotfile sources to enable automatic, context-aware symlinking. Files placed within designated root or home subdirectories will be linked to corresponding paths in your filesystem, relative to / or $HOME/$USER/ respectively.

The expected top-level directory structure within a dotfile source (e.g., a Git repository or a local directory specified in your TOML configuration) is as follows:

.  
├── root/                    \# Files to be linked relative to / (requires appropriate permissions)  
├── home/                    \# Files to be linked relative to $HOME/$USER/  
├── os/                      \# OS-specific dotfiles  
│   ├── \<os\_identifier\>/     \# e.g., linux, macos, windows, arch, guix (for GuixOS)  
│   │   ├── root/  
│   │   └── home/  
├── user/                    \# User-specific dotfiles  
│   ├── \<username\>/  
│   │   ├── root/  
│   │   └── home/  
├── guix/                    \# Guix Home Environment specific dotfiles  
│   ├── root/                \# Linked if Guix Home is detected (global for Guix Home)  
│   ├── home/                \# Linked if Guix Home is detected (global for Guix Home)  
│   ├── os/                  \# Per Guix Operating System definition  
│   │   └── \<system\_name\>/   \# Requires $BOS\_SYSTEM\_NAME to be set  
│   │       ├── root/  
│   │       └── home/  
│   └── user/                \# Per Guix Home Environment definition  
│       └── \<home\_env\_name\>/ \# Requires $BOS\_HOME\_NAME to be set  
│           ├── root/  
│           └── home/  
└── nix/                     \# Nix Profile/Home-Manager specific dotfiles (functions identically to guix/)  
    ├── root/  
    ├── home/  
    ├── os/  
    │   └── \<system\_name\>/  
    │       ├── root/  
    │       └── home/  
    └── user/  
        └── \<profile\_or\_home\_env\_name\>/  
            ├── root/  
            └── home/

**Example of Automatic Linking:**

A file located at os/linux/home/.config/nvim/init.vim within your dotfile source would be automatically linked to $HOME/$USER/.config/nvim/init.vim if DotsCLI detects the operating system as "linux".

**Explanation of Contextual Directories:**

* **root (global):** Files here are linked relative to the filesystem root (/). Use with caution, as this typically requires superuser privileges.  
* **home (global):** Files here are linked relative to the current user's home directory ($HOME/$USER/). This is the most common location for dotfiles.  
* **os/\<os\_identifier\>:** Dotfiles within these directories are linked only if the current operating system matches \<os\_identifier\>. The tool attempts to automatically detect the OS. For GuixOS, use guix as the \<os\_identifier\>.  
* **user/\<username\>:** Dotfiles here are linked only if the user executing DotsCLI matches \<username\>.  
* **guix/ and nix/:** These directories provide specialized handling for Guix and Nix environments.  
  * The top-level guix/root and guix/home (and similarly for nix/) are linked if a home manager context is detected (e.g., guix home command is available for Guix, or specific Nix environment variables/paths exist for Nix). This is useful for dotfiles that should apply whenever that specific home manager is active.  
  * For OS-level Guix/Nix configurations (e.g., a full Guix System Definition), it is generally recommended to use the os/guix/ or os/nix/ path for clarity, even though the specific guix/os/ or nix/os/ paths exist.  
  * The guix/os/\<system\_name\>/ and nix/os/\<system\_name\>/ subdirectories require the environment variable $BOS\_SYSTEM\_NAME to be set to \<system\_name\>.  
  * The guix/user/\<home\_env\_name\>/ and nix/user/\<profile\_or\_home\_env\_name\>/ subdirectories require $BOS\_HOME\_NAME (or a Nix-equivalent variable for user profiles/home-manager environments) to be set to the respective name. These are intended for configurations specific to a particular Guix Home Environment declaration or Nix setup.  
  * The tool([https://github.com/ghgsrt/boscli](https://github.com/ghgsrt/boscli)) (if used) can assist in managing these environment variables.

This structured approach ensures that dotfiles are organized logically within the source repository and can be deployed selectively and automatically based on the operational context. This systematic organization is crucial for maintaining clarity and manageability as the number of dotfiles and supported environments grows.

## **Installation**

*(Installation instructions would typically go here. As this is a conceptual improvement of a README, specific package manager commands or build steps are omitted but would be included in a real-world scenario, similar to examples found in 1 or.2)*

Example for a Rust project (if applicable):

Bash

cargo install dotscli

Or, if distributed via other package managers:

Bash

\# Example for pip (if it were a Python tool)  
pip install dotscli

\# Example for a generic binary download  
\# Download the binary from the releases page and place it in your $PATH

### **Prerequisites**

* The bash shell is generally assumed for script execution if any helper scripts are involved.  
* For Guix/Nix specific features, the respective Guix or Nix tools must be installed and configured on the system.  
* (Any other dependencies, like git for cloning remote repositories, should be listed here.)

## **Usage**

DotsCLI maintains a "trackfile" (typically located at $HOME/.local/share/dots/trackfile.json) to record the symlinks it manages. This file is crucial for the tool to understand which links it "owns" and can therefore modify or remove with greater confidence.

### **Nomenclature**

Understanding the following terms is important when interpreting DotsCLI's output and documentation:

* **Source Set:** The collection of dotfiles defined by a specific target (e.g., a TOML file, a directory).  
* **Trackfile:** A file maintained by DotsCLI listing all symlinks it has created, their source files, and target locations.  
* **Correct Symlink:** A symlink that:  
  1. Exists in the filesystem.  
  2. Is listed in the trackfile.  
  3. Points to the exact source file path specified for it in the trackfile (the "expected source" according to the trackfile).  
* **Intended Symlink:** A symlink that:  
  1. Exists in the filesystem.  
  2. Is listed in the trackfile.  
  3. Points to the source file that the *current command* expects it to point to (the "intended source" for the current operation), regardless of what the trackfile says its original source was. This is relevant during link operations where the source might change.  
* **Dangling Symlink:** A symlink that exists in the filesystem and is listed in the trackfile, but its target source file no longer exists. DotsCLI generally considers these safe to remove or overwrite.  
* **Foreign File/Symlink:** A file or symlink that exists at a target location but is *not* listed in DotsCLI's trackfile. These are treated with caution.

### **Basic Command Structure**

Bash

dots \<command\> \[shared\_options\]\[command\_specific\_options\]\[arguments\]

### **Shared Command Options**

The link, unlink, and relink commands share a set of common options to control their behavior:

| Option | Short | Description |
| :---- | :---- | :---- |
| \--include \<path\> | \-i | Explicitly include only the specified file or directory path from the source set. Can be used multiple times. If used, only these paths are processed. |
| \--exclude \<path\> | \-e | Explicitly exclude the specified file or directory path from the source set. Can be used multiple times. |
| \--verbose | \-v | Enable verbose output, providing more detailed information about operations. |
| \--dry-run |  | Perform a dry run. Actions will be logged as if they were performed, but no changes will be made to the filesystem. Essential for previewing operations. 1 |
| \--bail |  | If a potentially destructive action is encountered that isn't covered by a \--force-\* flag, the operation will throw an error and halt execution. Default behavior is to skip such actions. |
| \--interactive |  | Prompt for user confirmation before performing potentially destructive actions (e.g., overwriting an existing file or a foreign symlink). This flag is overridden by more specific \--force-\* flags. |
| \--force-correct-symlink | \-fc | **unlink only:** Allows unlink to remove a "correct symlink" even if it's not an "intended symlink" for the current operation (i.e., it points to the source specified in the trackfile, but not necessarily the source in the current unlink command's target). By default, unlink only removes "intended symlinks". |
| \--force-symlink | \-fs | Potentially destructive actions (like overwriting or unlinking) may apply to *any* symlink encountered at a target path, provided it is listed in the trackfile (i.e., it's either "correct" or "intended," or even a "dangling" symlink managed by DotsCLI). This is more permissive than default behavior. |
| \--force-file | \-ff | Potentially destructive actions may apply to *any* file or symlink at a target path, as long as it is listed in the trackfile. This includes regular files that DotsCLI previously created (if such a feature existed) or symlinks. |
| \--force-dangerously |  | **Use with extreme caution.** Potentially destructive actions may apply to *any* file or symlink encountered at a target path, regardless of whether it is in the trackfile or what its current state is. This can overwrite unrelated files. |

The force flags provide a hierarchy of assertiveness. Using \--force-dangerously implies the behavior of all other force flags. These options are critical for scripting and for handling edge cases where the default conservative behavior is not desired. The existence of a \--dry-run option is a best practice, allowing users to verify the intended changes before committing them.1

### **Commands**

#### ---

**link \[\<target\>\]**

**Alias:** ln

This command attempts to create symlinks for all files resolved from the specified \<target\>.

* **Argument:**  
  * \<target\>: (Optional if a default is set in global config) A path to:  
    * A directory containing dotfiles structured according to the(\#dotfile-directory-structure).  
    * A TOML configuration file defining dotfile sources.  
    * A URL to an external Git repository containing dotfiles.

**Behavior:**

* **Additive Operation:** New links are added to the trackfile. Existing entries in the trackfile are updated if the source file for a given target path changes.  
* **Default Handling of Existing Symlinks:**  
  * If an existing symlink at a target location is **dangling** (points to a non-existent source but is in the trackfile), it will be replaced.  
  * If an existing symlink is **intended** (already points to the source file this link command wants to link), it will be updated (effectively a refresh).  
  * If an existing symlink is **correct** (points to the source specified in the trackfile, which might differ from the current command's intended source), it will be replaced by a new symlink pointing to the new intended source.  
* **Handling Other Conflicts (default, without force flags):**  
  * If a **foreign symlink** (not in trackfile) exists at the target, the link operation for that path is skipped.  
  * If a **regular file** exists at the target, the link operation for that path is skipped.  
  * Use \--interactive or appropriate \--force-\* flags to manage these conflicts.

#### ---

**unlink \[\<target\>\]**

**Alias:** un

This command attempts to remove symlinks for files resolved from the specified \<target\>.

* **Argument:**  
  * \<target\>: (Optional, see \--hard) A path to a directory, TOML file, or Git repository, similar to the link command.

**Behavior:**

* By default, this command only removes **intended symlinks**: symlinks that are listed in the trackfile and point to the corresponding source file within the provided \<target\> source set.  
* Removed symlinks are also removed from the trackfile.

**Command-Specific Option:**

* \--hard:  
  * This flag should be provided **instead of** a \<target\> argument.  
  * When \--hard is used, the "target" becomes the entire existing trackfile.  
  * By default (without other force flags), this will remove all **correct symlinks** (symlinks that exist and point to their tracked source).  
  * **The trackfile itself will be destroyed after this operation.** This effectively makes DotsCLI "forget" all the files it was managing.

#### ---

**relink \[\<target\>\]**

**Alias:** re

This command is a convenient shorthand that first performs an unlink operation and then a link operation on the given \<target\>.

* **Argument:**  
  * \<target\>: (Required) A path to a directory, TOML file, or Git repository.

**Behavior:**

* The unlink phase behaves as dots unlink \<target\> (respecting options like \-fc, \-fs, etc., if provided to relink).  
* The link phase behaves as dots link \<target\> (respecting shared options).  
* This is useful for ensuring a clean state or when source paths within a dotfile set might have changed.

**Command-Specific Option:**

* \--hard:  
  * This flag should be provided **in addition to** the \<target\> argument.  
  * It modifies the operation to be equivalent to:  
    1. dots unlink \--hard (removes all currently tracked correct symlinks and destroys the trackfile)  
    2. dots link \<target\> (links the new target into a fresh trackfile)  
  * This is the most assertive way to completely swap out all managed dotfiles with a new set.

## **Configuration (TOML)**

DotsCLI uses TOML (Tom's Obvious, Minimal Language) for its configuration files. This allows for a human-readable, declarative approach to defining both tool behavior and dotfile sources.2

There are two main parts to the configuration, which can coexist in the same TOML file:

1. **Tool Configuration:** Settings that affect DotsCLI's global behavior.  
2. **Dotfiles Definition:** Declarations of dotfile sources and their linking properties.

### **Configuration File Locations**

DotsCLI looks for its main configuration file in the following locations, in order of precedence (the first one found is used):

1. $XDG\_CONFIG\_HOME/dots/config.toml (e.g., $HOME/.config/dots/config.toml)  
2. $XDG\_CONFIG\_HOME/bos/config.toml (e.g., $HOME/.config/bos/config.toml, for compatibility if BosCLI is used)  
3. $HOME/.dots (if it's a TOML file)  
4. $HOME/.bos (if it's a TOML file, for BosCLI compatibility)

The **Dotfiles Definition** part of a configuration file (the \[\[dotfiles\]\] array) will always be processed if the file is specified as a \<target\> to commands like dots link. If a Dotfiles Definition is present in one of the global configuration paths listed above, it can serve as the default target if no \<target\> argument is provided to a command.

### **DotsCLI Tool Configuration (\[dots\])**

This section, denoted by the \[dots\] table in your TOML file, is for configuring the tool itself.

Ini, TOML

\# Example: $HOME/.config/dots/config.toml

\[dots\]  
\# default\_target \= "\~/.config/dots/main\_dotfiles.toml" \# Example of a potential future key for default dotfile set  
\# No specific tool-wide configurations are defined yet.  
\# This section is reserved for future enhancements, such as setting a default  
\# dotfiles TOML file, global verbosity, or default force levels.

Currently, there are no globally configurable options under the \[dots\] table, but it is reserved for future expansion.

### **Dotfiles Definition (\[\[dotfiles\]\])**

This is the core of DotsCLI's declarative power, allowing users to specify multiple dotfile sources and how they should be processed. Each source is defined as a table in an array of tables named dotfiles. The structure and available keys are detailed below, drawing inspiration from robust configuration practices seen in tools like Flit for pyproject.toml.3

Ini, TOML

\# Example of a \[\[dotfiles\]\] entry  
\[\[dotfiles\]\]  
path \= "\~/my\_local\_dots\_repo" \# Source: a local directory  
\# includes \= \["home/.bashrc", "home/.config/nvim/"\]  
\# excludes \= \["home/.git"\]  
\# replace \= true \# Default behavior

\[\[dotfiles\]\]  
path \= "https://github.com/username/common-dotfiles.git" \# Source: a remote Git repository  
replace \= false \# Files from this source will NOT overwrite conflicting files from the previous entry

\[\[dotfiles\]\]  
path \= "specific\_configs.toml" \# Source: another TOML file for composition  
\# This allows for modular configuration.

#### **Dotfiles Configuration Keys (\[\[dotfiles\]\])**

The following table details the keys available within each \[\[dotfiles\]\] table entry:

| Key Name | TOML Type | Required? | Default Value | Description |
| :---- | :---- | :---- | :---- | :---- |
| path | String | **Yes** | N/A | Specifies the source of the dotfiles. Can be: \<br\>1. A local directory path (e.g., \~/dotfiles, /etc/shared-dots). \<br\>2. A URL to an external Git repository (e.g., https://github.com/user/repo.git). \<br\>3. A path to another TOML file, allowing for composition of configurations. |
| replace | Boolean | No | true | If true, and if both this \[\[dotfiles\]\] entry and a *prior* \[\[dotfiles\]\] entry within the same configuration file specify a file to be linked to the same conflicting path, then this \[\[dotfiles\]\] entry's file takes precedence ("last one wins" semantics for conflicting paths from different sources). If false, files from this source will *not* overwrite conflicting files from prior sources. **Note:** The order of \[\[dotfiles\]\] entries in the TOML file can be significant. |
| includes | Array of Strings | No | \`\` (empty) | A list of paths for files or directories *within the source specified by path* to explicitly include. Paths must be relative to one of the top-level directories defined in the(\#dotfile-directory-structure) (e.g., home/.bashrc, os/linux/root/etc/). If this field is not empty, **only** the files/directories specified here (and their descendants, if directories) will be considered from this source. For mapping arbitrary paths, use \[dotfiles.map\]. |
| excludes | Array of Strings | No | \`\` (empty) | A list of paths for files or directories *within the source specified by path* to explicitly exclude. Paths must be relative as described for includes. Excluded files/directories will not be linked. excludes take precedence over includes if a path matches both. |
| \[dotfiles.map\] | Table | No | {} (empty) | A table for manually mapping files or directories from *within the source specified by path* to specific locations on the filesystem. \<br\> \- **Keys:** Relative path within the path source (e.g., custom\_scripts/my\_script.sh). \<br\> \- **Values:** Absolute target path on the filesystem (e.g., /usr/local/bin/my\_script) or a path relative to home using \~ (e.g., \~/.local/bin/my\_script). \<br\> Mapped items are still subject to includes (if includes is non-empty, mapped items must be part of the included set) and excludes (if a mapped item is excluded, it won't be linked). |

**Important Note on Circularity:** DotsCLI currently does not implement guards against circular dependencies if one TOML configuration file's path points to another TOML file that, in turn, points back to the first (or creates a longer loop). Users should exercise caution to avoid such circular references in their configurations.

The detailed specification of these keys, including their types, whether they are required, default values, and precise descriptions, is crucial for users to effectively leverage the TOML configuration. This structured approach to documentation minimizes ambiguity and helps prevent common configuration errors, enabling users to build complex and reliable dotfile management setups.

### **TOML Configuration Examples**

**1\. Simple Local Directory Source:**

Ini, TOML

\# main.toml  
\[\[dotfiles\]\]  
path \= "\~/Documents/my-dotfiles-collection"  
\# All compatible files within \~/Documents/my-dotfiles-collection/home, \~/Documents/my-dotfiles-collection/root, etc.,  
\# will be linked according to the standard directory structure.

**2\. Sourcing from a Remote Git Repository:**

Ini, TOML

\# main.toml  
\[\[dotfiles\]\]  
path \= "https://github.com/yourusername/your-dotfiles-repo.git"  
\# DotsCLI will clone this repository into a temporary cache and process its contents.

**3\. Using includes and excludes:**

Ini, TOML

\# main.toml  
\[\[dotfiles\]\]  
path \= "\~/comprehensive-dots"  
includes \= \[  
  "home/.config/nvim",      \# Include the entire nvim config directory  
  "home/.zshrc",            \# Include the.zshrc file  
  "os/linux/home/.Xresources" \# Include a Linux-specific file  
\]  
excludes \= \[  
  "home/.config/nvim/lua/plugins\_dev.lua" \# Exclude a specific file from the included nvim config  
\]

**4\. Using Manual Mapping with \[dotfiles.map\]:**

Ini, TOML

\# main.toml  
\[\[dotfiles\]\]  
path \= "\~/special-configs"  
includes \= \[ \# If includes is used, mapped items must be covered or be top-level items in map's source.  
    "scripts/utility.sh"  
\]  
excludes \= \[  
    "old\_configs/" \# Exclude an entire directory from the source  
\]

\[dotfiles.map\]  
"scripts/utility.sh" \= "\~/.local/bin/my-utility" \# Maps source file to a specific target path  
"configs/app.conf" \= "/etc/custom\_apps/app.conf" \# Requires root if linking to /etc  
\# "assets/wallpaper.jpg" \= "\~/Pictures/Wallpapers/current\_wallpaper.jpg" \# Another example

*If includes is non-empty, files specified in dotfiles.map keys must effectively be part of the included set, or they won't be processed. For instance, if includes \= \["configs/"\], then configs/app.conf would be processed for mapping. If includes \= \["scripts/utility.sh"\], then scripts/utility.sh is processed. However, assets/wallpaper.jpg would not be processed unless assets/ or assets/wallpaper.jpg was also in includes.*

**5\. Multiple Sources with replace Behavior:**

Ini, TOML

\# main.toml

\# Base set of dotfiles  
\[\[dotfiles\]\]  
path \= "https://github.com/generic/base-configs.git"  
\# replace \= true (default)

\# Overrides or additions for development machine  
\[\[dotfiles\]\]  
path \= "\~/dev-machine-specific-configs"  
replace \= true \# Default, but explicit: files from here will overwrite conflicts from base-configs.git  
includes \= \[  
    "home/.config/git/config" \# Only take the git config from this source  
\]

\# Another set, but these won't overwrite previous ones if conflicts arise  
\[\[dotfiles\]\]  
path \= "\~/experimental-configs"  
replace \= false \# Files from here will NOT overwrite conflicts from the previous two sources.

In this example, if base-configs.git and \~/dev-machine-specific-configs both provide a file that would link to home/.config/git/config, the version from \~/dev-machine-specific-configs will be used because its replace is true (or defaulted to true) and it appears later in the TOML file. If \~/experimental-configs also provided a home/.config/git/config, it would *not* be linked if a version from a prior source (with replace \= true) already claimed that target path, due to replace \= false on the experimental set.

## **Advanced Examples & Use Cases**

These examples illustrate how to combine DotsCLI's features for common dotfile management scenarios.4

**1\. Setting Up a New Machine with a Master Git Repository:**

Imagine you store all your primary dotfiles in a Git repository, say https://github.com/me/master-dots.git. You also have a main.toml file within that repository (or locally) that defines how to use its contents:

* **\~/my\_dot\_configs/main.toml:**  
  Ini, TOML  
  \[\[dotfiles\]\]  
  path \= "https://github.com/me/master-dots.git" \# Points to your main dotfiles  
  \# This source might contain general 'home/' files and 'os/linux/home', 'os/macos/home' etc.

* **On the new machine:**  
  1. Install DotsCLI.  
  2. Obtain main.toml (e.g., clone a small bootstrap repo, or download it).  
  3. Run: dots link \~/my\_dot\_configs/main.toml DotsCLI will clone master-dots.git, identify the current OS, and link the appropriate files.

**2\. Managing OS-Specific Configurations:**

Your master-dots.git repository might be structured like this:

master-dots/  
├── home/  
│   └──.gitconfig         \# Common for all OS  
├── os/  
│   ├── linux/  
│   │   └── home/  
│   │       └──.bashrc    \# Linux-specific bashrc  
│   ├── macos/  
│   │   └── home/  
│   │       └──.bashrc    \# macOS-specific bashrc

When dots link is run using a TOML file pointing to this repository, DotsCLI automatically detects the OS. On a Linux machine, it links os/linux/home/.bashrc to \~/.bashrc. On macOS, it links os/macos/home/.bashrc. The common home/.gitconfig is linked on both.

**3\. Using with Guix Home Environments:**

Suppose you have a Guix Home Environment named my-dev-env and specific dotfiles for it.

* **Repository Structure (master-dots.git or a dedicated Guix config repo):**  
  guix/  
  └── user/  
      └── my-dev-env/  
          └── home/  
              └──.config/  
                  └── custom-tool/  
                      └── settings.conf

* **Environment Setup:** Ensure $BOS\_HOME\_NAME is set to my-dev-env when that Guix Home Environment is active.  
  Bash  
  export BOS\_HOME\_NAME="my-dev-env"

* **Linking:** When dots link \<your\_toml\_pointing\_to\_this\_repo\> is run in this environment, guix/user/my-dev-env/home/.config/custom-tool/settings.conf will be linked to $HOME/.config/custom-tool/settings.conf.

**4\. Overriding Specific Files from a Larger Set:**

You use a comprehensive shared dotfile set but want to override just your tmux.conf on a particular server.

* **server\_setup.toml:**  
  Ini, TOML  
  \# Source 1: Comprehensive shared dotfiles  
  \[\[dotfiles\]\]  
  path \= "https://github.com/shared/company-dots.git"  
  \# replace \= true (default)

  \# Source 2: Server-specific overrides  
  \[\[dotfiles\]\]  
  path \= "\~/server-overrides" \# This directory contains only 'home/.tmux.conf'  
  replace \= true \# Ensures this.tmux.conf overwrites the one from company-dots  
  includes \= \[  
      "home/.tmux.conf" \# Only process.tmux.conf from this source  
  \]  
  When dots link server\_setup.toml is run, the home/.tmux.conf from \~/server-overrides will be linked to \~/.tmux.conf, even if company-dots.git also provided one. Other files from company-dots.git will be linked as usual.

**5\. Composing Configurations with Multiple TOML Files:**

For very complex setups, you can break down configurations into multiple TOML files.

* **base.toml:**  
  Ini, TOML  
  \[\[dotfiles\]\]  
  path \= "\~/core-dotfiles" \# Essential, always-on dotfiles

* **dev\_tools.toml:**  
  Ini, TOML  
  \[\[dotfiles\]\]  
  path \= "\~/dev-tool-configs" \# Configurations for development tools (nvim, zsh plugins, etc.)  
  includes \= \["home/.config/nvim", "home/.zsh\_plugins"\]

* **main\_workstation.toml (imports the others):**  
  Ini, TOML  
  \# Import base configuration  
  \[\[dotfiles\]\]  
  path \= "base.toml"

  \# Import development tool configurations  
  \[\[dotfiles\]\]  
  path \= "dev\_tools.toml"

  \# Workstation-specific additions/overrides  
  \[\[dotfiles\]\]  
  path \= "\~/workstation-specific"  
  \[dotfiles.map\]  
  "scripts/monitor\_setup.sh" \= "\~/.local/bin/setup\_monitors"  
  Running dots link main\_workstation.toml will process all three TOML files, layering their configurations. This modular approach enhances organization and reusability. These practical scenarios demonstrate how the declarative nature of DotsCLI, combined with its structured directory conventions and TOML-based configuration, can simplify even sophisticated dotfile management requirements.

## **LSP Support (Future)**

There are plans to develop a Language Server Protocol (LSP) implementation for DotsCLI configuration files. This will enhance the editing experience for .\[bos|dots\]\[.toml\] files in compatible text editors and IDEs.5

Potential benefits of LSP support include:

* **Schema Validation:** Real-time validation of TOML syntax and DotsCLI-specific configuration schema (e.g., correct keys like path, includes, valid data types).  
* **Autocompletion:** Suggestions for valid keys, and potentially for values where applicable (e.g., boolean true/false).  
* **Diagnostics:** Identification of common errors, such as incorrect path structures within includes/excludes, or malformed URLs.  
* **Hover Information:** Descriptions for keys and their expected values on hover.

The development of an LSP server underscores a commitment to improving the developer experience and reducing the friction associated with managing complex configurations.

## **Contributing**

Contributions to DotsCLI are welcome\! Whether it's reporting a bug, suggesting an enhancement, or contributing code, your input is valuable. Please adhere to the following guidelines:

* **Reporting Bugs:** Use the GitHub Issues tracker for the project. Provide a clear description of the bug, steps to reproduce it, your operating system, DotsCLI version, and any relevant logs or configuration files.  
* **Suggesting Features:** Submit feature requests via GitHub Issues. Explain the proposed functionality and its benefits.  
* **Development Setup:** (Details on how to set up a local development environment, build the project, and run tests would be provided here.)  
* **Pull Requests:**  
  1. Fork the repository.  
  2. Create a new branch for your feature or bugfix (git checkout \-b feature/my-new-feature or git checkout \-b fix/issue-number).  
  3. Commit your changes with clear, descriptive commit messages.  
  4. Ensure your code adheres to any specified coding style or linting requirements.  
  5. Push your branch to your fork (git push origin feature/my-new-feature).  
  6. Open a pull request against the main DotsCLI repository. Provide a detailed description of your changes.  
* **Code of Conduct:** Please note that this project is released with a Contributor Code of Conduct. By participating in this project you agree to abide by its terms. (A CODE\_OF\_CONDUCT.md file should be present in the repository).

Clear guidelines for contribution are essential for fostering a collaborative and productive open-source community.4

## **License**

DotsCLI is licensed under the MIT License. See the LICENSE file in the root of the project repository for the full license text.
