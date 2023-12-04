#!/usr/bin/bash -e

if [[ -n "$DEBUG" ]]; then
    set -x
    export WINEDEBUG=warn+all
else 
    export WINEDEBUG=-all
fi

# This is a dirty hack for now
# The main reason why this exists is that I want to load a pw TCP socket module
# only for a duration of the app, so we can't just run this script as a different user,
# we also must prepare pw and clean it up afterwards. Also we we must keep in mind that
# multiple apps may be running, so cleanup should be handled only when all processes are complete
if [[ "$1" == "--as-user" ]]; then
    # Make pipewire accessible via a TCP socket for a remote session
    (
        flock -s 200
        COUNTER=$(cat /tmp/.${USER}-play.sh-counter 2>/dev/null || echo "0")
        echo $((COUNTER+1)) >| /tmp/.${USER}-play.sh-counter

        if [[ "$COUNTER" == "0" ]]; then 
            pactl load-module module-native-protocol-tcp listen=127.0.0.1 > /dev/null || true
        fi
    ) 200>/tmp/.${USER}-play.sh-lock

    set +e
    PULSE_SERVER="tcp:localhost" sudo -u "$2" play.sh "${@:3}"
    STATUS="$?"
    set -e

    (
        flock -s 200
        COUNTER=$(cat /tmp/.${USER}-play.sh-counter 2>/dev/null || echo "0")
        echo $((COUNTER-1)) >| /tmp/.${USER}-play.sh-counter

        if [[ "$COUNTER" == "1" ]]; then
	    set +e
            pactl unload-module module-native-protocol-tcp || true
	    set -e
        fi
    ) 200>/tmp/.${USER}-play.sh-lock

    exit $STATUS
fi

# Another hack when running with sudo for another user
if [[ -z "$DBUS_SESSION_BUS_ADDRESS" ]]; then
    export DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/$UID/bus"
fi


cd "$(dirname "$(readlink -f "$0")")" || exit

function expand {
    cat - | sed -r "s:~:$HOME:g"
}

YQ="yq"

XDG_DATA_HOME="${XDG_DATA_HOME:-"$HOME/.local/share"}"
XDG_CONFIG_HOME="${XDG_CONFIG_HOME:-"$HOME/.config"}"
XDG_CACHE_HOME="${XDG_CACHE_HOME:-"$HOME/.cache"}"

CONFIG_DIR="$XDG_CONFIG_HOME/play.sh"
CACHE_DIR="$XDG_CACHE_HOME/play.sh"
DATA_DIR="$XDG_DATA_HOME/play.sh"

YAML=${CONFIG:-"$CONFIG_DIR/games.yaml"}

if ! [ -f "$YAML" ]; then
    echo "Config file does not exist at $YAML"
    exit 1
fi

PREFIXES="$(cat "$YAML" | "$YQ" '.paths.prefixes // "'$DATA_DIR/prefixes'"' | expand)"
RUNTIMES="$(cat "$YAML" | "$YQ" '.paths.runtimes // "'$DATA_DIR/runtimes'"' | expand)"
LIBRARIES="$(cat "$YAML" | "$YQ" '.paths.libraries // "'$DATA_DIR/libraries'"' | expand)"
CACHE="$(cat "$YAML" | "$YQ" '.paths.cache // "'$CACHE_DIR'"' | expand)"

SHELLS_DIR="$(cat "$YAML" | "$YQ" '.paths.shells // "'$XDG_DATA_HOME/bin'"' | expand)"
DESKTOP_DIR="$(cat "$YAML" | "$YQ" '.paths.desktop // "'$XDG_DATA_HOME/applications/play.sh'"' | expand)"
SUNSHINE_CONF="$(cat "$YAML" | "$YQ" '.paths.sunshine // "'$XDG_CONFIG_HOME/sunshine/apps_linux.json'"' | expand)"

LAUNCHER="$(cat "$YAML" | "$YQ" ".launcher // \"$0 {}\"" | expand)"
WRAPPER="$(cat "$YAML" | "$YQ" '.wrapper // ""')"

SGDB_TOKEN="$(cat "$(cat "$YAML" | "$YQ" '.paths.steamgriddb_key // "'$CONFIG_DIR/steamgriddb_key'"')" 2>/dev/null || echo)"

