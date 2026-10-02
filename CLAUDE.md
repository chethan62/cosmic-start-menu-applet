---
code: CSTM-49
name: cosmic-start-menu-applet
type: code
tags: [cosmic,applet,rust,public,active]
git: local
created: 2026-09-23
---

# cosmic-start-menu-applet

## Purpose
COSMIC panel applet giving a Windows 10-style Start menu (A-Z app list, pinned tile groups, type-to-search, power/user rail) that follows COSMIC Appearance settings. Sibling of CCCA-40.

## Status
- 2026-09-23: created with standard structure.
- 2026-09-26: v0.1 built and in the panel. Super is bound to `--toggle` (COSMIC custom shortcut); tiles 2 or 3 across in Settings.
- 2026-10-01: the glass came back, and `just install` now reloads the panel button. The frosted card and its blur region went out with the panel popup in the "one menu" change, so the frame was opaque while every tile inside it was already glass; the card takes `background(theme.transparent)` again and the menu asks the compositor to blur behind its layer surface (libcosmic only blurs surfaces it tracks itself). That request has to wait until the surface has focus: the menu is this process's first surface, so a request batched with its creation ran before the Wayland platform was up and went nowhere — translucent card, nothing blurred behind it. The dead Start button was not a bug at all: cosmic-panel execs an applet once per session and never respawns it, so the panel was still running the build from before the fix — `just install` ends in `just restart`, which cycles the applet out of `plugins_wings` and back.
- 2026-09-30: one menu, one door. The panel button no longer draws its own popup — both it and the Super shortcut go through `--toggle`, so a single layer-surface process is the only menu. It closes when the keyboard leaves it (a layer surface reports that as a Wayland event, not a window one), and a guard stops the click that closed it reopening it.
- 2026-09-27: design-review round: avatar decodes (WebP sniffed), frost blur clipped to popup corners, search-highlight spacing, Settings entry pinned under the app list (right-click removed), lock mode, tile colour/picture/name-toggle, default-app rail shortcuts picked in Settings.
- 2026-09-29: blind-critic round 2 vs real Windows 10 shots: one "Most used" block (5 rows, de-duplicated against the letter sections), shorter/muted letter headers, A-Z as the default sort, fixed 24px icon column, coloured-tile hover/press/focus states, 4px scroll bars flush to their column, and the rail rebuilt as part of the frame (no tinted strip, no chip behind the avatar, bottom-pinned Settings/Power).

## Layout
- `context/` brief, decisions, the why  |  `references/` inputs, links  |  `working/` WIP
- `output/` deliverables  |  `assets/` media  |  `archive/` superseded versions

## Links
- 
