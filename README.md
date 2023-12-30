# 🧀 Brie

[![MIT/Apache 2.0](https://img.shields.io/badge/license-MIT%2FApache-blue.svg)](https://github.com/nikarh/brie#license)
[![GitHub Workflow Status (with event)](https://img.shields.io/github/actions/workflow/status/nikarh/brie/main)](https://github.com/nikarh/brie/actions/workflows/main.yaml)
[![Current Release](https://img.shields.io/github/release/nikarh/brie.svg)](https://github.com/nikarh/brie/releases)
[![Release RSS Feed](https://img.shields.io/badge/rss-releases-ffa500?logo=rss)](https://github.com/nikarh/brie/releases.atom)
[![Main Commits RSS Feed](https://img.shields.io/badge/rss-commits-ffa500?logo=rss)](https://github.com/nikarh/brie/commits/main.atom)

Brie is a CLI toolset for running Windows games via [Wine] in isolated prefixes, which also
  - Adds units to [Steam] as non-Steam games
  - Creates `.desktop` files
  - Manages [Sunshine] config

Much like [Lutris] and [Heroic Game Launcher], this tool tries to set up an environment closely resembling [Proton] for running games.
Unlike these tools, Brie aims to be CLI-first, with all runnable units and the configuration defined by a user in a single YAML file.

Originally this project started as a [shell script](https://github.com/nikarh/brie/blob/b4e09a0714f15c92a93504be32fba6428bd0dabf/play.sh), which at some point became too inconvenient to maintain and debug.

## Goals

Even though on the surface project shares similarities with [Lutris] and [Heroic Game Launcher], this project has different goals:

  - Designed to be CLI-first
  - The configuration for the runnable units is defined in a single YAML text file
  - Automatically download necessary [wine] distribution, and libraries and create an isolated wine prefix per unit
  - Automatically create `.desktop` files for configured units, as well as add them to [Sunshine] and [Steam] as non-Steam games

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
    - Ensures file associations are [not propagated to the host](https://wiki.winehq.org/FAQ#How_can_I_prevent_Wine_from_changing_the_filetype_associations_on_my_system_or_adding_unwanted_menu_entries.2Fdesktop_links.3F)
  - Installs downloaded libraries
  - Runs `winetricks`
  - Runs additional preparation scripts
  - Creates symlinks to mount letters provided in the config
- Sets the environment variables and launches the unit in the isolated Wine prefix with the requested runtime. Can optionally run the unit with additional tools if configured (e.g. `gamemoderun` and `mangohud`)

### briectl

`briectl` is responsible for additional features not necessarily related to launching units.

- Download icons and banners from the [SteamGridDB]
- Generate `.desktop` files for units
- Add units to the [Sunshine] configuration file
- Add units to [Steam] as anon-Steam games


## Paths

Brie uses [xdg] to determine where configuration and relevant data are stored.
Most commonly it would be:

 - Configuration in `~/.config/brie/brie.yaml`
 - Application state in `~/.local/share/brie`:
   - Wine prefixes in `~/.local/share/brie/prefixes`
   - Cached wine and libraries in `~/.local/share/brie/libraries`
   - Cached images (banners, icons) in `~/.local/share/brie/images`

## Configuration example

```yaml
x-wine-defaults: &wine-defaults
  runtime:
    kind: ge-proton
    version: "*"
  libraries:
    dxvk-nvapi: "*"
    dxvk-gpl-async: "*"
    vkd3d-proton: "*"
    nvidia-libs: "*"
  env: &wine-env
    MANGOHUD_CONFIG: no_display,vram,gpu_temp,gpu_core_clock,frametime
    DXVK_ASYNC: "1"
    DXVK_GPLASYNCCACHE: "1"
    DXVK_ENABLE_NVAPI: "1"
    WINE_HIDE_NVIDIA_GPU: "0"
    WINEESYNC: "0"
    VKDED_CONFIG: dxr

x-wine-game-defaults: &wine-game-defaults
  <<: *wine-defaults
  wrapper:
    - gamemoderun
    - mangohud
  generate:
    sunshine: true
    desktop: true
    steam_shortcut: false

x-wine-soft-defaults: &wine-soft-defaults
  <<: *wine-defaults
  generate:
    sunshine: false
    desktop: true
    steam_shortcut: false

steamgriddb_token: PLACE_YOUR_TOKEN_HERE

paths:
  steam_config: ~/.var/app/com.valvesoftware.Steam/.local/share/Steam/userdata/{YOUR_ID}/config
  sunshine: ~/.config/sunshine/all.json
  desktop: ~/.local/share/applications/brie/

units:
  ltspice:
    # Use YAML anchors to simplify the config https://yaml.org/spec/1.2.2/#3222-anchors-and-aliases
    <<: *wine-soft-defaults
    name: "LTSpice"
    command: ["C:/users/wine/AppData/Local/Programs/ADI/LTspice/LTspice.exe"]
    winetricks:
      - vcrun2019
  foobar:
    <<: *wine-soft-defaults
    name: "Foobar 2000"
    command: ["C:/Program Files (x86)/foobar2000/foobar2000.exe"]
    winetricks: ["vcrun2015"]
    mounts:
      d: ~/Music
  witcher3:
    <<: *wine-game-defaults
    name: "The Witcher 3: Wild Hunt"
    cd: /mnt/files/Games/The Witcher 3 Wild Hunt/bin/x64/
    command: ["witcher3.exe"]
    # Not necessary for this particular title, this serves just as a capability example:
    winetricks:
      - vcrun2019
      - d3dcompiler_42
      - d3dcompiler_47
    before:
      - ["winecfg", "-v", "win10"]
    env:
      <<: *wine-env
      VKD3D_SHADER_DEBUG: none
  # Non-wine (native) units are also supported for the purpose of adding them to sunshine config (or non-steam games)
  steam:
    type: native
    name: "Steam: Big Picture Mode"
    wrapper:
      - gamemoderun
      - mangohud
    generate:
      sunshine: true
    steamgriddb_id: 2332
    command:
      ["flatpak", "run", "com.valvesoftware.Steam", "-bigpicture"]
```

## License

Except where noted (below and/or in individual files), all code in this repository is dual-licensed at your option under either:

* MIT License ([LICENSE-MIT](LICENSE-MIT) or [http://opensource.org/licenses/MIT](http://opensource.org/licenses/MIT))
* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or [http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0))

[Wine]: https://www.winehq.org/
[sunshine]: https://github.com/LizardByte/Sunshine
[Steam]: https://store.steampowered.com/
[SteamGridDB]: https://www.steamgriddb.com/
[Lutris]: https://lutris.net/
[Heroic Game Launcher]: https://heroicgameslauncher.com/
[Proton]: https://github.com/ValveSoftware/Proton
[xdg]: https://specifications.freedesktop.org/basedir-spec/basedir-spec-latest.html
