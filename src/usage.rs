//! How often each app is launched from the menu, for the "Most used" list.
//! Kept in the state dir, not config: it is history, not a preference.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub counts: BTreeMap<String, u32>,
    /// Most recent launch first, at most `RECENT_CAP`.
    #[serde(default)]
    pub recent: Vec<String>,
}

pub const RECENT_CAP: usize = 16;

impl Usage {
    pub fn path() -> Option<PathBuf> {
        Some(
            dirs::state_dir()?
                .join("cosmic-start-menu-applet")
                .join("usage.toml"),
        )
    }

    /// A missing or unreadable file is an empty history, never an error.
    pub fn load_from(path: &Path) -> Usage {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| {
                toml::from_str(&raw)
                    .map_err(|e| tracing::warn!("{}: {e}", path.display()))
                    .ok()
            })
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
        self.recent.retain(|a| a != app);
        self.recent.insert(0, app.to_owned());
        self.recent.truncate(RECENT_CAP);
    }

    /// Up to `n` recently launched installed apps, newest first.
    pub fn recent(&self, n: usize, installed: &HashSet<&str>) -> Vec<String> {
        self.recent
            .iter()
            .filter(|a| installed.contains(a.as_str()))
            .take(n)
            .cloned()
            .collect()
    }

    /// The `n` most launched installed apps; ties by id so the order is stable.
    pub fn top(&self, n: usize, installed: &HashSet<&str>) -> Vec<String> {
        let mut v: Vec<(&String, &u32)> = self
            .counts
            .iter()
            .filter(|(id, _)| installed.contains(id.as_str()))
            .collect();
        v.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        v.into_iter().take(n).map(|(id, _)| id.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_orders_by_count_then_id_and_skips_uninstalled() {
        let mut u = Usage::default();
        for a in ["b", "a", "b", "gone", "gone", "gone", "c"] {
            u.record(a);
        }
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
    fn recent_is_newest_first_deduped_and_capped() {
        let mut u = Usage::default();
        for i in 0..20 {
            u.record(&format!("app{i}"));
        }
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

    #[test]
    fn count_saturates_instead_of_overflowing() {
        let mut u = Usage::default();
        u.counts.insert("x".into(), u32::MAX);
        u.record("x");
        assert_eq!(u.counts["x"], u32::MAX);
    }
}