function file-get {
    curl -s -fLo "$2" --create-dirs "$1"
}

function release {
    local version="$2"
    local res=$(curl -s "https://api.github.com/repos/$1/releases/$([[ "$2" != "latest" ]] && echo "tags/")$2")

    echo $(echo "$res" | jq -r '.tag_name')
    echo $(echo "$res" | jq -r '.assets[].browser_download_url' | grep "$3" | head -n 1)
}

function untar {
    mkdir -p "$2"
    tar --strip-components=1 -xf "$1" -C "$2" && rm "$1"
}

function get-steamgriddb-id {
    mkdir -p "$CACHE"
    touch "$CACHE/steamgriddb_ids.json"

    if [[ "$(cat "$YAML" | "$YQ" ".games[\"$1\"].steamgriddb")" == "false" ]]; then
        return
    fi

    # Try to extract from games.yaml
    local id="$(cat "$YAML" | "$YQ" ".games[\"$1\"].steamgriddb_id // \"\"")"
    if [ -n "$id" ]; then
        echo "$id";
        return
    fi

    # Try to extract from cache
    local name="$(cat "$YAML" | "$YQ" ".games[\"$1\"].name")"
    local ids="$(cat "$CACHE/steamgriddb_ids.json" || echo {})"
    if [ -z "$ids" ]; then
        ids="{}"
    fi

    local id="$(echo "$ids" | jq -r ".\"$name\" // \"\"")"
    if [ -n "$id" ]; then
        echo "$id";
        return
    fi

    # Try to find in steamgriddb and cache it
    local steamgridb_id="$(curl -s -H "Authorization: Bearer $SGDB_TOKEN" \
        "https://www.steamgriddb.com/api/v2/search/autocomplete/$(printf %s "$1" | jq -s -R -r @uri)" \
        | jq -r '.data[0].id // ""')"

    echo "$ids" | jq --arg id "$steamgridb_id" ". + {\"$name\": \$id}" >| "$CACHE/steamgriddb_ids.json"
    echo "$steamgridb_id"
}

function install-release {
    mkdir -p "$4"
    if [ -d "$4/$2" ]; then
        return;
    fi

    local release="$(release "$1" "$2" "$3")"
    local version=$(echo "$release" | sed -n 1p)
    local url="$(echo "$release" | sed -n 2p)"
    local path="$4/${url##*/}"

    echo Downloading "$1 $2 ($version)"
    file-get "$url" "$path"
    untar "$path" "$4/$version"

    if [[ "$2" == "latest" ]]; then
        ln -sf "$version" "$4/latest"
    fi
}

function prepare-runtime {
    install-release GloriousEggroll/wine-ge-custom "$1" "\.tar\.xz$" "$RUNTIMES"
}

function prepare-library {
    local version="$(cat "$YAML" | "$YQ" ".libraries[\"$1\"].version // \"\"")"
    local source="$(cat "$YAML" | "$YQ" ".libraries[\"$1\"].source // \"\"")"
    if [ -z "$version" ]; then
        return
    fi

    # Already installed
    if [ -d "$LIBRARIES/$1/$version" ]; then
        return;
    fi

    if [ -z "$source" ]; then
        install-release "$2" "$version" "$3" "$LIBRARIES/$1"
    else
        local path="$LIBRARIES/$1/${source##*/}"
        file-get "$source" "$path"
        untar "$path" "$LIBRARIES/$1/$version"
    fi
    

    if [ -n "$4" ] && ! [ -f "$LIBRARIES"/$1/$version/setup_$1.sh ]; then
        file-get "$4" "$LIBRARIES"/$1/$version/setup_$1.sh
    fi
}

