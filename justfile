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
    @just restart
    @echo "Installed. If it is not in the panel yet, add it in Settings -> Desktop -> Panel -> Configure applets."

# Reload the panel button so a fresh install is the one running.
#
# cosmic-panel execs each applet once, at session start, and never again: it
# does not respawn one that exits, and installing over the binary leaves the
# old copy live for the rest of the session. A whole session's worth of
# "the button does nothing" was that — the fix was built and installed, and
# the panel was still running the build from before it.
#
# Taking the applet out of `plugins_wings` and putting it back is the
# supported restart: cosmic-panel reacts to the config change. Killing
# cosmic-panel itself is not — it burns cosmic-session's restart budget and
# can leave the panel gone for the session.
restart:
    #!/usr/bin/env bash
    set -euo pipefail
    # The menu is its own long-lived process now, and it owns the bus name:
    # the panel applet's pre-warm finds the name taken and exits, so the
    # button would go on poking the build from before this install. Cycling
    # `plugins_wings` only ever restarts the applet, one level above it.
    pkill -f 'cosmic-start-menu-applet --(toggle|prewarm)' || true
    wings="$HOME/.config/cosmic/com.system76.CosmicPanel.Panel/v1/plugins_wings"
    if ! grep -q '{{appid}}' "$wings" 2>/dev/null; then
        echo "Not in the panel; nothing to restart."
        exit 0
    fi
    saved=$(mktemp)
    cp "$wings" "$saved"
    # Restore the list however this exits, so a failure here can never leave
    # the applet missing from the panel.
    trap 'cp "$saved" "$wings"; rm -f "$saved"' EXIT
    grep -v '{{appid}}' "$saved" > "$wings"
    sleep 1
    echo "Panel button restarted."

uninstall:
    rm -f {{bin-dst}} {{desktop-dst}} {{settings-desktop-dst}}

# Applets expect to be launched by cosmic-panel, so this mainly catches
# startup panics rather than showing a usable window.
run:
    RUST_LOG=debug cargo run
