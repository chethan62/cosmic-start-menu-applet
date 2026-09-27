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
- 2026-09-27: design-review round: avatar decodes (WebP sniffed), frost blur clipped to popup corners, search-highlight spacing, Settings entry pinned under the app list (right-click removed), lock mode, tile colour/picture/name-toggle, default-app rail shortcuts picked in Settings.

## Layout
- `context/` brief, decisions, the why  |  `references/` inputs, links  |  `working/` WIP
- `output/` deliverables  |  `assets/` media  |  `archive/` superseded versions

## Links
- 