function prepare-libaries {
    mkdir -p "$LIBRARIES"

    prepare-library dxvk         doitsujin/dxvk                 ".*\.tar\.gz$" "https://aur.archlinux.org/cgit/aur.git/plain/setup_dxvk.sh?h=dxvk-bin"
    prepare-library dxvk-async   Sporif/dxvk-async              ".*\.tar\.gz$"
    prepare-library dxvk-nvapi   jp7677/dxvk-nvapi              ".*\.tar\.gz$" "https://aur.archlinux.org/cgit/aur.git/plain/setup_dxvk_nvapi.sh?h=dxvk-nvapi-mingw"
    prepare-library vkd3d-proton HansKristian-Work/vkd3d-proton ".*\.tar\.zst$"

    find "$LIBRARIES" -name "*.sh" -type f -exec chmod +x {} \;
}

function prepare-winetricks {
    mkdir -p "$RUNTIMES/.bin/"
    if ! [ -f "$RUNTIMES/.bin/winetricks" ]; then
        file-get "https://raw.githubusercontent.com/Winetricks/winetricks/master/src/winetricks" "$RUNTIMES/.bin/winetricks"
        chmod +x "$RUNTIMES/.bin/winetricks"
    fi

    if ! [ -f "$RUNTIMES/.bin/cabextract" ]; then
        file-get "https://archlinux.org/packages/community/x86_64/cabextract/download/" "$RUNTIMES/cabextract.tar.zst"
        tar --extract -C "$RUNTIMES/.bin" -f "$RUNTIMES/cabextract.tar.zst" usr/bin/cabextract --strip-components 2
        rm "$RUNTIMES/cabextract.tar.zst"
    fi
}

