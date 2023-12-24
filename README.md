# 🧀 Brie

[![MIT/Apache 2.0](https://img.shields.io/badge/license-MIT%2FApache-blue.svg)](https://github.com/nikarh/brie#license)
[![GitHub Workflow Status (with event)](https://img.shields.io/github/actions/workflow/status/nikarh/brie/release)](https://github.com/nikarh/reaper-remote-bandui/actions/workflows/release.yaml)
[![Current Release](https://img.shields.io/github/release/nikarh/brie.svg)](https://github.com/nikarh/brie/releases)
[![Release RSS Feed](https://img.shields.io/badge/rss-releases-ffa500?logo=rss)](https://github.com/nikarh/brie/releases.atom)
[![Main Commits RSS Feed](https://img.shields.io/badge/rss-commits-ffa500?logo=rss)](https://github.com/nikarh/brie/commits/main.atom)

Brie is a CLI toolset for running Windows software via [Wine](https://www.winehq.org/) in isolated prefixes.

Much like [Lutris](https://lutris.net/) and [Heroic Game Launcher](https://heroicgameslauncher.com/), this tool tries to set up an environment closely resembling [Proton](https://github.com/ValveSoftware/Proton) for running games.
Unlike these tools, Brie aims to be CLI-first, with all runnable units and the configuration defined by a user in a single YAML file.

## Goals

Even though on the surface project shares similarities with [Lutris](https://lutris.net/) and [Heroic Game Launcher](https://heroicgameslauncher.com/), this project has totally different goals:

- Brie is designed to be CLI-first
- The configuration for the runnable units is in a single YAML text file
- Brie automatically creates `.desktop` files for configured units, as well as adds them to [sunshine](https://github.com/LizardByte/Sunshine) and [Steam](https://store.steampowered.com/) as non-Steam games

## CLI tools

The project provides two CLI tools.

### brie

`brie` is the unit launcher.

- The launcher uses `YAML` manifest, containing definitions of units. Each unit defines the executable that should be run, details about the wine prefix, additional steps (`winetricks`), and library dependencies.
- Before launching the unit, the tool:
  - Downloads optional dependencies with their corresponding versions defined for the unit:
    - [wine-ge-custom](https://github.com/GloriousEggroll/wine-ge-custom)
    - [dxvk](https://github.com/doitsujin/dxvk)
    - [dxvk-gplasync](https://gitlab.com/Ph42oN/dxvk-gplasync)
    - [dxvk-nvapi](https://github.com/jp7677/dxvk-nvapi)
    - [vkd3d-proton](https://github.com/HansKristian-Work/vkd3d-proton)
    - [nvidia-libs](https://github.com/SveSop/nvidia-libs) for `nvcuda`, `nvoptix` and `nvml`.
  - Creates a Wine prefix
    - Unlinks symlinks to `~/{Downloads,Documents}` and other folders
    - Ensures file associations are not propagated to the host
  - Installs downloaded libraries
  - Runs `winetricks` and "before" scripts
  - Creates symlinks to mount letters provided in the config
- Sets the environment variables and launches the unit in the isolated Wine prefix with the requested runtime. Can optionally run the unit with additional tools if configured (e.g. `gamemoderun` and `mangohud`)

### briectl

`briectl` is responsible for additional features not necessarily related to launching units.

- Download icons and banners from the [SteamGridDB](https://www.steamgriddb.com/)
- Generate `.desktop` files for units
- Add units to [sunshine](https://github.com/LizardByte/Sunshine) configuration file
- Add units to [Steam](https://store.steampowered.com/) as non-Steam games

## License

Except where noted (below and/or in individual files), all code in this repository is dual-licensed at your option under either:

* MIT License ([LICENSE-MIT](LICENSE-MIT) or [http://opensource.org/licenses/MIT](http://opensource.org/licenses/MIT))
* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or [http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0))

