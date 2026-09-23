# Start Menu applet Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A COSMIC panel applet whose popup is a Windows 10–style Start menu (power/user rail, A–Z app list, pinned tile groups, type-to-search) styled entirely from COSMIC's Appearance settings, good enough to replace the App Library panel button.

**Architecture:** One Rust binary, `cosmic-start-menu-applet`, built on libcosmic's `applet` entry point exactly like the sibling Control Center applet (CCCA-40, `~/1_Projects/cosmic-control-center-applet`). Pure logic lives in small, unit-tested modules (`apps`, `search`, `config`, `usage`, `tile_layout`). A thin `session`/`launch` layer does I/O. The `ui` module draws the popup from those modules' output. The same binary run with `--settings` opens an ordinary Settings window.

**Tech Stack:** Rust (edition 2021, MSRV 1.85), libcosmic pinned at rev `ef490df50b0a05a21c494c3f75737581bf0b39d9`, `freedesktop-desktop-entry` 0.8 (via libcosmic's `desktop` feature), zbus 5, tokio, serde + toml, Fluent.

**Spec:** `docs/superpowers/specs/2026-09-23-start-menu-applet-design.md`. §10 holds the amendments from the mockup and wins over earlier sections.

**Mockup:** `docs/mockups/start-menu-mockup.html` is the clickable prototype James approved. When a layout detail is unclear, open it in a browser and match it.

**Execution order:** Tasks 1–14, then 16–19, then 15.

**Reference code (copy and adapt, never link):** `~/1_Projects/cosmic-control-center-applet`. That repo is MIT/Apache, the same licence as this one, so copying is fine. Files named below as "CCCA `src/…`" are in that repo.

## Global Constraints

- App id: `io.github.jjnuthuagen.StartMenu`; settings window app id `io.github.jjnuthuagen.StartMenuSettings`.
- Binary / crate name: `cosmic-start-menu-applet`. Config: `~/.config/cosmic-start-menu-applet/config.toml`. Launch counts: `~/.local/state/cosmic-start-menu-applet/usage.toml`.
- libcosmic pinned to rev `ef490df50b0a05a21c494c3f75737581bf0b39d9`. Never unpinned.
- No hard-coded colours, radii, spacing or fonts. Everything comes from `cosmic::Theme` (spacing tokens, `corner_radii`, `background(theme.transparent)` component colours, accent).
- No hard-coded user-facing strings. Every string goes through `fl!` and `i18n/en/main.ftl`.
- Never block `update`/`view`. File parsing and D-Bus calls run in `Task::perform` / `spawn_blocking`.
- No panics on the user's data: broken `.desktop` files, a corrupt config, or a missing favourites file must all degrade gracefully.
- Public repo: every commit author is `jjnuthuagen <246846236+jjnuthuagen@users.noreply.github.com>` (already set in this repo's `.git/config`). The real name and gmail address must never appear in commits or files.
- Commit messages are written for strangers (a public repo). End every commit with the two trailer lines:
  ```
  Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>
  Claude-Session: https://claude.ai/code/session_01MVQf1P64Nd9VkJ8r4ic3ZP
  ```
- Never `git push`.
- Pins are stored by desktop-entry **app id without `.desktop`** (e.g. `firefox`, `com.system76.CosmicTerm`), the same form as the dock's favourites list.

**Spec refinements made while planning (all within the spec's intent):**
- "Watches app dirs" is implemented as **re-index on every popup open**, done off-thread. Parsing ~300 files takes a few ms, and this avoids a file-watcher dependency. The effect is the same: newly installed apps appear the next time the menu opens.
- Log out / Restart / Shut down call **`cosmic-osd log-out|restart|shutdown`**. That is COSMIC's own confirmation-with-countdown dialog, which is what the stock power applet shows. Lock and Suspend go through logind over D-Bus.
- The config example in spec §6 uses `app = "firefox.desktop"`. The real form is `app = "firefox"` (see Global Constraints).

## Review Focus

1. **The same app installed twice** (system package + Flatpak, or a user override in `~/.local/share/applications`) → it appears once, and the user's own override wins. *Test in Task 2.*
2. **Names starting with a digit, a lowercase letter, or a non-Latin letter** (`0 A.D.`, `btop`, `Яндекс`) → they are grouped under `#`, `B` and `Я` respectively, and `#` sorts first. *Test in Task 2.*
3. **A search query that is all whitespace, or in different case** (`"  "`, `FIREfox`) → whitespace counts as no search; matching ignores case. *Test in Task 3.*
4. **A pinned app is uninstalled and later reinstalled** → the tile is hidden while the app is gone and comes back unchanged. The config entry is never dropped. *Test in Task 4 (`visible_tiles`).*
5. **First run with no dock favourites file, or favourites that point at apps that aren't installed** → the seeded group contains only installed apps, taken from the fallback list. *Test in Task 4.*

---

## File Structure

```
Cargo.toml                 crate + pinned deps
justfile                   build / test / install recipes (from CCCA)
.github/workflows/build.yml  CI (from CCCA)
LICENSE-MIT, LICENSE-APACHE  copied from CCCA, same holder line
README.md
data/io.github.jjnuthuagen.StartMenu.desktop
data/io.github.jjnuthuagen.StartMenuSettings.desktop
i18n/en/main.ftl           every user-facing string
src/main.rs                arg dispatch: applet / --settings
src/i18n.rs                fl! macro (copied from CCCA)
src/apps.rs                load installed apps, A–Z bucketing          [pure + fs]
src/search.rs              rank apps against a query                   [pure]
src/config.rs              groups/tiles/finish; load/save/seed; pin ops [pure + fs]
src/usage.rs               launch counts → "Most used"                 [pure + fs]
src/tile_layout.rs         pack Small/Medium/Wide tiles into a 6-col grid [pure]
src/session.rs             lock/suspend (logind), log out/restart/shutdown (cosmic-osd)
src/launch.rs              start an app or desktop action; record usage
src/process.rs             spawn_and_reap (copied from CCCA src/process.rs)
src/ui/mod.rs              theme helpers: Spacing, radius, finish paint
src/ui/rail.rs             left rail + power menu
src/ui/app_list.rs         Most used + A–Z list + letter-jump grid
src/ui/tiles.rs            tile groups drawn from tile_layout
src/app.rs                 cosmic::Application: state, messages, popup
src/settings.rs            --settings window
```

---

### Task 1: Crate skeleton, a panel button, and an empty frosted popup

**Files:**
- Create: `Cargo.toml`, `justfile`, `.github/workflows/build.yml`, `LICENSE-MIT`, `LICENSE-APACHE`, `README.md`, `data/io.github.jjnuthuagen.StartMenu.desktop`, `i18n/en/main.ftl`, `src/main.rs`, `src/i18n.rs`, `src/app.rs`
- Modify: `.gitignore` (add `/target`)
- Delete: the scaffold's empty `src/.gitkeep` if present

**Interfaces:**
- Produces: `crate::fl!("key")` / `fl!("key", name = value)` → `String`; `app::App` implementing `cosmic::Application` with `Message::{TogglePopup, PopupClosed(Id)}`; `app::POPUP_WIDTH: f32 = 680.0`, `app::POPUP_HEIGHT: f32 = 600.0`.

- [ ] **Step 1: Write `Cargo.toml`**

```toml
[package]
name = "cosmic-start-menu-applet"
version = "0.1.0"
edition = "2021"
description = "A Windows 10-style Start menu for the COSMIC panel"
license = "MIT OR Apache-2.0"
repository = "https://github.com/jjnuthuagen/cosmic-start-menu-applet"
readme = "README.md"
rust-version = "1.85"

[dependencies]
# Same pin as cosmic-control-center-applet: libcosmic is pre-1.0 and moves.
libcosmic = { git = "https://github.com/pop-os/libcosmic", rev = "ef490df50b0a05a21c494c3f75737581bf0b39d9", default-features = false, features = [
    "applet",
    "tokio",
    "wayland",
    "multi-window",
    "winit",
    # Desktop-entry parsing and launching, in a systemd scope like the stock launcher.
    "desktop",
    "desktop-systemd-scope",
] }
zbus = { version = "5", default-features = false, features = ["tokio"] }
tokio = { version = "1", features = ["time", "process", "rt", "fs"] }
serde = { version = "1", features = ["derive"] }
toml = "0.9"
dirs = "6"
fluent-bundle = "0.16"
unic-langid = "0.9"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

[dev-dependencies]
tempfile = "3"

[profile.release]
lto = "thin"
opt-level = 3
strip = true
codegen-units = 1
```

- [ ] **Step 2: Copy the boilerplate from CCCA**

```bash
cd ~/1_Projects/CSTM-49-cosmic-start-menu-applet
C=~/1_Projects/cosmic-control-center-applet
cp $C/LICENSE-MIT $C/LICENSE-APACHE .
mkdir -p .github/workflows src i18n/en data
cp $C/.github/workflows/build.yml .github/workflows/build.yml
cp $C/src/i18n.rs src/i18n.rs
cp $C/src/process.rs src/process.rs
grep -n 'Nuthall\|gmail' LICENSE-MIT LICENSE-APACHE || echo "licences clean"
```

Expected: `licences clean`. If a real name shows up, replace it with `jjnuthuagen`.
Then remove any Control-Center-specific lines from `src/process.rs`: keep `in_flatpak`, `host_command` and `spawn_and_reap` exactly as they are.

- [ ] **Step 3: Write `i18n/en/main.ftl` with the first keys**

```ftl
app-name = Start Menu
search-placeholder = Type to search
most-used = Most used
no-results = No apps match “{ $query }”
```

- [ ] **Step 4: Write the failing i18n test**

`src/i18n.rs` already carries CCCA's tests (the embedded file parses, a missing key falls back to the key). Add one:

```rust
#[test]
fn every_start_menu_key_resolves() {
    for key in ["app-name", "search-placeholder", "most-used"] {
        assert_ne!(crate::fl!(key), key, "{key} is missing from main.ftl");
    }
}
```

- [ ] **Step 5: Write `src/main.rs`**

```rust
//! Start Menu — a Windows 10-style Start menu for the COSMIC panel.

mod app;
mod i18n;
mod process;

fn main() -> cosmic::iced::Result {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();
    cosmic::applet::run::<app::App>(())
}
```

- [ ] **Step 6: Write `src/app.rs`, with a button that opens an empty, blurred popup**

Adapted from CCCA `src/app.rs` lines 1545–1650 (the popup and blur code). Keep the comment that explains why blur has to go through the Wayland command.

```rust
use cosmic::app::{Core, Task};
use cosmic::iced::window::{self, Id};
use cosmic::iced::{Length, Limits};
use cosmic::widget::{container, text};
use cosmic::{Application, Element};

use crate::fl;

/// Rail + list + a six-cell tile column, with padding. Fixed like Windows 10's
/// menu; each column scrolls inside it.
pub const POPUP_WIDTH: f32 = 680.0;
pub const POPUP_HEIGHT: f32 = 600.0;

pub struct App {
    core: Core,
    popup: Option<Id>,
}

#[derive(Debug, Clone)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
}

impl Application for App {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = "io.github.jjnuthuagen.StartMenu";

    fn core(&self) -> &Core { &self.core }
    fn core_mut(&mut self) -> &mut Core { &mut self.core }

    fn init(core: Core, _flags: ()) -> (Self, Task<Message>) {
        (Self { core, popup: None }, Task::none())
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::TogglePopup => {
                if let Some(id) = self.popup.take() {
                    return cosmic::iced::platform_specific::shell::commands::popup::destroy_popup(id);
                }
                let id = window::Id::unique();
                self.popup = Some(id);
                let mut settings = self.core.applet.get_popup_settings(
                    self.core.main_window_id().unwrap_or(id), id, None, None, None,
                );
                settings.positioner.size_limits = Limits::NONE
                    .min_width(POPUP_WIDTH).max_width(POPUP_WIDTH)
                    .min_height(POPUP_HEIGHT).max_height(POPUP_HEIGHT);
                let popup = cosmic::iced::platform_specific::shell::commands::popup::get_popup(settings);
                // libcosmic only blurs surfaces it tracks, and a `get_popup`
                // surface is not one of them, so ask for the blur ourselves —
                // via the Wayland command, which parks the request until the
                // surface exists (see CCCA src/app.rs for the full story).
                if self.core.frosted(self.core.system_theme().cosmic()) {
                    let blur = cosmic::iced::platform_specific::shell::commands::blur::blur(
                        id,
                        Some(vec![cosmic::iced::Rectangle { x: 0.0, y: 0.0, width: f32::MAX, height: f32::MAX }]),
                    );
                    Task::batch([popup, blur.discard()])
                } else {
                    popup
                }
            }
            Message::PopupClosed(id) => {
                if self.popup == Some(id) { self.popup = None; }
                Task::none()
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        self.core
            .applet
            .icon_button("start-here-symbolic")
            .on_press(Message::TogglePopup)
            .into()
    }

    fn view_window(&self, _id: Id) -> Element<'_, Message> {
        let body = container(text(fl!("app-name")))
            .width(Length::Fixed(POPUP_WIDTH))
            .height(Length::Fixed(POPUP_HEIGHT));
        self.core.applet.popup_container(body).into()
    }
}
```

- [ ] **Step 7: Write the desktop entry, justfile and README stub**

`data/io.github.jjnuthuagen.StartMenu.desktop`:
```ini
[Desktop Entry]
Name=Start Menu
Comment=A Windows 10-style Start menu for the panel
Type=Application
Exec=cosmic-start-menu-applet
Terminal=false
Categories=COSMIC;
Keywords=COSMIC;Iced;start;menu;launcher;
Icon=start-here-symbolic
StartupNotify=true
NoDisplay=true
X-CosmicApplet=true
X-CosmicHoverPopup=Auto
X-OverflowPriority=10
```

`justfile`: copy CCCA's `justfile`, set `name := 'cosmic-start-menu-applet'` and `appid := 'io.github.jjnuthuagen.StartMenu'`, and delete the `metainfo`, `icons`, `check-system` and `toggle-tiling` lines. `settings-desktop` stays; Task 14 creates that file.

`README.md`: title, one-line description, a "Build" block containing `just install`, and a "Status: alpha" line.

- [ ] **Step 8: Build, test, and check it on the panel**

```bash
cargo test && just check && just install
```
Expected: tests PASS, clippy clean, binary installed to `~/.local/bin`.
Manual: Settings → Desktop → Panel → Configure applets → add **Start Menu**. Clicking it opens a 680×600 popup that says "Start Menu". With frosted styling on, the wallpaper is blurred behind it.

- [ ] **Step 9: Commit**

```bash
git add -A && git commit -m "Start the Start Menu applet: a panel button and an empty frosted popup

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01MVQf1P64Nd9VkJ8r4ic3ZP"
```

---

### Task 2: `apps` — load installed apps and group them A–Z

**Files:**
- Create: `src/apps.rs`
- Modify: `src/main.rs` (add `mod apps;`)

**Interfaces:**
- Produces:
  ```rust
  pub struct App { pub id: String, pub name: String, pub generic_name: Option<String>,
                   pub keywords: Vec<String>, pub icon: IconSource, pub exec: Option<String>,
                   pub terminal: bool, pub actions: Vec<Action> }
  pub struct Action { pub name: String, pub exec: String }
  pub use cosmic::desktop::fde::IconSource;
  pub fn locales() -> Vec<String>
  pub fn load_from(dirs: Vec<PathBuf>, locales: &[String], desktop: Option<&str>) -> Vec<App> // sorted A–Z, deduped by id, first dir wins
  pub fn load_all() -> Vec<App>            // default XDG dirs, $XDG_CURRENT_DESKTOP
  pub fn letter(name: &str) -> char        // '#' for digits/symbols, else uppercase first letter
  pub fn sections(apps: &[App]) -> Vec<(char, Vec<usize>)> // indices into `apps`, '#' first
  ```

- [ ] **Step 1: Write the failing tests** (bottom of `src/apps.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn entry(dir: &std::path::Path, id: &str, body: &str) {
        fs::write(dir.join(format!("{id}.desktop")),
            format!("[Desktop Entry]\nType=Application\n{body}\n")).unwrap();
    }

    #[test]
    fn loads_sorted_skips_hidden_and_nodisplay() {
        let d = tempfile::tempdir().unwrap();
        entry(d.path(), "zed", "Name=Zed\nExec=zed %U");
        entry(d.path(), "btop", "Name=btop\nExec=btop\nTerminal=true");
        entry(d.path(), "ghost", "Name=Ghost\nExec=g\nNoDisplay=true");
        entry(d.path(), "gone", "Name=Gone\nExec=g\nHidden=true");
        let apps = load_from(vec![d.path().into()], &[], None);
        let names: Vec<_> = apps.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["btop", "Zed"]);
        assert!(apps[0].terminal);
    }

    #[test]
    fn first_directory_wins_on_duplicate_id() {
        let user = tempfile::tempdir().unwrap();
        let system = tempfile::tempdir().unwrap();
        entry(user.path(), "firefox", "Name=Firefox (mine)\nExec=firefox");
        entry(system.path(), "firefox", "Name=Firefox\nExec=firefox");
        let apps = load_from(vec![user.path().into(), system.path().into()], &[], None);
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].name, "Firefox (mine)");
    }

    #[test]
    fn respects_only_show_in() {
        let d = tempfile::tempdir().unwrap();
        entry(d.path(), "kde", "Name=KDE thing\nExec=k\nOnlyShowIn=KDE;");
        assert!(load_from(vec![d.path().into()], &[], Some("COSMIC")).is_empty());
    }

    #[test]
    fn reads_keywords_generic_name_and_actions() {
        let d = tempfile::tempdir().unwrap();
        entry(d.path(), "ff", "Name=Firefox\nGenericName=Web Browser\nKeywords=internet;www;\nExec=firefox\nActions=private;\n\n[Desktop Action private]\nName=New Private Window\nExec=firefox --private-window");
        let a = &load_from(vec![d.path().into()], &[], None)[0];
        assert_eq!(a.generic_name.as_deref(), Some("Web Browser"));
        assert_eq!(a.keywords, ["internet", "www"]);
        assert_eq!(a.actions[0].name, "New Private Window");
    }

    #[test]
    fn a_broken_file_is_skipped_not_fatal() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("bad.desktop"), "\u{0}\u{1}not an ini").unwrap();
        entry(d.path(), "ok", "Name=Ok\nExec=ok");
        assert_eq!(load_from(vec![d.path().into()], &[], None).len(), 1);
    }

    #[test]
    fn letters_for_digits_lowercase_and_non_latin() {
        assert_eq!(letter("0 A.D."), '#');
        assert_eq!(letter("btop"), 'B');
        assert_eq!(letter("Яндекс"), 'Я');
        assert_eq!(letter("  "), '#');
    }

    #[test]
    fn sections_put_hash_first_then_alphabetical() {
        let mk = |n: &str| App { name: n.into(), ..App::default() };
        let apps = vec![mk("0 A.D."), mk("Alacritty"), mk("audacity"), mk("Bitwarden")];
        let s = sections(&apps);
        let letters: Vec<char> = s.iter().map(|(c, _)| *c).collect();
        assert_eq!(letters, ['#', 'A', 'B']);
        assert_eq!(s[1].1, [1, 2]);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test apps::`
Expected: compile errors (`load_from`, `letter`, `sections` not found).

- [ ] **Step 3: Implement**

```rust
//! Installed applications, read from the XDG desktop-entry directories.
//!
//! Loaded fresh every time the popup opens (off the UI thread), so an app
//! installed a moment ago is there on the next open — no file watcher needed.

use std::collections::HashSet;
use std::path::PathBuf;

use cosmic::desktop::fde;
pub use fde::IconSource;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct App {
    pub id: String,
    pub name: String,
    pub generic_name: Option<String>,
    pub keywords: Vec<String>,
    pub icon: IconSource,
    pub exec: Option<String>,
    pub terminal: bool,
    pub actions: Vec<Action>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Action {
    pub name: String,
    pub exec: String,
}

pub fn locales() -> Vec<String> {
    fde::get_languages_from_env()
}

/// Every app COSMIC's own launcher would show, from the standard directories.
pub fn load_all() -> Vec<App> {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").ok();
    load_from(fde::default_paths().collect(), &locales(), desktop.as_deref())
}

/// Load from `dirs` in precedence order: an id seen in an earlier directory
/// shadows the same id later, which is how a user override in
/// `~/.local/share/applications` beats the system copy.
pub fn load_from(dirs: Vec<PathBuf>, locales: &[String], desktop: Option<&str>) -> Vec<App> {
    let mut seen = HashSet::new();
    let mut apps: Vec<App> = fde::Iter::new(dirs.into_iter())
        .filter_map(|path| match fde::DesktopEntry::from_path(path.clone(), Some(locales)) {
            Ok(de) => Some(de),
            Err(err) => {
                tracing::debug!("skipping {}: {err}", path.display());
                None
            }
        })
        .filter(|de| seen.insert(de.id().to_owned()))
        .filter(|de| de.type_().is_none_or(|t| t == "Application"))
        .filter(|de| !de.hidden() && !de.no_display())
        .filter(|de| match (desktop, de.only_show_in()) {
            (Some(current), Some(only)) => only.iter().any(|d| current.split(':').any(|c| c == *d)),
            (None, Some(_)) => false,
            _ => true,
        })
        .filter(|de| match (desktop, de.not_show_in()) {
            (Some(current), Some(not)) => !not.iter().any(|d| current.split(':').any(|c| c == *d)),
            _ => true,
        })
        .map(|de| from_entry(&de, locales))
        .collect();
    apps.sort_by_cached_key(|a| (a.name.to_lowercase(), a.id.clone()));
    apps
}

fn from_entry(de: &fde::DesktopEntry, locales: &[String]) -> App {
    let actions = de
        .actions()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|action| {
            Some(Action {
                name: de.action_entry_localized(action, "Name", locales)?.to_string(),
                exec: de.action_entry(action, "Exec")?.to_string(),
            })
        })
        .collect();
    App {
        id: de.id().to_owned(),
        name: de.name(locales).map(|n| n.to_string()).unwrap_or_else(|| de.id().to_owned()),
        generic_name: de.generic_name(locales).map(|n| n.to_string()),
        keywords: de.keywords(locales).unwrap_or_default().into_iter().map(|k| k.to_string()).collect(),
        icon: IconSource::from_unknown(de.icon().unwrap_or(de.id())),
        exec: de.exec().map(str::to_owned),
        terminal: de.terminal(),
        actions,
    }
}

/// The A–Z header an app sorts under. Digits, symbols and blank names share
/// `#`, which Windows puts first; any other script keeps its own capital.
pub fn letter(name: &str) -> char {
    match name.trim().chars().next() {
        Some(c) if c.is_alphabetic() => c.to_uppercase().next().unwrap_or(c),
        _ => '#',
    }
}

/// Group `apps` (already sorted) under their letters, `#` first.
pub fn sections(apps: &[App]) -> Vec<(char, Vec<usize>)> {
    let mut out: Vec<(char, Vec<usize>)> = Vec::new();
    for (i, app) in apps.iter().enumerate() {
        let l = letter(&app.name);
        match out.iter_mut().find(|(c, _)| *c == l) {
            Some((_, v)) => v.push(i),
            None => out.push((l, vec![i])),
        }
    }
    out.sort_by_key(|(c, _)| (*c != '#', *c));
    out
}
```

`IconSource` has no `Default`. If `#[derive(Default)]` on `App` fails to compile, write a manual `impl Default for App` that uses `IconSource::Name(String::new())`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test apps::`
Expected: 7 passed.

- [ ] **Step 5: Commit**

```bash
git add src/apps.rs src/main.rs && git commit -m "Read installed apps and group them A to Z

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01MVQf1P64Nd9VkJ8r4ic3ZP"
```

---

### Task 3: `search` — rank apps against a typed query

**Files:**
- Create: `src/search.rs`
- Modify: `src/main.rs` (`mod search;`)

**Interfaces:**
- Consumes: `apps::App`
- Produces: `pub fn rank(apps: &[App], query: &str) -> Vec<usize>` returns indices into `apps`, best first, and an empty vec when the query is blank.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::apps::App;

    fn app(name: &str, generic: Option<&str>, kw: &[&str]) -> App {
        App { name: name.into(), generic_name: generic.map(Into::into),
              keywords: kw.iter().map(|s| s.to_string()).collect(), ..App::default() }
    }

    fn names(apps: &[App], q: &str) -> Vec<String> {
        rank(apps, q).into_iter().map(|i| apps[i].name.clone()).collect()
    }

    #[test]
    fn prefix_beats_word_start_beats_substring_beats_keyword() {
        let apps = vec![
            app("Terminal", None, &[]),             // substring "ter"? no: prefix
            app("GNOME Terminal", None, &[]),       // word start
            app("Sterling", None, &[]),             // substring
            app("Console", None, &["terminal"]),    // keyword
        ];
        assert_eq!(names(&apps, "ter"), ["Terminal", "GNOME Terminal", "Sterling", "Console"]);
    }

    #[test]
    fn case_is_ignored() {
        let apps = vec![app("Firefox", None, &[])];
        assert_eq!(names(&apps, "FIREfox"), ["Firefox"]);
    }

    #[test]
    fn blank_query_matches_nothing() {
        let apps = vec![app("Firefox", None, &[])];
        assert!(rank(&apps, "   ").is_empty());
        assert!(rank(&apps, "").is_empty());
    }

    #[test]
    fn generic_name_matches_like_a_keyword() {
        let apps = vec![app("Firefox", Some("Web Browser"), &[])];
        assert_eq!(names(&apps, "browser"), ["Firefox"]);
    }

    #[test]
    fn ties_keep_alphabetical_order() {
        let apps = vec![app("Files", None, &[]), app("Firefox", None, &[])];
        assert_eq!(names(&apps, "fi"), ["Files", "Firefox"]);
    }

    #[test]
    fn no_match_is_empty() {
        assert!(rank(&[app("Files", None, &[])], "zzz").is_empty());
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test search::` — Expected: compile error, `rank` not found.

- [ ] **Step 3: Implement**

```rust
//! Type-to-search ranking. Pure: the same query over the same apps always
//! gives the same order, which is what makes Enter-launches-top-hit safe.

use crate::apps::App;

/// Lower is better. `None` means no match.
fn score(app: &App, q: &str) -> Option<u8> {
    let name = app.name.to_lowercase();
    if name.starts_with(q) {
        return Some(0);
    }
    if name.split(|c: char| !c.is_alphanumeric()).any(|w| w.starts_with(q)) {
        return Some(1);
    }
    if name.contains(q) {
        return Some(2);
    }
    let extra = app.generic_name.iter().chain(app.keywords.iter());
    extra
        .map(|s| s.to_lowercase())
        .any(|s| s.contains(q))
        .then_some(3)
}

pub fn rank(apps: &[App], query: &str) -> Vec<usize> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    let mut hits: Vec<(u8, usize)> = apps
        .iter()
        .enumerate()
        .filter_map(|(i, a)| score(a, &q).map(|s| (s, i)))
        .collect();
    // `apps` is already A–Z, so a stable sort on score keeps ties alphabetical.
    hits.sort_by_key(|(s, _)| *s);
    hits.into_iter().map(|(_, i)| i).collect()
}
```

- [ ] **Step 4: Run to verify it passes** — `cargo test search::` → 6 passed.

- [ ] **Step 5: Commit** — `git add src/search.rs src/main.rs && git commit -m "Rank apps for type-to-search" …` (with the trailer lines).

---

### Task 4: `config` — tile groups, finish, load/save/seed and pin operations

**Files:**
- Create: `src/config.rs`
- Modify: `src/main.rs` (`mod config;`), `i18n/en/main.ftl` (add `default-group = Pinned`)

**Interfaces:**
- Produces:
  ```rust
  #[serde(rename_all="lowercase")] pub enum TileSize { Small, Medium, Wide }          // default Medium
  #[serde(rename_all="lowercase")] pub enum TileFinish { Frosted, Solid, Outline }    // default Frosted
  pub struct Tile { pub app: String, pub size: TileSize, pub source: Option<String> } // `source`: v2 live-tile hook, ignored in v1
  pub struct Group { pub name: String, pub tiles: Vec<Tile> }
  pub struct Config { pub finish: TileFinish, pub show_most_used: bool, pub groups: Vec<Group> }
  pub type TileRef = (usize, usize); // (group index, tile index)
  impl Config {
      pub fn path() -> Option<PathBuf>;
      pub fn load() -> Config;                       // missing → seeded; corrupt → .bak + seeded
      pub fn load_from(path: &Path, installed: &[String], favorites: Option<&str>) -> Config;
      pub fn save(&self) -> Result<(), String>;
      pub fn save_to(&self, path: &Path) -> Result<(), String>;
      pub fn seeded(installed: &[String], favorites: Option<&str>) -> Config;
      pub fn is_pinned(&self, app: &str) -> bool;
      pub fn pin(&mut self, app: &str);              // appends Medium to group 0 (creating it if needed)
      pub fn unpin(&mut self, app: &str);            // removes every tile for `app`
      pub fn resize(&mut self, at: TileRef, size: TileSize);
      pub fn move_tile(&mut self, from: TileRef, to_group: usize, to_index: usize);
      pub fn add_group(&mut self, name: String) -> usize;
      pub fn rename_group(&mut self, group: usize, name: String);
      pub fn remove_group(&mut self, group: usize);  // its tiles move to the previous group (or next, if first)
      pub fn visible_tiles<'a>(&'a self, group: usize, installed: &HashSet<&str>) -> Vec<(usize, &'a Tile)>;
  }
  pub fn parse_favorites(ron: &str) -> Vec<String>;
  pub const FALLBACK_PINS: &[&str] = &["firefox", "chromium", "com.system76.CosmicFiles", "com.system76.CosmicTerm", "com.system76.CosmicSettings"];
  ```

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    const FAVS: &str = "[\n    \"com.anthropic.Claude\",\n    \"chromium\",\n    \"com.system76.CosmicTerm\",\n]";

    fn installed(ids: &[&str]) -> Vec<String> { ids.iter().map(|s| s.to_string()).collect() }

    #[test]
    fn parses_the_dock_favorites_ron_list() {
        assert_eq!(parse_favorites(FAVS), ["com.anthropic.Claude", "chromium", "com.system76.CosmicTerm"]);
        assert!(parse_favorites("garbage").is_empty());
    }

    #[test]
    fn seeds_from_favorites_keeping_only_installed() {
        let c = Config::seeded(&installed(&["chromium", "com.system76.CosmicTerm"]), Some(FAVS));
        let ids: Vec<_> = c.groups[0].tiles.iter().map(|t| t.app.as_str()).collect();
        assert_eq!(ids, ["chromium", "com.system76.CosmicTerm"]);
    }

    #[test]
    fn seeds_from_fallback_when_favorites_missing_or_all_uninstalled() {
        let inst = installed(&["chromium", "com.system76.CosmicFiles"]);
        for favs in [None, Some("[\"not.installed\"]")] {
            let c = Config::seeded(&inst, favs);
            let ids: Vec<_> = c.groups[0].tiles.iter().map(|t| t.app.as_str()).collect();
            assert_eq!(ids, ["chromium", "com.system76.CosmicFiles"]);
        }
    }

    #[test]
    fn round_trips_through_toml() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config.toml");
        let mut c = Config::seeded(&installed(&["chromium"]), None);
        c.groups[0].tiles[0].size = TileSize::Wide;
        c.groups[0].tiles[0].source = Some("weather".into());
        c.save_to(&p).unwrap();
        assert_eq!(Config::load_from(&p, &[], None), c);
    }

    #[test]
    fn corrupt_file_is_moved_aside_and_defaults_seeded() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config.toml");
        std::fs::write(&p, "groups = 7").unwrap();
        let c = Config::load_from(&p, &installed(&["chromium"]), None);
        assert_eq!(c.groups[0].tiles[0].app, "chromium");
        assert_eq!(std::fs::read_to_string(p.with_extension("toml.bak")).unwrap(), "groups = 7");
    }

    #[test]
    fn pin_unpin_resize_move() {
        let mut c = Config { groups: vec![], ..Config::default() };
        c.pin("a"); c.pin("b");
        assert!(c.is_pinned("a"));
        assert_eq!(c.groups[0].tiles[0].size, TileSize::Medium);
        c.resize((0, 1), TileSize::Small);
        assert_eq!(c.groups[0].tiles[1].size, TileSize::Small);
        let g = c.add_group("Tools".into());
        c.move_tile((0, 0), g, 0);
        assert_eq!(c.groups[1].tiles[0].app, "a");
        c.unpin("b");
        assert!(!c.is_pinned("b"));
    }

    #[test]
    fn move_within_a_group_reorders() {
        let mut c = Config { groups: vec![], ..Config::default() };
        for a in ["a", "b", "c"] { c.pin(a); }
        c.move_tile((0, 0), 0, 2);
        let ids: Vec<_> = c.groups[0].tiles.iter().map(|t| t.app.as_str()).collect();
        assert_eq!(ids, ["b", "c", "a"]);
    }

    #[test]
    fn removing_a_group_keeps_its_tiles() {
        let mut c = Config { groups: vec![], ..Config::default() };
        c.pin("a");
        let g = c.add_group("Tools".into());
        c.groups[g].tiles.push(Tile { app: "b".into(), size: TileSize::Small, source: None });
        c.remove_group(g);
        assert_eq!(c.groups.len(), 1);
        assert!(c.is_pinned("b"));
    }

    #[test]
    fn uninstalled_tiles_are_hidden_but_kept() {
        let mut c = Config { groups: vec![], ..Config::default() };
        c.pin("gone"); c.pin("here");
        let inst: HashSet<&str> = ["here"].into();
        let vis: Vec<_> = c.visible_tiles(0, &inst).into_iter().map(|(i, t)| (i, t.app.clone())).collect();
        assert_eq!(vis, [(1, "here".to_string())]);
        assert!(c.is_pinned("gone"));
    }

    #[test]
    fn out_of_range_refs_are_ignored() {
        let mut c = Config::default();
        c.resize((9, 9), TileSize::Wide);
        c.move_tile((9, 9), 0, 0);
        c.rename_group(9, "x".into());
        c.remove_group(9);
    }
}
```

- [ ] **Step 2: Run to verify it fails** — `cargo test config::` → compile errors.

- [ ] **Step 3: Implement**

```rust
//! What the user has pinned, and how the menu is painted.
//!
//! Written by the menu itself (pin, resize, drag) and by the Settings window,
//! and safe to edit by hand. Pins are app ids without `.desktop`, the same
//! form COSMIC's dock uses, so seeding from the dock is a straight copy.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TileSize { Small, #[default] Medium, Wide }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TileFinish { #[default] Frosted, Solid, Outline }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tile {
    pub app: String,
    #[serde(default)]
    pub size: TileSize,
    /// Live-tile content source. Reserved for v2: parsed and preserved, never drawn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Group {
    pub name: String,
    #[serde(default)]
    pub tiles: Vec<Tile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub finish: TileFinish,
    pub show_most_used: bool,
    #[serde(rename = "group")]
    pub groups: Vec<Group>,
}

impl Default for Config {
    fn default() -> Self {
        Self { finish: TileFinish::Frosted, show_most_used: true, groups: Vec::new() }
    }
}

pub type TileRef = (usize, usize);

pub const FALLBACK_PINS: &[&str] = &[
    "firefox", "chromium", "com.system76.CosmicFiles",
    "com.system76.CosmicTerm", "com.system76.CosmicSettings",
];

const HEADER: &str = "\
# Start Menu configuration. Written by the menu and its Settings window;
# safe to edit by hand. `app` is a desktop-entry id without `.desktop`.
# `size` is small | medium | wide. `source` is reserved for live tiles.
";

/// The dock's favourites file is a RON list of strings. Pull the quoted
/// strings out rather than taking a RON dependency for one list.
pub fn parse_favorites(ron: &str) -> Vec<String> {
    let t = ron.trim();
    if !(t.starts_with('[') && t.ends_with(']')) {
        return Vec::new();
    }
    t.split('"').skip(1).step_by(2).map(str::to_owned).collect()
}

fn favorites_file() -> Option<String> {
    let p = dirs::config_dir()?.join("cosmic/com.system76.CosmicAppList/v1/favorites");
    std::fs::read_to_string(p).ok()
}

impl Config {
    pub fn path() -> Option<PathBuf> {
        Some(dirs::config_dir()?.join("cosmic-start-menu-applet").join("config.toml"))
    }

    pub fn seeded(installed: &[String], favorites: Option<&str>) -> Config {
        let has = |id: &str| installed.iter().any(|i| i == id);
        let mut ids: Vec<String> = favorites.map(parse_favorites).unwrap_or_default()
            .into_iter().filter(|id| has(id)).collect();
        if ids.is_empty() {
            ids = FALLBACK_PINS.iter().filter(|id| has(id)).map(|s| s.to_string()).collect();
        }
        Config {
            groups: vec![Group {
                name: crate::fl!("default-group"),
                tiles: ids.into_iter().map(|app| Tile { app, size: TileSize::Medium, source: None }).collect(),
            }],
            ..Config::default()
        }
    }

    /// Load from the real path. `installed` is only consulted when seeding.
    pub fn load() -> Config {
        let installed: Vec<String> = crate::apps::load_all().into_iter().map(|a| a.id).collect();
        match Self::path() {
            Some(p) => Self::load_from(&p, &installed, favorites_file().as_deref()),
            None => Self::seeded(&installed, favorites_file().as_deref()),
        }
    }

    pub fn load_from(path: &Path, installed: &[String], favorites: Option<&str>) -> Config {
        let raw = match std::fs::read_to_string(path) {
            Ok(raw) => raw,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let c = Self::seeded(installed, favorites);
                if let Err(err) = c.save_to(path) { tracing::warn!("{err}"); }
                return c;
            }
            Err(e) => {
                tracing::warn!("could not read {}: {e}", path.display());
                return Self::seeded(installed, favorites);
            }
        };
        match toml::from_str(&raw) {
            Ok(c) => c,
            Err(e) => {
                let bak = path.with_extension("toml.bak");
                tracing::error!("{} is invalid ({e}); moved to {}", path.display(), bak.display());
                let _ = std::fs::rename(path, &bak);
                let c = Self::seeded(installed, favorites);
                if let Err(err) = c.save_to(path) { tracing::warn!("{err}"); }
                c
            }
        }
    }

    pub fn save(&self) -> Result<(), String> {
        self.save_to(&Self::path().ok_or("no config directory")?)
    }

    /// Write-then-rename so a crash can't leave a half-written file.
    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
        }
        let body = toml::to_string_pretty(self).map_err(|e| format!("could not encode: {e}"))?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, format!("{HEADER}\n{body}")).map_err(|e| format!("could not write {}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, path).map_err(|e| format!("could not replace {}: {e}", path.display()))
    }

    pub fn is_pinned(&self, app: &str) -> bool {
        self.groups.iter().any(|g| g.tiles.iter().any(|t| t.app == app))
    }

    pub fn pin(&mut self, app: &str) {
        if self.is_pinned(app) { return; }
        if self.groups.is_empty() {
            self.groups.push(Group { name: crate::fl!("default-group"), tiles: Vec::new() });
        }
        self.groups[0].tiles.push(Tile { app: app.into(), size: TileSize::Medium, source: None });
    }

    pub fn unpin(&mut self, app: &str) {
        for g in &mut self.groups { g.tiles.retain(|t| t.app != app); }
    }

    pub fn resize(&mut self, (g, i): TileRef, size: TileSize) {
        if let Some(t) = self.groups.get_mut(g).and_then(|g| g.tiles.get_mut(i)) { t.size = size; }
    }

    pub fn move_tile(&mut self, (g, i): TileRef, to_group: usize, to_index: usize) {
        if to_group >= self.groups.len() || self.groups.get(g).is_none_or(|grp| i >= grp.tiles.len()) {
            return;
        }
        let tile = self.groups[g].tiles.remove(i);
        let dest = &mut self.groups[to_group].tiles;
        dest.insert(to_index.min(dest.len()), tile);
    }

    pub fn add_group(&mut self, name: String) -> usize {
        self.groups.push(Group { name, tiles: Vec::new() });
        self.groups.len() - 1
    }

    pub fn rename_group(&mut self, group: usize, name: String) {
        if let Some(g) = self.groups.get_mut(group) { g.name = name; }
    }

    pub fn remove_group(&mut self, group: usize) {
        if group >= self.groups.len() { return; }
        let removed = self.groups.remove(group);
        if self.groups.is_empty() {
            self.groups.push(Group { name: crate::fl!("default-group"), tiles: removed.tiles });
            return;
        }
        let target = group.saturating_sub(1).min(self.groups.len() - 1);
        self.groups[target].tiles.extend(removed.tiles);
    }

    pub fn visible_tiles<'a>(&'a self, group: usize, installed: &HashSet<&str>) -> Vec<(usize, &'a Tile)> {
        self.groups.get(group).map(|g| {
            g.tiles.iter().enumerate().filter(|(_, t)| installed.contains(t.app.as_str())).collect()
        }).unwrap_or_default()
    }
}
```

In `removing_a_group_keeps_its_tiles`, the removed group is group 1, so its tiles go into group 0.

- [ ] **Step 4: Run to verify it passes** — `cargo test config::` → 10 passed.

- [ ] **Step 5: Commit** — "Store pinned tile groups, seeded from the dock's favourites" (with trailers).

---

### Task 5: `usage` — launch counts for "Most used"

**Files:**
- Create: `src/usage.rs`
- Modify: `src/main.rs` (`mod usage;`)

**Interfaces:**
- Produces:
  ```rust
  #[derive(Default, Serialize, Deserialize, PartialEq, Debug, Clone)]
  pub struct Usage { pub counts: BTreeMap<String, u32> }
  impl Usage {
      pub fn path() -> Option<PathBuf>;           // dirs::state_dir()/cosmic-start-menu-applet/usage.toml
      pub fn load_from(path: &Path) -> Usage;     // missing or corrupt → empty
      pub fn save_to(&self, path: &Path) -> Result<(), String>;
      pub fn record(&mut self, app: &str);
      pub fn top(&self, n: usize, installed: &HashSet<&str>) -> Vec<String>; // most launched first, ties by id, installed only
  }
  ```

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_orders_by_count_then_id_and_skips_uninstalled() {
        let mut u = Usage::default();
        for a in ["b", "a", "b", "gone", "gone", "gone", "c"] { u.record(a); }
        let inst: HashSet<&str> = ["a", "b", "c"].into();
        assert_eq!(u.top(2, &inst), ["b", "a"]);
    }

    #[test]
    fn round_trip_and_corrupt_is_empty() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("usage.toml");
        let mut u = Usage::default();
        u.record("x");
        u.save_to(&p).unwrap();
        assert_eq!(Usage::load_from(&p), u);
        std::fs::write(&p, "counts = 3").unwrap();
        assert_eq!(Usage::load_from(&p), Usage::default());
    }

    #[test]
    fn count_saturates_instead_of_overflowing() {
        let mut u = Usage::default();
        u.counts.insert("x".into(), u32::MAX);
        u.record("x");
        assert_eq!(u.counts["x"], u32::MAX);
    }
}
```

- [ ] **Step 2: Run to verify it fails** — `cargo test usage::`

- [ ] **Step 3: Implement**

```rust
//! How often each app is launched from the menu, for the "Most used" list.
//! Kept in the state dir, not config: it is history, not a preference.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub counts: BTreeMap<String, u32>,
}

impl Usage {
    pub fn path() -> Option<PathBuf> {
        Some(dirs::state_dir()?.join("cosmic-start-menu-applet").join("usage.toml"))
    }

    pub fn load_from(path: &Path) -> Usage {
        std::fs::read_to_string(path).ok()
            .and_then(|raw| toml::from_str(&raw).map_err(|e| tracing::warn!("{}: {e}", path.display())).ok())
            .unwrap_or_default()
    }

    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let body = toml::to_string(self).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, body).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    }

    pub fn record(&mut self, app: &str) {
        let n = self.counts.entry(app.to_owned()).or_insert(0);
        *n = n.saturating_add(1);
    }

    pub fn top(&self, n: usize, installed: &HashSet<&str>) -> Vec<String> {
        let mut v: Vec<(&String, &u32)> = self.counts.iter()
            .filter(|(id, _)| installed.contains(id.as_str())).collect();
        v.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        v.into_iter().take(n).map(|(id, _)| id.clone()).collect()
    }
}
```

- [ ] **Step 4: Run to verify it passes** — `cargo test usage::` → 3 passed.

- [ ] **Step 5: Commit** — "Count launches for the Most used list" (with trailers).

---

### Task 6: `tile_layout` — pack Small/Medium/Wide tiles into a six-cell grid

**Files:**
- Create: `src/tile_layout.rs`
- Modify: `src/main.rs` (`mod tile_layout;`)

**Interfaces:**
- Consumes: `config::TileSize`
- Produces:
  ```rust
  pub const COLUMNS: u16 = 6;
  pub fn footprint(size: TileSize) -> (u16, u16);             // (cols, rows): Small 1×1, Medium 2×2, Wide 4×2
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub struct Placement { pub col: u16, pub row: u16, pub cols: u16, pub rows: u16 } // 0-based cells
  pub struct Pack { pub tiles: Vec<Placement>, pub rows: u16 }
  pub fn pack(sizes: &[TileSize]) -> Pack;                    // same order as input, first-fit, never reorders
  ```

This is CCCA's `tile_layout::pack` (CCCA `src/tile_layout.rs` lines 380–480), rewritten for three sizes and 0-based output. Ghost output is dropped because the Start menu leaves gaps empty.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use TileSize::*;

    fn p(col: u16, row: u16, s: TileSize) -> Placement {
        let (cols, rows) = footprint(s);
        Placement { col, row, cols, rows }
    }

    #[test]
    fn three_mediums_fill_a_row() {
        let pk = pack(&[Medium, Medium, Medium]);
        assert_eq!(pk.tiles, [p(0, 0, Medium), p(2, 0, Medium), p(4, 0, Medium)]);
        assert_eq!(pk.rows, 2);
    }

    #[test]
    fn smalls_tuck_into_the_gap_beside_a_wide() {
        let pk = pack(&[Wide, Small, Small, Small, Small]);
        assert_eq!(pk.tiles, [p(0, 0, Wide), p(4, 0, Small), p(5, 0, Small), p(4, 1, Small), p(5, 1, Small)]);
        assert_eq!(pk.rows, 2);
    }

    #[test]
    fn a_wide_that_does_not_fit_starts_a_new_band() {
        let pk = pack(&[Medium, Medium, Wide]);
        assert_eq!(pk.tiles[2], p(0, 2, Wide));
        assert_eq!(pk.rows, 4);
    }

    #[test]
    fn empty_is_zero_rows() {
        assert_eq!(pack(&[]).rows, 0);
    }

    #[test]
    fn no_two_tiles_overlap() {
        let sizes = [Small, Wide, Medium, Small, Medium, Wide, Small, Small, Medium];
        let pk = pack(&sizes);
        let mut seen = std::collections::HashSet::new();
        for t in &pk.tiles {
            for c in t.col..t.col + t.cols { for r in t.row..t.row + t.rows {
                assert!(seen.insert((c, r)), "overlap at {c},{r}");
                assert!(c < COLUMNS);
            }}
        }
    }
}
```

