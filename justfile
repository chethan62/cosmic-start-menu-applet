name := 'cosmic-start-menu-applet'
appid := 'io.github.jjnuthuagen.StartMenu'

# User-local install by default. For system-wide: `just prefix=/usr install`.
prefix := env_var('HOME') / '.local'

bin-src := 'target' / 'release' / name
bin-dst := prefix / 'bin' / name
desktop := appid + '.desktop'
desktop-src := 'data' / desktop
desktop-dst := prefix / 'share' / 'applications' / desktop

# The Settings window is a second toplevel with its own app id, and COSMIC finds
# a window's icon by matching that id to a desktop file.
settings-desktop := appid + 'Settings.desktop'
settings-desktop-src := 'data' / settings-desktop
settings-desktop-dst := prefix / 'share' / 'applications' / settings-desktop

_default:
    @just --list

build:
    cargo build --release

check:
    cargo clippy --all-targets --locked -- -D warnings

fmt:
    cargo fmt --all

test:
    cargo test

# Everything CI runs.
verify: fmt check test

install: build
    install -Dm0755 {{bin-src}} {{bin-dst}}
    install -Dm0644 {{desktop-src}} {{desktop-dst}}
    if [ -f {{settings-desktop-src}} ]; then install -Dm0644 {{settings-desktop-src}} {{settings-desktop-dst}}; fi
    @echo "Installed. Add it in Settings -> Desktop -> Panel -> Configure applets."

uninstall:
    rm -f {{bin-dst}} {{desktop-dst}} {{settings-desktop-dst}}

# Applets expect to be launched by cosmic-panel, so this mainly catches
# startup panics rather than showing a usable window.
run:
    RUST_LOG=debug cargo run