# For streaming to TV
function sync-to-sunshine {
    local BANNERS="$CACHE/banners"

    mkdir -p "$(dirname $SUNSHINE_CONF)"
    mkdir -p "$BANNERS"

    local CONFIG='{"apps": []}'

    while read game; do
        local game_id="$(get-steamgriddb-id "$game")"

        if [[ "$(cat "$YAML" | "$YQ" ".games[\"$game\"].sunshine")" == "false" ]]; then
            continue
        fi

        if [[ "$(cat "$YAML" | "$YQ" ".games[\"$game\"].steamgriddb")" == "false" ]]; then
            continue
        fi

        local game_data="$(jq --null-input \
            --arg name "$(cat "$YAML" | "$YQ" ".games[\"$game\"].name")" \
            --arg cmd "$(echo $LAUNCHER | sed s/{}/$game/)" \
            --arg image "$BANNERS/$game_id.png" \
            '[{"name": $name, "output": "", "cmd": $cmd, "image-path": $image}]')"

        
        if [ -n "$game_id" ] && [ ! -f "$BANNERS/$game_id.png" ]; then
            local BANNER_URL="$(curl -H "Authorization: Bearer $SGDB_TOKEN" \
                "https://www.steamgriddb.com/api/v2/grids/game/$game_id" \
                | jq -r '([.data[] | select(.width == 600)][0] | .url) // .data[0].url')"

            curl "$BANNER_URL" -o "$BANNERS/$game_id.png.orig"
            convert "$BANNERS/$game_id.png.orig" "$BANNERS/$game_id.png"
            rm "$BANNERS/$game_id.png.orig"
        fi

        CONFIG="$(echo "$CONFIG" | jq ".apps += $game_data")"
    done <<< "$(cat "$YAML" | "$YQ" '.games[] | key')"

    echo $CONFIG | jq >| "$SUNSHINE_CONF"
}

# For easy launching
function create-desktop-files {
    local ICONS="$CACHE/icons"
    rm -f "$DESKTOP_DIR"/*
    mkdir -p "$DESKTOP_DIR"
    mkdir -p "$ICONS"

    while read game; do
        local game_id="$(get-steamgriddb-id "$game")"

        if [[ "$(cat "$YAML" | "$YQ" ".games[\"$game\"].desktop")" == "false" ]]; then
            continue
        fi

        if [ -n "$game_id" ] && [ ! -f "$ICONS/$game_id.png" ]; then
            local ICON_URL="$(curl -H "Authorization: Bearer $SGDB_TOKEN" \
                "https://www.steamgriddb.com/api/v2/icons/game/$game_id" \
                | jq -r '.data[0].thumb')"

            curl "$ICON_URL" -o "$ICONS/$game_id.png"
        fi

        printf "%s\n" \
            "[Desktop Entry]" \
            "Type=Application" \
            "Version=1.0" \
            "Name=$(cat "$YAML" | "$YQ" ".games[\"$game\"].name")" \
            "Path=$(dirname "$(realpath "$0")")" \
            "Exec=$(echo $LAUNCHER | sed s/{}/$game/)" \
            "Icon=$ICONS/$game_id.png" \
            "Terminal=false" \
            "Categories=Games;" > "$DESKTOP_DIR/$game.desktop"

    done <<< "$(cat "$YAML" | "$YQ" '.games[] | key')"
}

# For https://github.com/SteamGridDB/steam-rom-manager
function create-shell-scripts {
    rm -rf "$SHELLS_DIR"
    mkdir -p "$SHELLS_DIR"

    while read line; do
        local name="$(cat "$YAML" | "$YQ" ".games[\"$game\"].name")"

        echo -e "#!/bin/bash\n$(echo $LAUNCHER | sed s/{}/$game/)" > "$SHELLS_DIR/$name.sh"
        chmod +x "$SHELLS_DIR/$name.sh"
    done <<< "$(cat "$YAML" | "$YQ" '.games[] | key')"
}

function run-wine {
    local GAME="$1"
    local RUNTIME=$(cat "$YAML" | "$YQ" '.runtime // "native"')

    if [ "$RUNTIME" != "native" ]; then
        prepare-runtime "$RUNTIME"
        export PATH="$RUNTIMES/$RUNTIME/bin:$PATH"
    fi

    prepare-libaries

    local NAME="$(cat "$YAML" | "$YQ" ".games[\"$GAME\"].name")"
    local PREFIX="$(cat "$YAML" | "$YQ" ".games[\"$GAME\"].prefix // \"$NAME\"")"
    local GAME_EXE="$(cat "$YAML" | "$YQ" ".games[\"$GAME\"].run")"

    eval "$(cat "$YAML" | "$YQ" -o p '.env' | sed -r 's/([^ ]+) = (.*)/export \1="\2"/')" > /dev/null
    eval "$(cat "$YAML" | "$YQ" -o p ".games[\"$GAME\"].env // \"\"" | sed -r 's/([^ ]+) = (.*)/export \1="\2"/')" > /dev/null

    export WINEPREFIX="$PREFIXES/$PREFIX"
    export WINEDLLOVERRIDES="winemenubuilder.exe="

    local GAME_DIR="$(cat "$YAML" | "$YQ" ".games[\"$GAME\"].dir // \"$WINEPREFIX/drive_c\"")"

    # Init prefix
    if [ ! -d "$WINEPREFIX" ]; then
        echo Initializing prefix
        wine __INITPREFIX > /dev/null 2>&1 || true
        wineserver --wait
    fi

    cd "$WINEPREFIX"

    # Replace symlinks to $HOME with directories
    find "$WINEPREFIX/drive_c/users/$USER" -maxdepth 1 -type l \
        -exec unlink {} \; \
        -exec mkdir {} \;

    # Winetricks
    while read line; do
        if [ -z "$line" ]; then continue; fi
        if ! grep -Fxq "$line" "$WINEPREFIX/.winetricks"; then
            winetricks -q $line
            echo "$line" >> "$WINEPREFIX/.winetricks"
        fi
    done <<< "$(cat "$YAML" | $YQ ".games[\"$GAME\"].winetricks[]")"

    # Mounts
    while read line; do
        local from="$(cat "$YAML" | $YQ ".games[\"$GAME\"].mounts[\"$line\"]")"
        ln -Tsf "$from" "$WINEPREFIX/dosdevices/${line}:"
    done <<< "$(cat "$YAML" | $YQ '.games["'"$GAME"'"].mounts[] | key')"

    # Install libraries for games
    local system32="$WINEPREFIX/drive_c/windows/system32"

    while read line; do
        if [ -z "$line" ]; then continue; fi

        local version="$(cat "$YAML" | "$YQ" ".libraries[\"$line\"].version")"

        # FIXME: Ugly hack for dxvk-async 2.1
        local dll="$(find "$LIBRARIES/$line/$version/x64" -name "*.dll" -not -name '*_config.dll' | head -n 1)"
        
        if ! diff -q "$system32/${dll##*/}" "$dll"; then
            echo Installing $line...

            # FIXME: Ugly hack for dxvk-async 2.1
            local command=install
            if echo "$line/$version" | grep -q "^dxvk-async/fork"; then
                command=""
            fi

            find "$LIBRARIES/$line/$version/" -maxdepth 2 -name "*.sh" -type f -exec {} $command \;
        fi
    done <<< "$(cat "$YAML" | $YQ '.libraries[] | key')"

    # Enable nvidia DLSS 2.0, this comes with nvidia-utils
    if [ -f /usr/lib/nvidia/wine/nvngx.dll ]; then
        echo Copying ngngx.dll
        cp /usr/lib/nvidia/wine/nvngx.dll "$WINEPREFIX/drive_c/windows/system32/"
        cp /usr/lib/nvidia/wine/_nvngx.dll "$WINEPREFIX/drive_c/windows/system32/"
    fi

    # Enable CUDA for DLSS 3.0 or PhysX. This is taken from wine
    if [ -f "/usr/lib/wine/x86_64-windows/nvcuda.dll" ]; then
        echo Copying nvcuda
        cp "/usr/lib/wine/x86_64-windows/nvcuda.dll" "$WINEPREFIX/drive_c/windows/system32/nvcuda.dll"
        cp "/usr/lib32/wine/i386-windows/nvcuda.dll" "$WINEPREFIX/drive_c/windows/syswow64/nvcuda.dll"
    fi

    # Before scripts
    while read line; do
        if [ -z "$line" ]; then continue; fi
        $line
    done <<< "$(cat "$YAML" | $YQ ".games[\"$GAME\"].before[]")"

    wineserver --wait

    cd "$GAME_DIR"
    $WRAPPER wine "$GAME_EXE" ${@:2} $(cat "$YAML" | "$YQ" ".games[\"$GAME\"].args[]")
    wineserver --wait

    if [[ "$(cat "$YAML" | $YQ ".games[\"$GAME\"].cleanup")" != "false" ]]; then
        wineserver -k
    fi
}

function run-native {
    local GAME="$1"
    local RUN="$(cat "$YAML" | "$YQ" ".games[\"$GAME\"].run")"
    local GAME_DIR="$(cat "$YAML" | $YQ ".games[\"$GAME\"].dir // \"$HOME\"")"

    cd "$GAME_DIR"
    $WRAPPER "$RUN" $(cat "$YAML" | "$YQ" ".games[\"$GAME\"].args[]")
}

function run {
    local GAME="$1"

    if [ -z "$GAME" ]; then
        echo Provide game key as an argument:
        echo "$(cat "$YAML" | "$YQ" '.games[] | key')"
        exit 1;
    fi

    if [[ "$(cat "$YAML" | "$YQ" ".games[\"$GAME\"]")" == "null" ]]; then
        echo Invalid game "$GAME", provide valid game key as an argument:
        echo "$(cat "$YAML" | "$YQ" '.games[] | key')"
        exit 1;
    fi

    local TYPE="$(cat "$YAML" | "$YQ" ".games[\"$GAME\"].type // \"wine\"")"

    case "$TYPE" in
        native)
            run-native "$1" "${@:2}";;
        "wine")
            run-wine "$1" "${@:2}";;
        *)
            echo "invalid type";;
    esac
}

function refresh {
    if ! sha256sum --quiet --check "$DATA_DIR/config.sha256" 2>/dev/null; then
        echo "Checksum changed!"
        if [[ "$(cat "$YAML" | "$YQ" '.generate.sunshine')" == "true" ]]; then
            sync-to-sunshine
        fi
        if [[ "$(cat "$YAML" | "$YQ" '.generate.desktop')" == "true" ]]; then
            create-desktop-files
        fi
        if [[ "$(cat "$YAML" | "$YQ" '.generate.shell')" == "true" ]]; then
            create-shell-scripts
        fi
        sha256sum "$YAML" >| "$DATA_DIR/config.sha256"
    fi
}

refresh

if [ "$1" == "watch" ]; then
    while inotifywait -e modify "$YAML"; do
        echo "Refreshing generated files"
        refresh;
    done
else
    run "$1" "${@:2}"
fi