- [ ] **Step 2: Run to verify it fails** — `cargo test tile_layout::`

- [ ] **Step 3: Implement**

```rust
//! Where each pinned tile sits in its group's grid.
//!
//! Windows 10 groups are six small cells wide. Tiles are placed first-fit in
//! the user's order — the packer never reorders, so a tile stays where the
//! user dropped it, and a gap it cannot fill stays a gap.

use crate::config::TileSize;

pub const COLUMNS: u16 = 6;

pub fn footprint(size: TileSize) -> (u16, u16) {
    match size {
        TileSize::Small => (1, 1),
        TileSize::Medium => (2, 2),
        TileSize::Wide => (4, 2),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement { pub col: u16, pub row: u16, pub cols: u16, pub rows: u16 }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pack { pub tiles: Vec<Placement>, pub rows: u16 }

pub fn pack(sizes: &[TileSize]) -> Pack {
    let mut occupied: Vec<[bool; COLUMNS as usize]> = Vec::new();
    let mut tiles = Vec::with_capacity(sizes.len());
    for &size in sizes {
        let (w, h) = footprint(size);
        let mut row = 0usize;
        let col = loop {
            while occupied.len() < row + h as usize { occupied.push([false; COLUMNS as usize]); }
            let fits = |c: usize| (0..w as usize).all(|dc| (0..h as usize).all(|dr| !occupied[row + dr][c + dc]));
            if let Some(c) = (0..=(COLUMNS - w) as usize).find(|&c| fits(c)) { break c; }
            row += 1;
        };
        for dc in 0..w as usize { for dr in 0..h as usize { occupied[row + dr][col + dc] = true; } }
        tiles.push(Placement { col: col as u16, row: row as u16, cols: w, rows: h });
    }
    let rows = occupied.iter().rposition(|r| r.iter().any(|&c| c)).map_or(0, |i| i as u16 + 1);
    Pack { tiles, rows }
}
```

