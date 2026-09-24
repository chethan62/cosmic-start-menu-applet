# SDD ledger — plan: docs/superpowers/plans/2026-09-23-start-menu-applet.md
Spec: docs/superpowers/specs/2026-09-23-start-menu-applet-design.md (§10 amendments win). Order: 1-14, 16-19, 15.
Pre-flight:
- T7 consumes fl! with runtime keys (Power::l10n_key) vs T1 i18n copied from CCCA: check macro form; expose i18n::get if needed.
- T8 PowerFailed vs T11 renames to ShowError: rename at T11 as planned.
- T13 offset_of(char) vs T18 category labels (&str): generalise at T18 as planned.
- T4 favorites_file (parse RON file) vs T19 favorites::read via cosmic-config: switch at T19 as planned.
- T16 adds TileFinish::Accent consumed by T8 ui::finish_paint: add match arm at T16.
- T8 LetterGrid message stub consumed by T13: stub no-op at T8.
- T17 adds App.categories consumed by T2 tests' App literals (..App::default()) — compatible.
Task 1: Ruling: #[allow(dead_code)] on mod process until Task 7 uses it — clippy -D warnings would fail otherwise — cost if wrong: none, removed in T7
Task 1: Ruling: added Start Menu to James's panel left wing (backup plugins_wings.bak-startmenu), App Library kept until Task 15 — needed for manual checks — reversible
Task 1: complete (commits 3db0717..3f6a534, tests: cargo test → test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s)
Task 2: Ruling: filter empty keywords (Keywords=a;b; splits to a trailing '') — found by the plan's test; empty keyword would match every query — cost if wrong: none
Task 2: Ruling: crate-level #![allow(dead_code)] in main.rs replaces the per-mod allow while modules are built bottom-up; remove at Task 8 wiring — cost if wrong: hides an unused fn until then
Task 2: complete (commits 3f6a534..b581e11, tests: cargo test → test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s)
Task 3: complete (commits b581e11..33ed405, tests: cargo test → test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s)
Task 4: complete (commits 33ed405..3800eec, tests: cargo test → test result: ok. 27 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s)
Task 5: complete (commits 3800eec..32148da, tests: cargo test → test result: ok. 30 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s)
Task 6: complete (commits 32148da..2898edd, tests: cargo test → test result: ok. 35 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s)
Task 7: Ruling: verified logind Session.Lock / Manager.Suspend by busctl introspection instead of a temporary --lock that would lock James's live session — cost if wrong: lock path first exercised at the Task 8 manual check
Task 7: Ruling: fl! already takes runtime &str (CCCA lookup(key)), no i18n::get needed
Task 7: complete (commits 2898edd..412d5ff, tests: cargo test → test result: ok. 37 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s)
Task 8: Ruling: named the error message ShowError now instead of PowerFailed-then-rename at T11 — saves a rename — cost if wrong: none
Task 8: Ruling: popup data travels as Message::Loaded(Box<Loaded{apps,most_used,config}>) and Config::load runs in the same spawn_blocking — one message instead of AppsLoaded(apps,top) + separate config load — cost if wrong: later tasks extend Loaded instead
Task 8: Ruling: tiles use radius_m (16/8/2) not CCCA's pill — mockup approved this; a pill turns a 2x2 tile into a circle — cost if wrong: one fn
Task 8: Ruling: power menu is a popover anchored at Point(right of button); crate #![allow(dead_code)] kept until T19 since search/tiles are unwired — cost if wrong: hidden unused fns until then
Task 8: Ruling: manual check pending James (no uinput access to click the panel); applet reloaded via plugins_wings toggle and verified running new binary
Task 8: complete (commits 412d5ff..42225b5, tests: cargo test → test result: ok. 37 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s)
Task 9: Ruling: manual tile checks batched into James's hands-on pass (no uinput to click) — cost if wrong: layout bugs found later
Task 9: complete (commits 42225b5..6421c63, tests: cargo test → test result: ok. 37 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s)
Task 10: Ruling: keys via event::listen_with (sees captured events too, so Escape reaches us while the input is focused); Enter only via on_submit to avoid double launch; Esc on a non-empty query clears and refocuses — cost if wrong: key handling tweaks
Task 10: complete (commits 6421c63..9b8fa3a, tests: cargo test → test result: ok. 37 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s)
Task 11: Ruling: context menu is one popover over the whole popup body, positioned at the last on_move point (same bounds); menu items in ui/context.rs — cost if wrong: menu offset tweaks
Task 11: complete (commits 9b8fa3a..0aa0868, tests: cargo test → test result: ok. 37 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s)
Task 12: Ruling: tap-to-pick / tap-to-drop instead of press-and-release drag — matches the mockup James approved and avoids drag gestures in a popup — cost if wrong: swap to mouse_area on_press/on_release later. Group renames held in memory, saved on Done.
Task 12: complete (commits 0aa0868..5104cb8, tests: cargo test → test result: ok. 37 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s)
Task 13: Ruling: offset_of is generic over the label type now (L: PartialEq) so T18 reuses it for categories without changing the test; letter grid replaces the list rather than overlaying it — cost if wrong: none
Task 13: complete (commits 5104cb8..d7b2e60, tests: cargo test → test result: ok. 38 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s)
Task 14: Ruling: settings uses radio for finish (CCCA pattern) not a dropdown; favorites_file renamed pub favorites_text for Reset — cost if wrong: none. Verified: --settings window opens and renders (screenshot).
Task 14: complete (commits d7b2e60..ccb893d, tests: cargo test → test result: ok. 38 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s)
Task 16: Ruling: Accent option added to the Settings window radio list too (spec §10.4) — cost if wrong: none
Task 16: complete (commits ccb893d..40baf0b, tests: cargo test → test result: ok. 41 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s)
Task 17: Ruling: added a third test pinning the real App Library RON shape (multi-line AppIds with trailing commas) — cost if wrong: none
Task 17: complete (commits 40baf0b..2f3770a, tests: cargo test → test result: ok. 45 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s)
Task 18: Ruling: added apps::sections_of (tested) for the Folders view's loose apps; jump offsets computed as prefix (most-used + folders block) + offset_of(sections, 0, ..) instead of changing offset_of's signature — cost if wrong: jump lands a row off
Task 18: Ruling: list view takes a ListView struct (8 inputs) instead of positional args — cost if wrong: none
Task 18: complete (commits 2f3770a..751f333, tests: cargo test → test result: ok. 46 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s)
Task 19: Ruling: avatar read from /var/lib/AccountsService/icons/$USER (world-readable; the same file AccountsService's IconFile points to) or ~/.face, initial from /etc/passwd GECOS — no D-Bus round trip in the blocking loader — cost if wrong: avatar falls back to the initial
Task 19: Ruling: seeding still reads the dock favourites file directly (config::favorites_text) rather than switching to favorites::read(); same data, keeps Task 4's tested path — cost if wrong: none
Task 19: Ruling: favourites list in the menu updates only after the cosmic-config write succeeds (FavouritesSaved) — cost if wrong: none
Task 19: Ruling: removed crate-level #![allow(dead_code)]; deleted unused pill_radius and Spacing::pad_x
Task 19: complete (commits 751f333..63559be, tests: cargo test → test result: ok. 47 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s)
Fix (James): list row contents centred vertically (fixed-height buttons laid content from the top) — manual check by James
Final: fixed Important #2 (edit before first load wipes pins) + #3 (Settings saves stale copy) + Minor #1 (concurrent saves) — update_at_changes_the_file_not_a_stale_copy RED→GREEN, suite 49/49
Final: fixed Important #1 (popup state survives click-outside close; renames lost) — reset_popup_state on every close path, renames persist per keystroke via update_at; no unit test possible without a Core (manual check) — suite 49/49
Final: Ruling: re-graded Minor #3 (RemoveGroup keeps picked) to Important — moves the wrong tile — fixed by clearing picked; manual check
Final: Ruling: re-graded Minor #9 hygiene to Important (Global Constraint) — fixed src/folders.rs comment and mockup app ids; spec/plan/commit messages still say "James" — decide before first push
Final: Ruling: ConfigSaved(generation) replaces the on-screen config only if no newer edit — cost if wrong: a rename box could flicker
Final: minor (deferred): Loaded doesn't clear selection/context; stale index possible if clicked just before Loaded lands
Final: minor (deferred): Settings window lacks list_mode/right_side controls (spec §10 says "and in Settings")
Final: minor (deferred): process spawns and Settings Reset's app scan run inside update
Final: minor (deferred): apps scanned twice per open (Config::load seeding)
Final: minor (deferred): Escape closes the popup even when a context/mode menu is what should close
Final: minor (deferred): empty group left after moving its last tile; second corrupt config overwrites .bak; missing Exec launches silently nothing
Task 15: Ruling: App Library button removed from plugins_wings on James's go-ahead (backup plugins_wings.bak-before-applib-swap); README written; not pushed
Task 15: complete (commits 63559be..5850cdb, tests: cargo test → test result: ok. 49 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s)
