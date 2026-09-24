# Start Menu

A Windows 10-style Start menu for the [COSMIC](https://system76.com/cosmic)
panel. It is styled entirely by COSMIC's own Appearance settings.

**Status: alpha.**

## What's in it

- **Rail:** your account picture, Files, Settings and Power (lock, log out,
  sleep, restart, shut down). Log out, restart and shut down use COSMIC's own
  confirmation dialog.
- **App list:** Most used, then every app. The list can be arranged
  **A–Z**, by **category**, or by the **folders** you made in COSMIC's App
  Library. Tap a heading to jump.
- **Right side:** pinned **tiles** in named groups (small, medium or wide),
  a plain grid of your dock **favourites**, or your **recent** apps.
- **Search:** start typing. ↑ ↓ pick a result, Enter opens it, and Esc clears
  the search, then closes the menu.
- **Right-click** an app or tile to pin it, resize it, move it, add it to
  favourites, or run one of the app's own actions (such as "New private
  window").
- **Edit:** tap a tile to pick it up and tap where it should go. You can also
  rename, add and remove groups.

## Follows your Appearance settings

Light and dark mode, the accent colour, roundness (Round / Slightly round /
Square) and frosted glass all come from COSMIC. Nothing is hard-coded. The
tile finish (frosted, solid, outline or accent) is the menu's own setting.

## Build and install

```sh
just install
```

Then add **Start Menu** in Settings → Desktop → Panel → Configure applets.
Right-click the panel button for its Settings window.

## Configuration

`~/.config/cosmic-start-menu-applet/config.toml` is written by the menu and
its Settings window, and is safe to edit by hand:

```toml
finish = "frosted"          # frosted | solid | outline | accent
show_most_used = true
list_mode = "az"            # az | category | folders
right_side = "tiles"        # tiles | favourites | recent

[[group]]
name = "Pinned"

[[group.tiles]]
app = "firefox"             # desktop-entry id, without .desktop
size = "medium"             # small | medium | wide
```

On first run the tiles are seeded from your dock's favourites. Live tiles are
planned: each tile's optional `source` field is reserved for them.

## Licence

MIT or Apache-2.0, at your option.