- [ ] **Step 4: Run to verify it passes** — `cargo test tile_layout::` → 5 passed.

- [ ] **Step 5: Commit** — "Pack pinned tiles into a six-cell grid" (with trailers).

---

### Task 7: `session` and `launch` — power actions and starting apps

**Files:**
- Create: `src/session.rs`, `src/launch.rs`
- Modify: `src/main.rs` (`mod session; mod launch;`), `i18n/en/main.ftl`

**Interfaces:**
- Consumes: `apps::App`, `apps::Action`, `usage::Usage`, `process::spawn_and_reap`
- Produces:
  ```rust
  // session.rs
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum Power { Lock, LogOut, Suspend, Restart, ShutDown }
  pub const ALL: [Power; 5];
  impl Power { pub fn l10n_key(self) -> &'static str; pub fn icon(self) -> &'static str; }
  pub fn osd_arg(p: Power) -> Option<&'static str>;          // LogOut→"log-out", Restart→"restart", ShutDown→"shutdown"
  pub async fn run(p: Power) -> Result<(), String>;
  pub fn open_settings_page(page: Option<&str>) -> Result<(), String>; // `cosmic-settings [page]`
  pub fn open_files() -> Result<(), String>;                           // `xdg-open ~`
  // launch.rs
  pub async fn app(app: App) ;                                // spawn_desktop_exec(exec, [], Some(id), terminal)
  pub async fn action(app_id: String, action: Action, terminal: bool);
  pub fn record(app_id: &str);                                // load usage, record, save (runs in spawn_blocking)
  ```

