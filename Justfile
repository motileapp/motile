set shell := ["bash", "-euo", "pipefail", "-c"]

app_id := "app.motile.motile"
prefix := env("PREFIX", if `id -u` == "0" { "/usr/local" } else { home_directory() / ".local" })
client := prefix / "bin/motile-gpui"
desktop_entry := prefix / "share/applications" / (app_id + ".desktop")
icon := prefix / "share/icons/hicolor/512x512/apps" / (app_id + ".png")
built_client := env("CARGO_TARGET_DIR", justfile_directory() / "apps/gpui/target") / "release/motile-gpui"
server := "/usr/local/bin/motile"
built_server := env("CARGO_TARGET_DIR", justfile_directory() / "target") / "release/motile"

default:
    @just --list --unsorted

# build the client, which leaves its desktop entry to install
build:
    apps/gpui/scripts/patch-crates.sh
    MOTILE_PACKAGED=1 cargo build --release --manifest-path apps/gpui/Cargo.toml

# install the built client with its icon and desktop entry, in ~/.local, or /usr/local with sudo
install:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ ! -x "{{ built_client }}" ]; then
        echo "nothing to install yet, run just build first" >&2
        exit 1
    fi
    install -Dm755 "{{ built_client }}" "{{ client }}.new"
    mv "{{ client }}.new" "{{ client }}"
    install -Dm644 apps/gpui/assets/icon.png "{{ icon }}"
    mkdir -p "$(dirname "{{ desktop_entry }}")"
    cat > "{{ desktop_entry }}" <<EOF
    [Desktop Entry]
    Type=Application
    Name=Motile
    Comment=The command center for coding agents
    Exec="{{ client }}" %u
    Icon={{ app_id }}
    Terminal=false
    Categories=Development;
    MimeType=x-scheme-handler/motile;
    StartupWMClass={{ app_id }}
    EOF
    if command -v update-desktop-database >/dev/null; then update-desktop-database -q "$(dirname "{{ desktop_entry }}")"; fi
    if [ "$(id -u)" != 0 ] && command -v xdg-mime >/dev/null; then xdg-mime default "{{ app_id }}.desktop" x-scheme-handler/motile; fi
    echo "installed {{ client }}"

# build the client and start it here, with its log in this terminal
run *args: _client-stopped build
    exec "{{ built_client }}" {{ args }}

# remove the client, its icon and its desktop entry; its data stays in ~/.local/share/motile-gpui
uninstall:
    rm -f "{{ client }}" "{{ desktop_entry }}" "{{ icon }}"
    if command -v update-desktop-database >/dev/null; then update-desktop-database -q "$(dirname "{{ desktop_entry }}")"; fi

# build the server
server-build:
    cargo build --release -p motile-server

# link this machine as a server with the code the client shows, and start motile.service
server-setup *args: server-build
    "{{ built_server }}" setup {{ args }}

# replace the installed server with this build and restart it, which cuts off running turns
server-update: server-build
    sudo install -m755 "{{ built_server }}" "$(dirname "{{ server }}")/.motile.new"
    sudo mv "$(dirname "{{ server }}")/.motile.new" "{{ server }}"
    sudo systemctl restart motile

_client-stopped:
    @if pgrep -x motile-gpui >/dev/null; then echo "a client is running; quit it first, or this launch only brings it to the front" >&2; exit 1; fi
