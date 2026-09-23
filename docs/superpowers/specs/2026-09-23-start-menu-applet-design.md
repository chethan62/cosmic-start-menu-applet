# Start Menu applet — design spec

*2026-09-23 · status: draft for review*

## 1. Goal

A COSMIC panel applet that opens a **Windows 10–style Start menu** and becomes
James's main way to open apps, **replacing the App Library panel button**. It
follows COSMIC's Appearance settings exactly as the Control Center applet
(CCCA-40) does: nothing about its look is hard-coded.

**Success looks like**
- James removes the App Library button from his panel and doesn't miss it.
- Changing light/dark, accent, roundness or frosted glass in COSMIC Settings
  restyles the menu live, with no restart.
- Someone else can clone, `cargo build` and run it without help (public repo).

**Said by James:** replace App Library; A–Z list, pinned tile grid,
type-to-search, left power/user rail; live tiles later, not v1; public repo
like Control Center; tiles seeded with defaults.
**Assumed (correct me):** popup attached to the panel button, not a full-height
window; the name is "Start Menu" (`cosmic-start-menu-applet`).

## 2. Approach

A `cosmic-applet` popup built on the same foundations as Control Center:
the same pinned libcosmic revision, Wayland-path blur request, Fluent i18n,
`config.toml` + separate Settings window, and tile-grid packer
(`tile_layout.rs`). The shared parts are copied in and adapted, not linked.
Both repos are MIT/Apache, so there is no licence problem.

Rejected: a standalone layer-shell window. It would have more room, but it
would re-solve theming and blur for little gain.

## 3. Layout

```
┌──┬──────────────────┬───────────────────────────────┐
│👤│ Most used         │  Productivity         [edit]  │
│  │  Firefox          │  ┌────┐┌────┐┌─────────┐      │
│  │  Files            │  │ M  ││ M  ││  Wide   │      │
│  │ A                 │  └────┘└────┘└─────────┘      │
│  │  Alacritty        │  ┌──┐┌──┐                     │
│📁│  Audacity         │  └──┘└──┘ (small)             │
│⚙ │ B                 │  Tools                        │
│⏻ │  Bitwarden   …    │  …                            │
└──┴──────────────────┴───────────────────────────────┘
```

- **Left rail** (icons, labels on hover). From the top: user avatar (tap opens
  COSMIC Settings → Users), then pinned places at the bottom: Files, Settings,
  Power. Power opens a small menu: Lock, Log out, Suspend, Restart, Shut down.
- **App list**: an optional "Most used" section (top 5), then every app A–Z
  under letter headers. Tapping a letter header shows a letter grid; tapping a
  letter jumps to it (Win10 behaviour).
- **Tile grid**: named groups of pinned apps. Tile sizes are Small (1×1),
  Medium (2×2) and Wide (4×2) on a 1×1 base cell. Groups stack vertically and
  the grid scrolls.
- **Search**: typing while the menu is open replaces the list and grid with
  ranked results (prefix > word-start > substring > keyword/generic-name
  match). Enter launches the top hit; arrow keys move the selection. Esc clears
  the search, and a second Esc closes the menu.
- **Context menu** (right-click an app or tile): Pin/Unpin, Resize (tile only),
  Move to group…, Open (and desktop-file Actions, e.g. "New private window").
- **Edit mode**: drag tiles within and between groups; rename or delete groups.
  Drag uses `mouse_area` in-window reordering, not `dnd_*` (per the libcosmic
  DnD gotcha memory).

## 4. Appearance rules

- Every colour, radius, spacing and font comes from the active COSMIC theme.
  Accent drives the selection highlight and the default tile fill.
- Roundness follows the desktop's radius tokens (as in CCCA-40 commit f098a7c).
- Frosted glass: when COSMIC frost is on, the popup requests blur down the
  Wayland path (as in CCCA-40 commit ac8b4a0). Tile finish is a setting with
  the same three options as Control Center: frosted (default), solid, outline.
- Light/dark and theme changes apply live through the theme subscription.
- Avoid known traps: don't use `Container::Primary` under frost (it reads as a
  hole); draw the grid from the packer, not `Grid`.

## 5. Components

| Unit | Does | Depends on |
|---|---|---|
| `apps` | Loads installed apps (libcosmic `desktop` feature), watches app dirs, re-indexes on change | freedesktop-desktop-entry |
| `search` | Pure ranking function: query + app index → ordered results | nothing (unit-tested) |
| `usage` | Counts launches for "Most used"; small local state file | config dir |
| `pins` | Groups, tiles, sizes, order; read/write `config.toml`; seeds defaults on first run | serde/toml |
| `tile_layout` | Packs tiles of mixed sizes into rows (adapted from CCCA-40) | nothing (unit-tested) |
| `session` | Lock/log out/suspend/restart/shut down over D-Bus (logind; COSMIC session for log out) | zbus |
| `launch` | Starts an app or desktop action via libcosmic `spawn_desktop_exec`, in a systemd scope | libcosmic |
| `ui` | Rail, list, grid, search, context menu, edit mode | all above |
| `settings` | Settings window: tile finish, show Most used, columns, rail items | config |

## 6. Data and config

- **Seeded pins** on first run: one group, "Pinned", built from the dock's
  favourites (`com.system76.CosmicAppList` → `favorites`). If that is empty,
  fall back to browser, Files, terminal and Settings, using whichever are
  installed.
- `config.toml`, written by the app and editable by hand:
  ```toml
  [[group]]
  name = "Pinned"
  tiles = [ { app = "firefox.desktop", size = "medium" }, … ]
  ```
- **Live-tile hook:** every tile carries an optional `source = …` field.
  v1 ignores it and draws the icon. v2 can add sources without changing the
  config shape or the layout.

## 7. Failure behaviour

- An app that disappears → its tile is hidden, but kept in config so it
  returns if the app is reinstalled.
- A broken `.desktop` file → skipped and logged; never crashes.
- A session D-Bus call is refused → show an inline error on the Power menu.
- A corrupt `config.toml` → rename it to `.bak`, start with seeded defaults,
  and log it.
- All I/O runs async; nothing blocks the UI loop.

## 8. Testing

- Unit: search ranking, A–Z bucketing (incl. non-Latin / lowercase names),
  tile packer, config round-trip + seeding, corrupt-config recovery.
- Manual (the real test): on James's panel — theme switches live, frost on/off,
  search → Enter launches, pin/resize/move, every power action.
- CI: `cargo build`, `cargo test`, `cargo clippy` (as in CCCA-40).

## 9. Out of scope for v1

Live tiles; recent files and "jump lists"; web search in the search box;
full-screen Start mode; replacing the Super-key launcher. v1 is a panel button
only. Super-key binding is a v2 candidate.