- [ ] **Step 1: Write the failing tests** (in `session.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logout_restart_shutdown_go_through_cosmic_osd() {
        assert_eq!(osd_arg(Power::LogOut), Some("log-out"));
        assert_eq!(osd_arg(Power::Restart), Some("restart"));
        assert_eq!(osd_arg(Power::ShutDown), Some("shutdown"));
        assert_eq!(osd_arg(Power::Lock), None);
        assert_eq!(osd_arg(Power::Suspend), None);
    }

    #[test]
    fn every_power_label_is_translated() {
        for p in ALL { assert_ne!(crate::fl!(p.l10n_key()), p.l10n_key()); }
    }
}
```

`fl!` must accept a runtime `&str` key. If CCCA's macro only takes literals, add the fn form `crate::i18n::get(key: &str) -> String` (CCCA's `i18n.rs` already has a lookup function behind the macro; expose it as `pub fn get`) and use that in this test.

- [ ] **Step 2: Run to verify it fails** — `cargo test session::`

- [ ] **Step 3: Add the strings to `i18n/en/main.ftl`**

```ftl
power = Power
power-lock = Lock
power-log-out = Log out
power-suspend = Sleep
power-restart = Restart
power-shut-down = Shut down
power-failed = Couldn’t { $action }: { $error }
rail-files = Files
rail-settings = Settings
rail-account = Account settings
```

- [ ] **Step 4: Implement `session.rs`**

