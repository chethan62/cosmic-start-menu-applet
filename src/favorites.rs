//! The dock's favourites. Read and written through cosmic-config — the same
//! store the dock watches — so adding one here shows up in the dock at once.

use cosmic::cosmic_config::{Config, ConfigGet, ConfigSet};

const ID: &str = "com.system76.CosmicAppList";

pub fn read() -> Vec<String> {
    Config::new(ID, 1)
        .ok()
        .and_then(|c| c.get::<Vec<String>>("favorites").ok())
        .unwrap_or_default()
}

/// `list` with `id` appended, unless it is already there.
pub fn with_added(list: &[String], id: &str) -> Vec<String> {
    let mut v = list.to_vec();
    if !v.iter().any(|x| x == id) {
        v.push(id.to_owned());
    }
    v
}

pub fn with_removed(list: &[String], id: &str) -> Vec<String> {
    list.iter().filter(|x| *x != id).cloned().collect()
}

pub fn write(list: &[String]) -> Result<(), String> {
    Config::new(ID, 1)
        .map_err(|e| e.to_string())?
        .set("favorites", list.to_vec())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_remove_are_idempotent_and_keep_order() {
        let l = vec!["a".to_string(), "b".to_string()];
        assert_eq!(with_added(&l, "c"), ["a", "b", "c"]);
        assert_eq!(with_added(&l, "a"), ["a", "b"]);
        assert_eq!(with_removed(&l, "a"), ["b"]);
        assert_eq!(with_removed(&l, "zzz"), ["a", "b"]);
    }
}
