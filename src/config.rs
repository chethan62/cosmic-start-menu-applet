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
pub enum TileSize {
    Small,
    #[default]
    Medium,
    Wide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TileFinish {
    #[default]
    Frosted,
    Solid,
    Outline,
}

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
        Self {
            finish: TileFinish::Frosted,
            show_most_used: true,
            groups: Vec::new(),
        }
    }
}

/// (group index, tile index) into `Config::groups`.
pub type TileRef = (usize, usize);

/// Pinned on first run when the dock has no favourites we can use.
pub const FALLBACK_PINS: &[&str] = &[
    "firefox",
    "chromium",
    "com.system76.CosmicFiles",
    "com.system76.CosmicTerm",
    "com.system76.CosmicSettings",
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

fn default_group(tiles: Vec<Tile>) -> Group {
    Group {
        name: crate::fl!("default-group"),
        tiles,
    }
}

impl Config {
    pub fn path() -> Option<PathBuf> {
        Some(
            dirs::config_dir()?
                .join("cosmic-start-menu-applet")
                .join("config.toml"),
        )
    }

    /// A first-run config: one group of the dock's favourites that are
    /// installed, or of the fallback list when none are.
    pub fn seeded(installed: &[String], favorites: Option<&str>) -> Config {
        let has = |id: &str| installed.iter().any(|i| i == id);
        let mut ids: Vec<String> = favorites
            .map(parse_favorites)
            .unwrap_or_default()
            .into_iter()
            .filter(|id| has(id))
            .collect();
        if ids.is_empty() {
            ids = FALLBACK_PINS
                .iter()
                .filter(|id| has(id))
                .map(|s| s.to_string())
                .collect();
        }
        Config {
            groups: vec![default_group(
                ids.into_iter()
                    .map(|app| Tile {
                        app,
                        size: TileSize::Medium,
                        source: None,
                    })
                    .collect(),
            )],
            ..Config::default()
        }
    }

    /// Load from the real path, seeding (and saving) on first run.
    pub fn load() -> Config {
        let installed: Vec<String> = crate::apps::load_all().into_iter().map(|a| a.id).collect();
        let favorites = favorites_file();
        match Self::path() {
            Some(p) => Self::load_from(&p, &installed, favorites.as_deref()),
            None => Self::seeded(&installed, favorites.as_deref()),
        }
    }

    /// `installed` and `favorites` are only consulted when seeding.
    pub fn load_from(path: &Path, installed: &[String], favorites: Option<&str>) -> Config {
        let raw = match std::fs::read_to_string(path) {
            Ok(raw) => raw,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let c = Self::seeded(installed, favorites);
                if let Err(err) = c.save_to(path) {
                    tracing::warn!("{err}");
                }
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
                // Keep the user's file rather than overwrite it: whatever they
                // wrote is still there to fix by hand.
                let bak = path.with_extension("toml.bak");
                tracing::error!(
                    "{} is invalid ({e}); moved to {}",
                    path.display(),
                    bak.display()
                );
                let _ = std::fs::rename(path, &bak);
                let c = Self::seeded(installed, favorites);
                if let Err(err) = c.save_to(path) {
                    tracing::warn!("{err}");
                }
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
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("could not create {}: {e}", dir.display()))?;
        }
        let body = toml::to_string_pretty(self).map_err(|e| format!("could not encode: {e}"))?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, format!("{HEADER}\n{body}"))
            .map_err(|e| format!("could not write {}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, path)
            .map_err(|e| format!("could not replace {}: {e}", path.display()))
    }

    pub fn is_pinned(&self, app: &str) -> bool {
        self.groups
            .iter()
            .any(|g| g.tiles.iter().any(|t| t.app == app))
    }

    /// Append a Medium tile to the first group, creating it if needed.
    pub fn pin(&mut self, app: &str) {
        if self.is_pinned(app) {
            return;
        }
        if self.groups.is_empty() {
            self.groups.push(default_group(Vec::new()));
        }
        self.groups[0].tiles.push(Tile {
            app: app.into(),
            size: TileSize::Medium,
            source: None,
        });
    }

    pub fn unpin(&mut self, app: &str) {
        for g in &mut self.groups {
            g.tiles.retain(|t| t.app != app);
        }
    }

    pub fn resize(&mut self, (g, i): TileRef, size: TileSize) {
        if let Some(t) = self.groups.get_mut(g).and_then(|g| g.tiles.get_mut(i)) {
            t.size = size;
        }
    }

    /// Move a tile to `to_index` in `to_group`; past the end appends.
    pub fn move_tile(&mut self, (g, i): TileRef, to_group: usize, to_index: usize) {
        if to_group >= self.groups.len()
            || self.groups.get(g).is_none_or(|grp| i >= grp.tiles.len())
        {
            return;
        }
        let tile = self.groups[g].tiles.remove(i);
        let dest = &mut self.groups[to_group].tiles;
        dest.insert(to_index.min(dest.len()), tile);
    }

    pub fn add_group(&mut self, name: String) -> usize {
        self.groups.push(Group {
            name,
            tiles: Vec::new(),
        });
        self.groups.len() - 1
    }

    pub fn rename_group(&mut self, group: usize, name: String) {
        if let Some(g) = self.groups.get_mut(group) {
            g.name = name;
        }
    }

    /// Remove a group, moving its tiles into the group before it (or the
    /// next one, if it was first) so nothing pinned is lost.
    pub fn remove_group(&mut self, group: usize) {
        if group >= self.groups.len() {
            return;
        }
        let removed = self.groups.remove(group);
        if self.groups.is_empty() {
            self.groups.push(default_group(removed.tiles));
            return;
        }
        let target = group.saturating_sub(1).min(self.groups.len() - 1);
        self.groups[target].tiles.extend(removed.tiles);
    }

    /// Tiles in `group` whose app is installed, with their config index. An
    /// uninstalled app's tile stays in the config and returns if it does.
    pub fn visible_tiles<'a>(
        &'a self,
        group: usize,
        installed: &HashSet<&str>,
    ) -> Vec<(usize, &'a Tile)> {
        self.groups
            .get(group)
            .map(|g| {
                g.tiles
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| installed.contains(t.app.as_str()))
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    const FAVS: &str =
        "[\n    \"com.anthropic.Claude\",\n    \"chromium\",\n    \"com.system76.CosmicTerm\",\n]";

    fn installed(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    fn ids(g: &Group) -> Vec<&str> {
        g.tiles.iter().map(|t| t.app.as_str()).collect()
    }

    #[test]
    fn parses_the_dock_favorites_ron_list() {
        assert_eq!(
            parse_favorites(FAVS),
            [
                "com.anthropic.Claude",
                "chromium",
                "com.system76.CosmicTerm"
            ]
        );
        assert!(parse_favorites("garbage").is_empty());
    }

    #[test]
    fn seeds_from_favorites_keeping_only_installed() {
        let c = Config::seeded(
            &installed(&["chromium", "com.system76.CosmicTerm"]),
            Some(FAVS),
        );
        assert_eq!(ids(&c.groups[0]), ["chromium", "com.system76.CosmicTerm"]);
    }

    #[test]
    fn seeds_from_fallback_when_favorites_missing_or_all_uninstalled() {
        let inst = installed(&["chromium", "com.system76.CosmicFiles"]);
        for favs in [None, Some("[\"not.installed\"]")] {
            let c = Config::seeded(&inst, favs);
            assert_eq!(ids(&c.groups[0]), ["chromium", "com.system76.CosmicFiles"]);
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
        assert_eq!(
            std::fs::read_to_string(p.with_extension("toml.bak")).unwrap(),
            "groups = 7"
        );
    }

    #[test]
    fn pin_unpin_resize_move() {
        let mut c = Config {
            groups: vec![],
            ..Config::default()
        };
        c.pin("a");
        c.pin("b");
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
        let mut c = Config {
            groups: vec![],
            ..Config::default()
        };
        for a in ["a", "b", "c"] {
            c.pin(a);
        }
        c.move_tile((0, 0), 0, 2);
        assert_eq!(ids(&c.groups[0]), ["b", "c", "a"]);
    }

    #[test]
    fn removing_a_group_keeps_its_tiles() {
        let mut c = Config {
            groups: vec![],
            ..Config::default()
        };
        c.pin("a");
        let g = c.add_group("Tools".into());
        c.groups[g].tiles.push(Tile {
            app: "b".into(),
            size: TileSize::Small,
            source: None,
        });
        c.remove_group(g);
        assert_eq!(c.groups.len(), 1);
        assert!(c.is_pinned("b"));
    }

    #[test]
    fn uninstalled_tiles_are_hidden_but_kept() {
        let mut c = Config {
            groups: vec![],
            ..Config::default()
        };
        c.pin("gone");
        c.pin("here");
        let inst: HashSet<&str> = ["here"].into();
        let vis: Vec<_> = c
            .visible_tiles(0, &inst)
            .into_iter()
            .map(|(i, t)| (i, t.app.clone()))
            .collect();
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