```rust
//! Power actions. Log out / restart / shut down go through `cosmic-osd`, which
//! shows COSMIC's own confirm-with-countdown dialog — the same one the stock
//! power applet shows. Lock and suspend have no dialog and go to logind.

use crate::process::{host_command, spawn_and_reap};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Power { Lock, LogOut, Suspend, Restart, ShutDown }

pub const ALL: [Power; 5] = [Power::Lock, Power::LogOut, Power::Suspend, Power::Restart, Power::ShutDown];

impl Power {
    pub fn l10n_key(self) -> &'static str {
        match self {
            Power::Lock => "power-lock", Power::LogOut => "power-log-out",
            Power::Suspend => "power-suspend", Power::Restart => "power-restart",
            Power::ShutDown => "power-shut-down",
        }
    }
    pub fn icon(self) -> &'static str {
        match self {
            Power::Lock => "system-lock-screen-symbolic", Power::LogOut => "system-log-out-symbolic",
            Power::Suspend => "system-suspend-symbolic", Power::Restart => "system-reboot-symbolic",
            Power::ShutDown => "system-shutdown-symbolic",
        }
    }
}

pub fn osd_arg(p: Power) -> Option<&'static str> {
    match p {
        Power::LogOut => Some("log-out"),
        Power::Restart => Some("restart"),
        Power::ShutDown => Some("shutdown"),
        Power::Lock | Power::Suspend => None,
    }
}

#[zbus::proxy(interface = "org.freedesktop.login1.Manager",
              default_service = "org.freedesktop.login1", default_path = "/org/freedesktop/login1")]
trait Login1 {
    fn suspend(&self, interactive: bool) -> zbus::Result<()>;
}

#[zbus::proxy(interface = "org.freedesktop.login1.Session",
              default_service = "org.freedesktop.login1", default_path = "/org/freedesktop/login1/session/auto")]
trait Login1Session {
    fn lock(&self) -> zbus::Result<()>;
}

pub async fn run(p: Power) -> Result<(), String> {
    if let Some(arg) = osd_arg(p) {
        let mut cmd = host_command("cosmic-osd");
        cmd.arg(arg);
        return spawn_and_reap(cmd).map(|_| ()).map_err(|e| e.to_string());
    }
    let conn = zbus::Connection::system().await.map_err(|e| e.to_string())?;
    match p {
        Power::Lock => Login1SessionProxy::new(&conn).await.map_err(|e| e.to_string())?
            .lock().await.map_err(|e| e.to_string()),
        Power::Suspend => Login1Proxy::new(&conn).await.map_err(|e| e.to_string())?
            .suspend(true).await.map_err(|e| e.to_string()),
        _ => unreachable!("osd actions returned above"),
    }
}

pub fn open_settings_page(page: Option<&str>) -> Result<(), String> {
    let mut cmd = host_command("cosmic-settings");
    if let Some(page) = page { cmd.arg(page); }
    spawn_and_reap(cmd).map(|_| ()).map_err(|e| e.to_string())
}

pub fn open_files() -> Result<(), String> {
    let mut cmd = host_command("xdg-open");
    cmd.arg(dirs::home_dir().ok_or("no home directory")?);
    spawn_and_reap(cmd).map(|_| ()).map_err(|e| e.to_string())
}
```

If `host_command` returns a `tokio::process::Command` rather than a `std` one, match whatever CCCA's `process.rs` uses.

- [ ] **Step 5: Implement `launch.rs`**

```rust
//! Starting apps, the way COSMIC's own launcher does: through libcosmic's
//! `spawn_desktop_exec`, which strips field codes and puts the app in its own
//! systemd scope.

use crate::apps::{Action, App};
use crate::usage::Usage;

pub async fn app(app: App) {
    let Some(exec) = app.exec.clone() else { return };
    cosmic::desktop::spawn_desktop_exec(exec, Vec::<(String, String)>::new(), Some(&app.id), app.terminal).await;
    record_blocking(app.id).await;
}

pub async fn action(app_id: String, action: Action, terminal: bool) {
    cosmic::desktop::spawn_desktop_exec(action.exec, Vec::<(String, String)>::new(), Some(&app_id), terminal).await;
    record_blocking(app_id).await;
}

async fn record_blocking(app_id: String) {
    let _ = tokio::task::spawn_blocking(move || record(&app_id)).await;
}

pub fn record(app_id: &str) {
    let Some(path) = Usage::path() else { return };
    let mut u = Usage::load_from(&path);
    u.record(app_id);
    if let Err(e) = u.save_to(&path) { tracing::warn!("could not save usage: {e}"); }
}
```

- [ ] **Step 6: Run the tests** — `cargo test` → all pass.

- [ ] **Step 7: Check lock by hand (the only action safe to try)**

Add a temporary `--lock` arm to `main.rs` that calls `session::run(Power::Lock)` on a current-thread tokio runtime. Run `cargo run -- --lock` and confirm the screen locks. Then **remove the temporary arm**. Log out, restart and shut down are checked in Task 8 through their confirm dialog, which you cancel.

- [ ] **Step 8: Commit** — "Add power actions and app launching" (with trailers).

---

### Task 8: Popup layout — rail with power menu, and the A–Z list that launches apps

**Files:**
- Create: `src/ui/mod.rs`, `src/ui/rail.rs`, `src/ui/app_list.rs`
- Modify: `src/app.rs`, `src/main.rs` (`mod ui;`), `i18n/en/main.ftl`

**Interfaces:**
- Consumes: everything from Tasks 2–7.
- Produces (in `ui/mod.rs`, copied and trimmed from CCCA `src/ui/mod.rs` lines 30–60 and 355–520):
  ```rust
  pub struct Spacing { pub gap: u16, pub pad_y: u16, pub pad_x: u16, pub section: u16 }
  impl Spacing { pub fn from_theme(theme: &cosmic::Theme) -> Self }
  pub fn radius(theme: &cosmic::Theme, height: f32) -> f32;     // CCCA pill_radius: (h/2).min(radius_xl[0])
  pub fn tile_button_class(finish: TileFinish) -> button::ButtonClass;   // CCCA verbatim
  pub fn quiet_button() -> button::ButtonClass;                          // CCCA verbatim (list rows, rail)
  pub const RAIL_WIDTH: f32 = 48.0; pub const LIST_WIDTH: f32 = 272.0;
  ```
  In `app.rs`: new state fields `apps: Vec<apps::App>`, `config: Config`, `usage_top: Vec<String>`, `power_open: bool`, `error: Option<String>`. New messages:
  ```rust
  AppsLoaded(Vec<apps::App>, Vec<String> /* most used */),
  Launch(usize),                       // index into self.apps
  PowerMenu(bool), Power(session::Power), PowerFailed(String),
  OpenFiles, OpenSettingsApp, OpenAccount,
  ```

- [ ] **Step 1: Copy the theme helpers into `src/ui/mod.rs`**

Copy these from CCCA `src/ui/mod.rs`, changing only the `TileFinish` import path to `crate::config::TileFinish`: `Spacing` and its impl, `tile_component`, `finish_paint`, `FROSTED_TILE_ALPHA`, `tile_border` (take the radius as a parameter, not from `tile_radius`), `tile_button_class` and `quiet_button`. Keep their doc comments; they record the reasons (for example, why `radius_s` must not be used). Then add:

```rust
/// Half of `height`, capped by the largest radius the desktop's Appearance
/// "roundness" allows — Round lets a pill through, Slightly round trims it,
/// Square keeps it square.
pub fn radius(theme: &cosmic::Theme, height: f32) -> f32 {
    (height / 2.0).min(theme.cosmic().corner_radii.radius_xl[0])
}

pub const RAIL_WIDTH: f32 = 48.0;
pub const LIST_WIDTH: f32 = 272.0;
pub const ICON: u16 = 24;
```

- [ ] **Step 2: Write `src/ui/rail.rs`**

```rust
//! The thin left strip: account at the top; Files, Settings and Power at the
//! bottom, like Windows 10. Icons only; the name is in the tooltip.

use cosmic::iced::{Alignment, Length};
use cosmic::widget::{button, column, icon, popover, tooltip, vertical_space};
use cosmic::Element;

use crate::app::Message;
use crate::fl;
use crate::session::{self, Power};
use crate::ui::{quiet_button, RAIL_WIDTH, ICON};

fn rail_button<'a>(icon_name: &'static str, label: String, msg: Message) -> Element<'a, Message> {
    tooltip(
        button::custom(icon::from_name(icon_name).size(ICON))
            .class(quiet_button())
            .width(Length::Fixed(RAIL_WIDTH))
            .height(Length::Fixed(RAIL_WIDTH))
            .on_press(msg),
        cosmic::widget::text(label),
        tooltip::Position::Right,
    )
    .into()
}

pub fn view<'a>(power_open: bool) -> Element<'a, Message> {
    let power_menu = column::with_children(
        session::ALL.iter().map(|&p| {
            button::custom(
                cosmic::widget::row::with_children(vec![
                    icon::from_name(p.icon()).size(16).into(),
                    cosmic::widget::text(fl!(p.l10n_key())).into(),
                ]).spacing(8).align_y(Alignment::Center),
            )
            .class(quiet_button())
            .width(Length::Fill)
            .on_press(Message::Power(p))
            .into()
        }).collect::<Vec<_>>(),
    )
    .width(Length::Fixed(180.0));

    let power = rail_button("system-shutdown-symbolic", fl!("power"), Message::PowerMenu(!power_open));
    let mut power = popover(power).on_close(Message::PowerMenu(false));
    if power_open {
        power = power.popup(cosmic::widget::container(power_menu).class(cosmic::theme::Container::Dropdown));
    }

    column::with_children(vec![
        rail_button("avatar-default-symbolic", fl!("rail-account"), Message::OpenAccount),
        vertical_space().into(),
        rail_button("folder-symbolic", fl!("rail-files"), Message::OpenFiles),
        rail_button("preferences-system-symbolic", fl!("rail-settings"), Message::OpenSettingsApp),
        power.into(),
    ])
    .width(Length::Fixed(RAIL_WIDTH))
    .height(Length::Fill)
    .into()
}
```

If `popover` needs `Position::Bottom` or `Point` to sit above the button, set `.position(popover::Position::Bottom)` and check how it looks. The menu must not be clipped by the popup edge. If it is, open the power menu as an inline list that replaces the rail's lower buttons.

- [ ] **Step 3: Write `src/ui/app_list.rs`** (Most used + A–Z; the letter jump comes in Task 13)

```rust
//! Middle column: an optional "Most used" block, then every app under its
//! letter header.

use cosmic::iced::{Alignment, Length};
use cosmic::widget::{button, column, icon, row, scrollable, text};
use cosmic::desktop::IconSourceExt;
use cosmic::Element;

use crate::app::Message;
use crate::apps::{self, App};
use crate::fl;
use crate::ui::{quiet_button, ICON, LIST_WIDTH};

pub fn app_row<'a>(app: &'a App, index: usize, selected: bool) -> Element<'a, Message> {
    let body = row::with_children(vec![
        icon(app.icon.as_cosmic_icon()).size(ICON).into(),
        text(&app.name).into(),
    ])
    .spacing(12)
    .align_y(Alignment::Center);
    let b = button::custom(body).width(Length::Fill).on_press(Message::Launch(index));
    let b = if selected { b.class(cosmic::theme::Button::Suggested) } else { b.class(quiet_button()) };
    b.into()
}

pub fn letter_header<'a>(letter: char) -> Element<'a, Message> {
    button::custom(text::heading(letter.to_string()))
        .class(quiet_button())
        .on_press(Message::LetterGrid(true))
        .into()
}

pub fn view<'a>(apps_list: &'a [App], most_used: &[String], show_most_used: bool) -> Element<'a, Message> {
    let mut col = column::with_capacity(apps_list.len() + 16).spacing(2);
    if show_most_used && !most_used.is_empty() {
        col = col.push(text::heading(fl!("most-used")));
        for id in most_used {
            if let Some(i) = apps_list.iter().position(|a| &a.id == id) {
                col = col.push(app_row(&apps_list[i], i, false));
            }
        }
    }
    for (letter, idxs) in apps::sections(apps_list) {
        col = col.push(letter_header(letter));
        for i in idxs {
            col = col.push(app_row(&apps_list[i], i, false));
        }
    }
    scrollable(col).width(Length::Fixed(LIST_WIDTH)).height(Length::Fill).into()
}
```

(`Message::LetterGrid(bool)` is added now as a no-op arm; Task 13 implements it.)

- [ ] **Step 4: Wire the popup in `app.rs`**

Add the state and messages from **Interfaces**. On `TogglePopup` when opening, also run the load off the UI thread:

```rust
let load = Task::perform(
    async {
        tokio::task::spawn_blocking(|| {
            let apps = crate::apps::load_all();
            let ids: std::collections::HashSet<&str> = apps.iter().map(|a| a.id.as_str()).collect();
            let top = crate::usage::Usage::path()
                .map(|p| crate::usage::Usage::load_from(&p).top(5, &ids))
                .unwrap_or_default();
            (apps, top)
        })
        .await
        .unwrap_or_default()
    },
    |(apps, top)| cosmic::Action::App(Message::AppsLoaded(apps, top)),
);
```

Load `Config::load()` in `init` and again on each open, as CCCA does. Put the load inside the same `spawn_blocking`, because `Config::load` reads files. Handle the messages:

```rust
Message::AppsLoaded(apps, top) => { self.apps = apps; self.usage_top = top; Task::none() }
Message::Launch(i) => {
    let Some(app) = self.apps.get(i).cloned() else { return Task::none() };
    let close = self.close_popup();
    Task::batch([close, Task::perform(crate::launch::app(app), |()| cosmic::Action::None)])
}
Message::PowerMenu(open) => { self.power_open = open; Task::none() }
Message::Power(p) => {
    self.power_open = false;
    Task::perform(crate::session::run(p), move |r| match r {
        Ok(()) => cosmic::Action::None,
        Err(e) => cosmic::Action::App(Message::PowerFailed(fl!("power-failed", action = fl!(p.l10n_key()), error = e))),
    })
}
Message::PowerFailed(e) => { self.error = Some(e); Task::none() }
Message::OpenFiles => { self.report(crate::session::open_files()); self.close_popup() }
Message::OpenSettingsApp => { self.report(crate::session::open_settings_page(None)); self.close_popup() }
Message::OpenAccount => { self.report(crate::session::open_settings_page(Some("users"))); self.close_popup() }
```

Add the helpers `fn close_popup(&mut self) -> Task<Message>` (take `self.popup`, return `destroy_popup(id)` or `Task::none()`) and `fn report(&mut self, r: Result<(), String>)` (on `Err`, set `self.error`). Opening the popup must reset `power_open = false` and `error = None`.

In `view_window`, lay out `row![rail::view(self.power_open), app_list::view(&self.apps, &self.usage_top, self.config.show_most_used), tiles placeholder: vertical_space]` inside a `container` with `.padding(spacing.section)` and the fixed popup size. If `self.error` is `Some`, draw it as a `text::caption` line at the bottom of the popup.

- [ ] **Step 5: Build, install, and check by hand**

```bash
cargo test && just check && just install
```
Then restart the applet so the new binary loads. Remove it from the panel and add it back (panel settings → applets), **never `pkill cosmic-panel`**. Check:
- Opening shows the rail, Most used (empty on first run) and the full A–Z list with headers. `#` comes first if any app names start with a digit.
- Clicking an app launches it and closes the menu. The next time it opens, the app is listed under Most used.
- Power → Log out shows COSMIC's countdown dialog. **Cancel it.** Do the same for Restart and Shut down: dialog, then cancel.
- Account opens COSMIC Settings → Users. Settings opens COSMIC Settings. Files opens the file manager.
- Switch dark/light and the accent colour in COSMIC Settings → Appearance while the menu is open. It restyles live.

- [ ] **Step 6: Commit** — "Lay out the rail and the A to Z list, and launch apps from it" (with trailers).

---

### Task 9: Tile groups on the right

**Files:**
- Create: `src/ui/tiles.rs`
- Modify: `src/app.rs`, `src/main.rs`

**Interfaces:**
- Consumes: `config::{Config, TileFinish, TileSize}`, `tile_layout::{pack, Placement}`, `apps::App`, `ui::{tile_button_class, radius, Spacing}`
- Produces: `pub fn view<'a>(config: &'a Config, apps: &'a [App], spacing: Spacing) -> Element<'a, Message>` and `pub const CELL: f32 = 44.0`. New message `Message::LaunchId(String)` (launch by app id).

- [ ] **Step 1: Implement `src/ui/tiles.rs`**

Each group is a heading, then a `stack` of `pin`-positioned tiles in a fixed-size box. Positions come straight from `tile_layout::pack`. This avoids `Grid`, which is known to collapse Fill children and misplace spans (see James's libcosmic Grid notes).

```rust
//! The right-hand column: pinned tiles in named groups.

use std::collections::HashSet;
use cosmic::iced::widget::{pin, stack};
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{button, column, container, icon, scrollable, text};
use cosmic::desktop::IconSourceExt;
use cosmic::Element;

use crate::app::Message;
use crate::apps::App;
use crate::config::{Config, TileSize};
use crate::tile_layout::{self, COLUMNS};
use crate::ui::{tile_button_class, Spacing};

pub const CELL: f32 = 44.0;

fn span(cells: u16, gap: f32) -> f32 {
    CELL * f32::from(cells) + gap * f32::from(cells.saturating_sub(1))
}

fn tile<'a>(app: &'a App, size: TileSize, w: f32, h: f32, config: &Config) -> Element<'a, Message> {
    let icon_size = match size { TileSize::Small => 24, TileSize::Medium => 32, TileSize::Wide => 32 };
    let mut body = column::with_capacity(2).align_x(Alignment::Center).spacing(4)
        .push(icon(app.icon.as_cosmic_icon()).size(icon_size));
    if size != TileSize::Small {
        body = body.push(text::caption(&app.name).wrapping(cosmic::iced::widget::text::Wrapping::None));
    }
    button::custom(container(body).center(Length::Fill))
        .class(tile_button_class(config.finish))
        .width(Length::Fixed(w))
        .height(Length::Fixed(h))
        .on_press(Message::LaunchId(app.id.clone()))
        .into()
}

pub fn view<'a>(config: &'a Config, apps: &'a [App], spacing: Spacing) -> Element<'a, Message> {
    let gap = f32::from(spacing.gap);
    let installed: HashSet<&str> = apps.iter().map(|a| a.id.as_str()).collect();
    let mut groups = column::with_capacity(config.groups.len() * 2).spacing(spacing.section);
    for (g, group) in config.groups.iter().enumerate() {
        let visible = config.visible_tiles(g, &installed);
        let packed = tile_layout::pack(&visible.iter().map(|(_, t)| t.size).collect::<Vec<_>>());
        let mut layer = stack::with_capacity(visible.len());
        for ((_, t), p) in visible.iter().zip(packed.tiles.iter()) {
            let Some(app) = apps.iter().find(|a| a.id == t.app) else { continue };
            let (x, y) = (f32::from(p.col) * (CELL + gap), f32::from(p.row) * (CELL + gap));
            layer = layer.push(pin(tile(app, t.size, span(p.cols, gap), span(p.rows, gap), config)).x(x).y(y));
        }
        groups = groups
            .push(text::heading(&group.name))
            .push(container(layer).width(Length::Fixed(span(COLUMNS, gap))).height(Length::Fixed(span(packed.rows, gap))));
    }
    scrollable(groups).height(Length::Fill).into()
}
```

If `pin`/`stack` aren't re-exported at `cosmic::iced::widget` on this rev, import them from `cosmic::iced_widget`. Both exist in the pinned iced (`iced/widget/src/helpers.rs`).

- [ ] **Step 2: Wire it in**

In `view_window`, replace the placeholder with `tiles::view(&self.config, &self.apps, Spacing::from_theme(theme))`. Handle `Message::LaunchId(id)` by finding the app's index and reusing the `Launch(i)` path.

- [ ] **Step 3: Check by hand**

Run `just install`, then remove and re-add the applet. Check:
- On first run, one "Pinned" group appears containing the dock favourites (Claude, Chromium, Terminal, the web app, Files) as Medium tiles, three to a row.
- Hand-edit `~/.config/cosmic-start-menu-applet/config.toml`: set one tile to `wide` and another to `small`, then reopen the menu. The sizes match and nothing overlaps.
- Appearance → Roundness: cycle Round / Slightly round / Square. The tile corners follow.
- Frosted styling on: the tiles are glass. Set `finish = "outline"` and reopen: the tiles show only an edge.
- Clicking a tile launches the app.

- [ ] **Step 4: Commit** — "Draw pinned tile groups" (with trailers).

---

### Task 10: Type-to-search and keyboard navigation

**Files:**
- Modify: `src/app.rs`, `src/ui/app_list.rs`, `i18n/en/main.ftl`

**Interfaces:**
- Produces: state `query: String`, `selected: usize`, `search_id: cosmic::widget::Id`. Messages `Query(String)`, `SearchKey(Key)` where `enum Key { Up, Down, Enter, Escape }`.
- `app_list::results_view<'a>(apps: &'a [App], hits: &[usize], selected: usize, query: &str) -> Element<'a, Message>`

- [ ] **Step 1: Add the search box and focus it on open**

Put a `text_input(fl!("search-placeholder"), &self.query).id(self.search_id.clone()).on_input(Message::Query).on_submit(|_| Message::SearchKey(Key::Enter))` at the top of the middle column. When opening the popup, clear `query`, set `selected = 0`, and batch in `cosmic::widget::text_input::focus(self.search_id.clone())` so typing works straight away, as in Windows.

- [ ] **Step 2: Swap the view while searching**

While `!self.query.trim().is_empty()`, the middle and right columns are replaced by one `results_view` column, full width minus the rail:

```rust
pub fn results_view<'a>(apps: &'a [App], hits: &[usize], selected: usize, query: &str) -> Element<'a, Message> {
    if hits.is_empty() {
        return text(fl!("no-results", query = query.trim())).into();
    }
    let rows = hits.iter().enumerate().map(|(n, &i)| app_row(&apps[i], i, n == selected)).collect::<Vec<_>>();
    scrollable(column::with_children(rows).spacing(2)).height(Length::Fill).into()
}
```

Compute `hits` in `view_window` with `crate::search::rank(&self.apps, &self.query)`. It is cheap (a few hundred string comparisons).

- [ ] **Step 3: Keys**

Subscribe to key presses only while the popup is open: `cosmic::iced::keyboard::on_key_press` in `subscription()`, mapping `Named::ArrowUp/ArrowDown/Escape` to `SearchKey`. In `update`:

```rust
Message::Query(q) => { self.query = q; self.selected = 0; Task::none() }
Message::SearchKey(Key::Down) => { self.selected = self.selected.saturating_add(1); Task::none() }
Message::SearchKey(Key::Up) => { self.selected = self.selected.saturating_sub(1); Task::none() }
Message::SearchKey(Key::Enter) => {
    let hits = crate::search::rank(&self.apps, &self.query);
    match hits.get(self.selected.min(hits.len().saturating_sub(1))) {
        Some(&i) => self.update(Message::Launch(i)),
        None => Task::none(),
    }
}
Message::SearchKey(Key::Escape) => {
    if self.query.is_empty() { self.close_popup() } else { self.query.clear(); self.selected = 0; Task::none() }
}
```

When drawing, clamp `selected` to `hits.len() - 1` (as in the `Enter` arm) so the highlight never falls off the end.

- [ ] **Step 4: Check by hand**

Open the menu and type `ter` without clicking first. Results appear, with Terminal first. Press ↓ then Enter to launch the second hit. Esc once clears the search. Esc a second time closes the menu. Type `zzzz` and you see the "No apps match" message.

- [ ] **Step 5: Commit** — "Search apps by typing, with arrow keys and Enter" (with trailers).

---

### Task 11: Right-click menu — pin, unpin, resize, move, app actions

**Files:**
- Modify: `src/app.rs`, `src/ui/app_list.rs`, `src/ui/tiles.rs`, `i18n/en/main.ftl`

**Interfaces:**
- Produces: state `context: Option<Context>` where
  ```rust
  pub enum Target { App(usize /* apps index */), Tile((usize, usize) /* TileRef */) }
  pub struct Context { pub target: Target, pub at: cosmic::iced::Point }
  ```
  Messages: `Pointer(Point)`, `OpenContext(Target)`, `CloseContext`, `Pin(String)`, `Unpin(String)`, `Resize(TileRef, TileSize)`, `MoveToGroup(TileRef, usize)`, `NewGroupWith(TileRef)`, `RunAction(usize, usize)` (apps index, action index).

- [ ] **Step 1: Strings**

```ftl
ctx-pin = Pin to Start
ctx-unpin = Unpin from Start
ctx-resize = Resize
size-small = Small
size-medium = Medium
size-wide = Wide
ctx-move = Move to group
ctx-new-group = New group
new-group-name = New group
```

- [ ] **Step 2: Track the pointer, and open the menu on right-press**

Wrap the whole popup body in `mouse_area(...).on_move(Message::Pointer)` and keep the last `Point` in `self.pointer`. Wrap each `app_row` in `mouse_area(row).on_right_press(Message::OpenContext(Target::App(i)))` and each tile in `mouse_area(tile).on_right_press(Message::OpenContext(Target::Tile((g, ti))))`. `ti` is the **config** index from `visible_tiles`, not the visible position. `OpenContext` stores `Context { target, at: self.pointer }`.

- [ ] **Step 3: Draw the menu**

Wrap the popup body in `cosmic::widget::popover(body).on_close(Message::CloseContext)`. When `self.context` is `Some`, add `.popup(menu).position(popover::Position::Point(ctx.at))`. `menu` is a `container(column).class(cosmic::theme::Container::Dropdown)` of `quiet_button` rows:
- `Target::App(i)`: Pin or Unpin (by `config.is_pinned`), then one row per `apps[i].actions[k]` → `RunAction(i, k)`.
- `Target::Tile(r)`: Unpin, Resize → three rows (Small/Medium/Wide, current one marked with `object-select-symbolic`), Move to group → one row per other group + "New group".

- [ ] **Step 4: Handle the messages**

Each config change is a `Config` method from Task 4, followed by a save that runs off-thread:

```rust
fn edit(&mut self, f: impl FnOnce(&mut Config)) -> Task<Message> {
    f(&mut self.config);
    self.context = None;
    let snapshot = self.config.clone();
    Task::perform(
        async move { tokio::task::spawn_blocking(move || snapshot.save()).await.unwrap_or_else(|e| Err(e.to_string())) },
        |r| match r { Ok(()) => cosmic::Action::None, Err(e) => cosmic::Action::App(Message::PowerFailed(e)) },
    )
}
```

Rename `PowerFailed` to `ShowError` now, since it covers more than power, and update Task 8's call sites. Map the messages: `Pin(id)` → `edit(|c| c.pin(&id))`; `Unpin` → `unpin`; `Resize(r, s)` → `resize`; `MoveToGroup(r, g)` → `move_tile(r, g, usize::MAX)`; `NewGroupWith(r)` → `let g = c.add_group(fl!("new-group-name")); c.move_tile(r, g, 0)`; `RunAction(i, k)` → close the popup and `launch::action(app.id, app.actions[k].clone(), app.terminal)`.

- [ ] **Step 5: Check by hand**

- Right-click an unpinned app, choose Pin to Start, and a Medium tile appears in the first group.
- Right-click that tile and resize it to Wide, then Small. The layout re-packs.
- Move it to New group. A "New group" heading appears.
- Right-click Firefox (or Chromium) → "New Private/Incognito Window" opens one.
- Close and reopen the menu. Everything is kept, and `config.toml` shows the change.

- [ ] **Step 6: Commit** — "Add right-click pin, resize, move and app actions" (with trailers).

---

### Task 12: Edit mode — drag tiles, and rename or delete groups

**Files:**
- Modify: `src/app.rs`, `src/ui/tiles.rs`, `src/config.rs` (only if a helper is missing), `i18n/en/main.ftl`

**Interfaces:**
- Produces: state `editing: bool`, `drag: Option<TileRef>`, `renaming: Option<(usize, String)>`. Messages: `ToggleEdit`, `DragStart(TileRef)`, `DropAt(usize /* group */, usize /* index */)`, `DragCancel`, `RenameStart(usize)`, `RenameInput(String)`, `RenameCommit`, `RemoveGroup(usize)`.

In-window reordering uses `mouse_area`, **not** libcosmic `dnd_*`. `dnd_*` is cross-app Wayland DnD and is the wrong tool here (see the libcosmic Grid/DnD notes).

- [ ] **Step 1: Strings** — `edit-tiles = Edit`, `edit-done = Done`, `group-rename = Rename`, `group-remove = Remove group`.

- [ ] **Step 2: An edit toggle at the top of the tile column**

A small text button reading `edit-tiles`, or `edit-done` while editing, sends `ToggleEdit`. Leaving edit mode clears `drag` and `renaming`.

- [ ] **Step 3: Drag by press-and-release**

While `editing`:
- Each tile is `mouse_area(tile).on_press(DragStart(r))`, and tiles do not launch.
- The dragged tile is drawn at 50% alpha.
- Every tile gets `on_release(DropAt(g, config_index))`.
- Each group's heading row gets `on_release(DropAt(g, usize::MAX))`, which drops at the end.
- Releasing anywhere else sends `DragCancel`, via an outer `mouse_area(...).on_release`.

`DropAt(g, i)` → `edit(|c| c.move_tile(from, g, i))` and clears `drag`.

- [ ] **Step 4: Rename and remove**

While editing, the heading becomes `row![text_input(name).on_input(RenameInput).on_submit(|_| RenameCommit), button(icon "user-trash-symbolic").on_press(RemoveGroup(g))]`. `RenameCommit` → `edit(|c| c.rename_group(g, name))`. `RemoveGroup(g)` → `edit(|c| c.remove_group(g))`; its tiles move to the neighbouring group, as tested in Task 4.

- [ ] **Step 5: Check by hand**

Enter Edit and drag a tile onto another tile: it takes that position. Drag a tile to another group's heading: it goes to the end of that group. Rename a group. Remove a group: its tiles join the group above. Press Done. Clicking a tile launches it again.

- [ ] **Step 6: Commit** — "Add edit mode for dragging tiles and managing groups" (with trailers).

---

### Task 13: Letter-jump grid

**Files:**
- Modify: `src/ui/app_list.rs`, `src/app.rs`

**Interfaces:**
- Produces: state `letter_grid: bool`, `list_id: cosmic::widget::Id` (a `scrollable` id). Messages: `LetterGrid(bool)` (already stubbed), `JumpTo(char)`.
- `pub fn letter_grid<'a>(present: &[char]) -> Element<'a, Message>`
- `pub fn offset_of(sections: &[(char, Vec<usize>)], most_used_rows: usize, letter: char, row_h: f32, header_h: f32) -> f32` (pure, unit-tested)

- [ ] **Step 1: Write the failing test for `offset_of`**

```rust
#[test]
fn offset_counts_headers_and_rows_before_the_letter() {
    let s = vec![('#', vec![0]), ('A', vec![1, 2]), ('B', vec![3])];
    // Most used: heading + 2 rows. Then '#': header+1 row, 'A': header+2 rows.
    assert_eq!(offset_of(&s, 2, 'B', 10.0, 20.0), 20.0 + 2.0 * 10.0 + 20.0 + 10.0 + 20.0 + 2.0 * 10.0);
    assert_eq!(offset_of(&s, 0, '#', 10.0, 20.0), 0.0);
}
```

- [ ] **Step 2: Implement `offset_of`**

```rust
pub fn offset_of(sections: &[(char, Vec<usize>)], most_used_rows: usize, letter: char, row_h: f32, header_h: f32) -> f32 {
    let mut y = if most_used_rows > 0 { header_h + most_used_rows as f32 * row_h } else { 0.0 };
    for (c, rows) in sections {
        if *c == letter { return y; }
        y += header_h + rows.len() as f32 * row_h;
    }
    y
}
```

Measure `row_h` and `header_h` from the actual rows. Set a fixed height on `app_row` and `letter_header` (`Length::Fixed`) derived from `Spacing` and `ICON`, so the offset is exact rather than estimated.

- [ ] **Step 3: The grid view**

When `letter_grid` is true, the middle column shows a 4-column wrap of `#, A…Z` plus any other letters present. Each button is enabled only if that letter has apps. Tapping one sends `JumpTo(c)` → `letter_grid = false` plus `scrollable::scroll_to(list_id, AbsoluteOffset { x: 0.0, y: offset_of(...) })`.

- [ ] **Step 4: Run the test and check by hand** — `cargo test app_list::` passes. Tapping a letter header opens the grid; tapping `S` lands on the S apps.

- [ ] **Step 5: Commit** — "Jump to a letter from the A to Z headers" (with trailers).

---

### Task 14: Settings window

**Files:**
- Create: `src/settings.rs`, `data/io.github.jjnuthuagen.StartMenuSettings.desktop`, `src/single_instance.rs` (copied from CCCA)
- Modify: `src/main.rs`, `src/app.rs`, `i18n/en/main.ftl`

**Interfaces:**
- Produces: `settings::Settings: cosmic::Application` (APP_ID `io.github.jjnuthuagen.StartMenuSettings`) and `settings::window_settings()`. The `--settings` flag is handled in `main.rs`. `Message::OpenSettings` on right-click of the panel button.

- [ ] **Step 1: Strings**

```ftl
settings-title = Start Menu settings
settings-finish = Tile finish
finish-frosted = Frosted
finish-solid = Solid
finish-outline = Outline
settings-most-used = Show most used apps
settings-reset = Reset pinned tiles
settings-reset-hint = Replaces your groups with the dock’s favourites.
```

- [ ] **Step 2: Build the window**

Use CCCA `src/settings.rs` as the pattern: a `cosmic::app::Application` with one `settings::section()` page. Copy only the parts needed:
- a dropdown for `finish` (three options)
- a toggler for `show_most_used`
- a destructive button, "Reset pinned tiles", that replaces `config.groups` with `Config::seeded(...)` after a confirm dialog

Every change calls `Config::save()`. Copy `single_instance.rs` and the `--settings` arm in `main.rs` from CCCA verbatim, changing only the app id.

- [ ] **Step 3: Right-click opens it**

In `App::view`, wrap the panel button in `mouse_area(button).on_right_press(Message::OpenSettings)`. Copy CCCA's `open_settings_window()`, which spawns `current_exe --settings` via `process::spawn_and_reap`. The applet already re-reads the config on every open (Task 8), so changes show up the next time the menu opens.

- [ ] **Step 4: Check by hand**

`just install` (the justfile already installs the Settings desktop file). Right-click the panel button and the window opens. A second right-click brings the same window forward instead of opening another. Change the finish and reopen the menu: the new finish shows. Turn off Most used: the section is gone. Reset: the tiles go back to the dock favourites.

- [ ] **Step 5: Commit** — "Add the Settings window" (with trailers).

---

### Task 15: README, and swap it in for the App Library button (run LAST, after Tasks 16–19)

**Files:**
- Modify: `README.md`, `CLAUDE.md` (project status), `context/` (a new context note)

- [ ] **Step 1: README**

Cover:
- what it is
- a screenshot (take one with `cosmic-screenshot`)
- build: `just install`
- adding it to the panel
- the `config.toml` format with an example
- the Appearance rules it follows
- "Live tiles: planned (the `source` field is reserved)"
- licence

- [ ] **Step 2: Full verification**

```bash
cargo fmt --all -- --check && cargo clippy --all-targets --locked -- -D warnings && cargo test --locked && cargo build --release --locked
```
Expected: all green.

- [ ] **Step 3: Replace the App Library button (with James)**

Settings → Desktop → Panel → Configure applets:
- remove **App Library** (`com.system76.CosmicPanelAppButton`)
- put **Start Menu** in its place, at the far left of the left wing

This is reversible: the App Library can be added back from the same screen. Then run through the spec §8 manual list once more on the real panel.

- [ ] **Step 4: Commit** — "Document the Start Menu and mark v0.1 ready" (with trailers). **Do not push.** Ask James before the repo goes to GitHub.

---

## Amendment tasks (from the mockup, spec §10)

### Task 16: New config options, the Accent finish, and recent launches

**Files:**
- Modify: `src/config.rs`, `src/usage.rs`, `src/launch.rs`

**Interfaces:**
- Produces:
  ```rust
  // config.rs
  #[serde(rename_all="lowercase")] pub enum ListMode { #[default] Az, Category, Folders }
  #[serde(rename_all="lowercase")] pub enum RightSide { #[default] Tiles, Favourites, Recent }
  pub enum TileFinish { Frosted, Solid, Outline, Accent }   // Accent added
  pub struct Config { …, #[serde(default)] pub list_mode: ListMode, #[serde(default)] pub right_side: RightSide }
  // usage.rs
  pub struct Usage { pub counts: BTreeMap<String,u32>, #[serde(default)] pub recent: Vec<String> }
  impl Usage { pub fn recent(&self, n: usize, installed: &HashSet<&str>) -> Vec<String> } // newest first
  pub const RECENT_CAP: usize = 16;
  ```
  `Usage::record` now also moves the id to the front of `recent` and trims it to `RECENT_CAP`.

- [ ] **Step 1: Write the failing tests**

```rust
// config.rs tests
#[test]
fn new_modes_default_and_round_trip() {
    let c: Config = toml::from_str("").unwrap();
    assert_eq!((c.list_mode, c.right_side), (ListMode::Az, RightSide::Tiles));
    let c: Config = toml::from_str("list_mode = \"folders\"\nright_side = \"recent\"\nfinish = \"accent\"").unwrap();
    assert_eq!((c.list_mode, c.right_side, c.finish), (ListMode::Folders, RightSide::Recent, TileFinish::Accent));
}

// usage.rs tests
#[test]
fn recent_is_newest_first_deduped_and_capped() {
    let mut u = Usage::default();
    for i in 0..20 { u.record(&format!("app{i}")); }
    u.record("app5");
    assert_eq!(u.recent.len(), RECENT_CAP);
    assert_eq!(u.recent[0], "app5");
    assert_eq!(u.recent.iter().filter(|a| *a == "app5").count(), 1);
    let inst: HashSet<&str> = ["app5", "app19"].into();
    assert_eq!(u.recent(8, &inst), ["app5", "app19"]);
}

#[test]
fn old_usage_files_without_recent_still_load() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("usage.toml");
    std::fs::write(&p, "[counts]\nfirefox = 3\n").unwrap();
    let u = Usage::load_from(&p);
    assert_eq!(u.counts["firefox"], 3);
    assert!(u.recent.is_empty());
}
```

- [ ] **Step 2: Run to verify they fail** — `cargo test config:: usage::`

- [ ] **Step 3: Implement**

Add the two enums with `#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]`, the `Accent` variant, and the two `#[serde(default)]` fields on `Config`. Then in `usage.rs`:

```rust
pub const RECENT_CAP: usize = 16;

pub fn record(&mut self, app: &str) {
    let n = self.counts.entry(app.to_owned()).or_insert(0);
    *n = n.saturating_add(1);
    self.recent.retain(|a| a != app);
    self.recent.insert(0, app.to_owned());
    self.recent.truncate(RECENT_CAP);
}

pub fn recent(&self, n: usize, installed: &HashSet<&str>) -> Vec<String> {
    self.recent.iter().filter(|a| installed.contains(a.as_str())).take(n).cloned().collect()
}
```

In `ui/mod.rs`'s `finish_paint`, `Accent` paints `theme.cosmic().accent_color()` as a solid fill. Its text and icon colour is `theme.cosmic().on_accent_color()`, set on the tile button's style, which is the one place the tile sets a text colour.

- [ ] **Step 4: Run to verify they pass** — `cargo test`

- [ ] **Step 5: Commit** — "Add list and right-side modes, the Accent finish, and recent launches" (with trailers).

---

### Task 17: `folders` — read the App Library's folders, and group apps by category

**Files:**
- Create: `src/folders.rs`
- Modify: `src/apps.rs` (add `categories: Vec<String>` to `App`, filled from `de.categories()`), `src/main.rs`, `Cargo.toml` (`ron = "0.12"`, already in the lock file via libcosmic)

**Interfaces:**
- Produces:
  ```rust
  // apps.rs
  pub fn category_of(app: &App) -> &'static str     // l10n key: "cat-games", "cat-graphics", … "cat-other"
  pub const CATEGORY_ORDER: &[&str]                  // keys in display order, "cat-other" last
  pub fn category_sections(apps: &[App]) -> Vec<(&'static str, Vec<usize>)>
  // folders.rs
  pub struct Folder { pub name: String, pub apps: Vec<usize> }     // indices into apps, sorted by name
  pub fn parse(ron: &str) -> Vec<RawFolder>                         // tolerant: bad file → empty
  pub fn resolve(raw: &[RawFolder], apps: &[App]) -> (Vec<Folder>, Vec<usize> /* apps in no folder */)
  pub fn load(apps: &[App]) -> (Vec<Folder>, Vec<usize>)            // reads ~/.config/cosmic/com.system76.CosmicAppLibrary/v1/groups
  ```

- [ ] **Step 1: Write the failing tests**

```rust
// folders.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::apps::App;

    const GROUPS: &str = r#"[
        (name: "Games", icon: "folder-symbolic", filter: AppIds(["openttd", "0ad", "not-installed"])),
        (name: "Utilities", icon: "folder-symbolic", filter: Categories(
            categories: ["Utility"], exclude: ["calc"], include: ["btop"])),
    ]"#;

    fn app(id: &str, name: &str, cats: &[&str]) -> App {
        App { id: id.into(), name: name.into(), categories: cats.iter().map(|s| s.to_string()).collect(), ..App::default() }
    }

    #[test]
    fn resolves_id_lists_and_category_filters() {
        let apps = vec![
            app("0ad", "0 A.D.", &["Game"]), app("btop", "btop++", &["System"]),
            app("calc", "Calculator", &["Utility"]), app("micro", "Micro", &["Utility"]),
            app("openttd", "OpenTTD", &["Game"]), app("vim", "Vim", &["TextEditor"]),
        ];
        let (folders, loose) = resolve(&parse(GROUPS), &apps);
        assert_eq!(folders[0].name, "Games");
        assert_eq!(folders[0].apps, [0, 4]);           // not-installed dropped
        assert_eq!(folders[1].apps, [1, 3]);           // Utility minus calc, plus btop
        assert_eq!(loose, [2, 5]);                     // calc and vim are in no folder
    }

    #[test]
    fn a_bad_file_means_no_folders() {
        assert!(parse("not ron at all").is_empty());
        assert!(parse("").is_empty());
    }
}

// apps.rs — add to its tests
#[test]
fn category_priority_and_other() {
    let mk = |c: &[&str]| App { categories: c.iter().map(|s| s.to_string()).collect(), ..App::default() };
    assert_eq!(category_of(&mk(&["Utility", "Development"])), "cat-development");
    assert_eq!(category_of(&mk(&["Graphics", "3DGraphics"])), "cat-graphics");
    assert_eq!(category_of(&mk(&["COSMIC"])), "cat-other");
}
```

- [ ] **Step 2: Run to verify they fail** — `cargo test folders:: apps::`

- [ ] **Step 3: Implement `folders.rs`**

```rust
//! The folders James already made in COSMIC's App Library, reused as the
//! Start menu's "Folders" view. Read-only: the App Library owns this file.

use serde::Deserialize;
use crate::apps::App;

#[derive(Debug, Clone, Deserialize)]
pub struct RawFolder {
    pub name: String,
    #[serde(default)]
    pub filter: Filter,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub enum Filter {
    AppIds(Vec<String>),
    Categories {
        #[serde(default)] categories: Vec<String>,
        #[serde(default)] exclude: Vec<String>,
        #[serde(default)] include: Vec<String>,
    },
    #[default]
    #[serde(other)]
    Unknown,
}

pub struct Folder { pub name: String, pub apps: Vec<usize> }

pub fn parse(ron_text: &str) -> Vec<RawFolder> {
    ron::from_str(ron_text).unwrap_or_else(|e| {
        tracing::debug!("App Library folders unreadable: {e}");
        Vec::new()
    })
}

fn matches(filter: &Filter, app: &App) -> bool {
    match filter {
        Filter::AppIds(ids) => ids.iter().any(|id| *id == app.id),
        Filter::Categories { categories, exclude, include } => {
            include.contains(&app.id)
                || (!exclude.contains(&app.id) && app.categories.iter().any(|c| categories.contains(c)))
        }
        Filter::Unknown => false,
    }
}

pub fn resolve(raw: &[RawFolder], apps: &[App]) -> (Vec<Folder>, Vec<usize>) {
    let mut used = vec![false; apps.len()];
    let folders = raw.iter().map(|f| {
        let idx: Vec<usize> = (0..apps.len()).filter(|&i| matches(&f.filter, &apps[i])).collect();
        for &i in &idx { used[i] = true; }
        Folder { name: f.name.clone(), apps: idx }  // `apps` is already A–Z, so this is sorted
    }).collect();
    let loose = (0..apps.len()).filter(|&i| !used[i]).collect();
    (folders, loose)
}

pub fn load(apps: &[App]) -> (Vec<Folder>, Vec<usize>) {
    let text = dirs::config_dir()
        .map(|d| d.join("cosmic/com.system76.CosmicAppLibrary/v1/groups"))
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_default();
    resolve(&parse(&text), apps)
}
```

If `#[serde(other)]` on a unit variant is rejected alongside the data variants by `ron`, drop the `Unknown` variant. A file with an unknown filter kind then parses to an empty list through the `unwrap_or_else`, which is acceptable.

- [ ] **Step 4: Implement the categories in `apps.rs`**

```rust
const MAIN: &[(&str, &str)] = &[
    ("Game", "cat-games"), ("Graphics", "cat-graphics"), ("AudioVideo", "cat-audio-video"),
    ("Development", "cat-development"), ("Network", "cat-internet"), ("Office", "cat-office"),
    ("Education", "cat-education"), ("Science", "cat-science"), ("Settings", "cat-settings"),
    ("System", "cat-system"), ("Utility", "cat-utilities"),
];
pub const CATEGORY_ORDER: &[&str] = &["cat-games", "cat-graphics", "cat-audio-video", "cat-development",
    "cat-internet", "cat-office", "cat-education", "cat-science", "cat-settings", "cat-system",
    "cat-utilities", "cat-other"];

pub fn category_of(app: &App) -> &'static str {
    MAIN.iter().find(|(k, _)| app.categories.iter().any(|c| c == k)).map_or("cat-other", |(_, v)| v)
}

pub fn category_sections(apps: &[App]) -> Vec<(&'static str, Vec<usize>)> {
    CATEGORY_ORDER.iter().filter_map(|&key| {
        let idx: Vec<usize> = (0..apps.len()).filter(|&i| category_of(&apps[i]) == key).collect();
        (!idx.is_empty()).then_some((key, idx))
    }).collect()
}
```

Add to `main.ftl`:
```ftl
cat-games = Games
cat-graphics = Graphics & 3D
cat-audio-video = Sound & Video
cat-development = Development
cat-internet = Internet
cat-office = Office
cat-education = Education
cat-science = Science
cat-settings = Settings
cat-system = System
cat-utilities = Utilities
cat-other = Other
```

- [ ] **Step 5: Run to verify they pass** — `cargo test`

- [ ] **Step 6: Commit** — "Read App Library folders and group apps by category" (with trailers).

---

### Task 18: List modes in the popup

**Files:**
- Modify: `src/ui/app_list.rs`, `src/app.rs`, `i18n/en/main.ftl`

**Interfaces:**
- Consumes: `config::ListMode`, `apps::category_sections`, `folders::{load, Folder}`
- Produces: state `folders: Vec<Folder>`, `loose: Vec<usize>`, `open_folders: HashSet<usize>`, `mode_menu: bool`. Messages: `ModeMenu(bool)`, `SetListMode(ListMode)`, `ToggleFolder(usize)`, `JumpToCategory(&'static str)`.

- [ ] **Step 1: Strings** — `all-apps = All apps`, `mode-az = A–Z`, `mode-category = Category`, `mode-folders = Folders`, `folders = Folders`.

- [ ] **Step 2: Load folders with the apps**

Inside the `spawn_blocking` in `TogglePopup` (Task 8), also compute `folders::load(&apps)` and carry `(folders, loose)` in `AppsLoaded`.

- [ ] **Step 3: The list bar**

Between the search box and the list, add a row: `text::caption(fl!("all-apps"))` on the left and a `popover` button on the right labelled with the current mode plus a chevron. It opens a menu of the three modes, with a check on the current one. `SetListMode(m)` → `edit(|c| c.list_mode = m)` (the saving helper from Task 11), closes the menu, and scrolls the list to the top. Hide the bar while searching.

- [ ] **Step 4: Draw each mode** (the mockup's `renderList` is the reference)

- `Az`: unchanged.
- `Category`: for each `(key, idx)` in `category_sections`, a heading button `fl!(key)`, then the rows. The heading opens the jump grid, which now shows a two-column grid of category names (`JumpToCategory`). Reuse `offset_of` from Task 13 with category keys in place of letters. Generalise its `char` parameter to a `&str` label, and update Task 13's test to match.
- `Folders`: a "Folders" section header. Then, for each folder, a row showing a folder icon on a `radius_s` tinted base, the name, the app count and a chevron (rotated when open) → `ToggleFolder(i)`. An open folder's apps are indented under it, with a divider line on the left. After the folders comes the A–Z block built from `loose` only; letter headers and jumps work on that subset. If no folders are found (no file, or a bad one), Folders mode shows the plain A–Z list.

- [ ] **Step 5: Check by hand** — switch between the three modes from the list bar. Category headings jump correctly. Folders shows Design, Games and Utilities with the right apps, matching the App Library. Close and reopen the menu: the mode is kept.

- [ ] **Step 6: Commit** — "Organise the app list by category or App Library folder" (with trailers).

---

### Task 19: Right side — Favourites and Recent grids; the avatar follows roundness

**Files:**
- Create: `src/favorites.rs`
- Modify: `src/ui/tiles.rs`, `src/ui/rail.rs`, `src/app.rs`, `src/main.rs`, `i18n/en/main.ftl`

**Interfaces:**
- Produces:
  ```rust
  // favorites.rs — the dock's favourites, via cosmic-config (the same store the dock watches)
  pub fn read() -> Vec<String>
  pub fn with_added(list: &[String], id: &str) -> Vec<String>     // pure; appends if absent
  pub fn with_removed(list: &[String], id: &str) -> Vec<String>   // pure
  pub fn write(list: &[String]) -> Result<(), String>
  ```
  State: `favs: Vec<String>`, `recent: Vec<String>`, `right_menu: bool`. Messages: `RightMenu(bool)`, `SetRightSide(RightSide)`, `AddFavourite(String)`, `RemoveFavourite(String)`.

- [ ] **Step 1: Write the failing tests** (`favorites.rs`)

```rust
#[test]
fn add_and_remove_are_idempotent_and_keep_order() {
    let l = vec!["a".to_string(), "b".to_string()];
    assert_eq!(with_added(&l, "c"), ["a", "b", "c"]);
    assert_eq!(with_added(&l, "a"), ["a", "b"]);
    assert_eq!(with_removed(&l, "a"), ["b"]);
    assert_eq!(with_removed(&l, "zzz"), ["a", "b"]);
}
```

- [ ] **Step 2: Implement**

```rust
//! The dock's favourites. Read and written through cosmic-config — the same
//! store the dock watches — so adding one here shows up in the dock at once.

use cosmic::cosmic_config::{Config, ConfigGet, ConfigSet};

const ID: &str = "com.system76.CosmicAppList";

pub fn read() -> Vec<String> {
    Config::new(ID, 1).ok().and_then(|c| c.get::<Vec<String>>("favorites").ok()).unwrap_or_default()
}

pub fn with_added(list: &[String], id: &str) -> Vec<String> {
    let mut v = list.to_vec();
    if !v.iter().any(|x| x == id) { v.push(id.to_owned()); }
    v
}

pub fn with_removed(list: &[String], id: &str) -> Vec<String> {
    list.iter().filter(|x| *x != id).cloned().collect()
}

pub fn write(list: &[String]) -> Result<(), String> {
    Config::new(ID, 1).map_err(|e| e.to_string())?.set("favorites", list.to_vec()).map_err(|e| e.to_string())
}
```

`config::parse_favorites` from Task 4 is now redundant for reading. Switch `favorites_file()` in `config.rs` to `favorites::read()`, keep `parse_favorites` and its test for `Config::seeded`, and delete neither.

Manual check of the write path: add an app to favourites from the menu and confirm it appears in the dock straight away. Then remove it again. This is reversible.

- [ ] **Step 3: Draw the grids** (`ui/tiles.rs`)

- In the right column's header row, on the left, add a `popover` button labelled with the current mode ("Tiles", "Favourites" or "Recent") plus a chevron. It opens a three-item menu → `SetRightSide`, saved through `edit`. The Edit button stays on the right and is shown only in Tiles mode; leaving Tiles also leaves edit mode.
- In Favourites and Recent modes, draw a four-column grid of `quiet_button` cells, each a 40 px icon over an elided caption name. Build it from `row`s of four, not `Grid`. Clicking a cell launches the app, and right-clicking gives the app context menu. Below the grid, add a caption: "Same list as your dock." for Favourites, or "Apps you opened recently." for Recent. Show at most 16 cells.
- Recent comes from `Usage::recent(16, &installed)`, loaded with the Most used list in the popup-open `spawn_blocking`.

- [ ] **Step 4: Context menu entries**

In the `Target::App` menu, after Pin/Unpin, add "Add to favourites" or "Remove from favourites" (based on `self.favs`). Run `favorites::write(&with_added(..))` off-thread, then update `self.favs`.

- [ ] **Step 5: The avatar follows roundness** (`ui/rail.rs`)

Replace the `avatar-default-symbolic` icon with a 28 px avatar container: the user's AccountsService `IconFile` image if present, otherwise their initial on an accent fill. Its radius is:

```rust
fn avatar_radius(theme: &cosmic::Theme, size: f32) -> f32 {
    let r = theme.cosmic().corner_radii;
    if r.radius_xl[0] >= size / 2.0 { size / 2.0 } else { r.radius_s[0] }  // Round → circle; else radius_s
}
```

Round gives a circle, Slightly round an 8 px square, and Square a 2 px square. Read `IconFile` from `org.freedesktop.Accounts` (`FindUserByName($USER)` → `org.freedesktop.Accounts.User.IconFile`) in the popup-open task. On error, fall back to the initial.

Strings: `right-tiles = Tiles`, `right-favourites = Favourites`, `right-recent = Recent`, `fav-add = Add to favourites`, `fav-remove = Remove from favourites`, `fav-note = Same list as your dock.`, `recent-note = Apps you opened recently.`

- [ ] **Step 6: Check by hand** — switch the right side through all three modes. Favourites matches the dock exactly. Adding a favourite from the menu shows up in the dock. Opening an app puts it first in Recent. Cycle Roundness: the avatar goes circle → rounded square → square.

- [ ] **Step 7: Commit** — "Offer favourites and recent grids on the right, and shape the avatar by roundness" (with trailers).
