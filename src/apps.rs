//! Installed applications, read from the XDG desktop-entry directories.
//!
//! Loaded fresh every time the popup opens (off the UI thread), so an app
//! installed a moment ago is there on the next open — no file watcher needed.

use std::collections::HashSet;
use std::path::PathBuf;

use cosmic::desktop::fde;
pub use fde::IconSource;

#[derive(Debug, Clone, PartialEq)]
pub struct App {
    pub id: String,
    pub name: String,
    pub generic_name: Option<String>,
    pub keywords: Vec<String>,
    pub icon: IconSource,
    pub exec: Option<String>,
    pub terminal: bool,
    pub actions: Vec<Action>,
    /// Desktop-entry `Categories`, e.g. `["Graphics", "3DGraphics"]`.
    pub categories: Vec<String>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            generic_name: None,
            keywords: Vec::new(),
            icon: IconSource::Name(String::new()),
            exec: None,
            terminal: false,
            actions: Vec::new(),
            categories: Vec::new(),
        }
    }
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
    load_from(
        fde::default_paths().collect(),
        &locales(),
        desktop.as_deref(),
    )
}

/// Whether a `;`-list from `OnlyShowIn`/`NotShowIn` names the current desktop.
/// `XDG_CURRENT_DESKTOP` is itself a `:`-list, e.g. `COSMIC:GNOME`.
fn names_current(list: &[&str], current: &str) -> bool {
    list.iter().any(|d| current.split(':').any(|c| c == *d))
}

/// Load from `dirs` in precedence order: an id seen in an earlier directory
/// shadows the same id later, which is how a user override in
/// `~/.local/share/applications` beats the system copy.
pub fn load_from(dirs: Vec<PathBuf>, locales: &[String], desktop: Option<&str>) -> Vec<App> {
    let mut seen = HashSet::new();
    let mut apps: Vec<App> = fde::Iter::new(dirs.into_iter())
        .filter_map(
            |path| match fde::DesktopEntry::from_path(path.clone(), Some(locales)) {
                Ok(de) => Some(de),
                Err(err) => {
                    tracing::debug!("skipping {}: {err}", path.display());
                    None
                }
            },
        )
        .filter(|de| seen.insert(de.id().to_owned()))
        .filter(|de| de.type_().is_none_or(|t| t == "Application"))
        .filter(|de| !de.hidden() && !de.no_display())
        .filter(|de| match (desktop, de.only_show_in()) {
            (Some(current), Some(only)) => names_current(&only, current),
            (None, Some(_)) => false,
            _ => true,
        })
        .filter(|de| match (desktop, de.not_show_in()) {
            (Some(current), Some(not)) => !names_current(&not, current),
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
                name: de
                    .action_entry_localized(action, "Name", locales)?
                    .to_string(),
                exec: de.action_entry(action, "Exec")?.to_string(),
            })
        })
        .collect();
    App {
        id: de.id().to_owned(),
        name: de
            .name(locales)
            .map(|n| n.to_string())
            .unwrap_or_else(|| de.id().to_owned()),
        generic_name: de.generic_name(locales).map(|n| n.to_string()),
        keywords: de
            .keywords(locales)
            .unwrap_or_default()
            .into_iter()
            // `Keywords=a;b;` ends with a separator, which splits to a
            // trailing empty keyword that would match every search.
            .filter(|k| !k.trim().is_empty())
            .map(|k| k.to_string())
            .collect(),
        icon: IconSource::from_unknown(de.icon().unwrap_or(de.id())),
        exec: de.exec().map(str::to_owned),
        terminal: de.terminal(),
        actions,
        categories: de
            .categories()
            .unwrap_or_default()
            .into_iter()
            .filter(|c| !c.is_empty())
            .map(str::to_owned)
            .collect(),
    }
}

/// Main desktop-entry categories, in priority order: an app in both Utility
/// and Development is filed under Development.
const MAIN: &[(&str, &str)] = &[
    ("Game", "cat-games"),
    ("Graphics", "cat-graphics"),
    ("AudioVideo", "cat-audio-video"),
    ("Development", "cat-development"),
    ("Network", "cat-internet"),
    ("Office", "cat-office"),
    ("Education", "cat-education"),
    ("Science", "cat-science"),
    ("Settings", "cat-settings"),
    ("System", "cat-system"),
    ("Utility", "cat-utilities"),
];

/// Display order of the category headings; l10n keys.
pub const CATEGORY_ORDER: &[&str] = &[
    "cat-games",
    "cat-graphics",
    "cat-audio-video",
    "cat-development",
    "cat-internet",
    "cat-office",
    "cat-education",
    "cat-science",
    "cat-settings",
    "cat-system",
    "cat-utilities",
    "cat-other",
];

/// The l10n key of the category heading an app goes under.
pub fn category_of(app: &App) -> &'static str {
    MAIN.iter()
        .find(|(k, _)| app.categories.iter().any(|c| c == k))
        .map_or("cat-other", |(_, v)| v)
}

/// The A–Z header an app sorts under. Digits, symbols and blank names share
/// `#`, which Windows puts first; any other script keeps its own capital.
pub fn letter(name: &str) -> char {
    match name.trim().chars().next() {
        Some(c) if c.is_alphabetic() => c.to_uppercase().next().unwrap_or(c),
        _ => '#',
    }
}

/// Group `apps` (already sorted) under their letters, `#` first, without
/// the apps the pinned block at the top of the list already shows — so
/// nothing appears twice on the same screen.
pub fn sections_excluding(apps: &[App], skip: &[usize]) -> Vec<(char, Vec<usize>)> {
    let kept: Vec<usize> = (0..apps.len()).filter(|i| !skip.contains(i)).collect();
    sections_of(apps, &kept)
}

/// Like [`sections_of`], minus the pinned apps.
pub fn sections_of_excluding(
    apps: &[App],
    indices: &[usize],
    skip: &[usize],
) -> Vec<(char, Vec<usize>)> {
    let kept: Vec<usize> = indices
        .iter()
        .copied()
        .filter(|i| !skip.contains(i))
        .collect();
    sections_of(apps, &kept)
}

/// Apps grouped by category, in `CATEGORY_ORDER`, minus the pinned apps
/// for the same reason.
pub fn category_sections_excluding(
    apps: &[App],
    skip: &[usize],
) -> Vec<(&'static str, Vec<usize>)> {
    CATEGORY_ORDER
        .iter()
        .filter_map(|&key| {
            let idx: Vec<usize> = (0..apps.len())
                .filter(|&i| !skip.contains(&i) && category_of(&apps[i]) == key)
                .collect();
            (!idx.is_empty()).then_some((key, idx))
        })
        .collect()
}

/// Like [`sections_excluding`], over only the apps at `indices` — the Folders view's
/// apps that are in no folder.
pub fn sections_of(apps: &[App], indices: &[usize]) -> Vec<(char, Vec<usize>)> {
    let mut out: Vec<(char, Vec<usize>)> = Vec::new();
    for &i in indices {
        let Some(app) = apps.get(i) else { continue };
        let l = letter(&app.name);
        match out.iter_mut().find(|(c, _)| *c == l) {
            Some((_, v)) => v.push(i),
            None => out.push((l, vec![i])),
        }
    }
    out.sort_by_key(|(c, _)| (*c != '#', *c));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn entry(dir: &std::path::Path, id: &str, body: &str) {
        fs::write(
            dir.join(format!("{id}.desktop")),
            format!("[Desktop Entry]\nType=Application\n{body}\n"),
        )
        .unwrap();
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
        entry(
            d.path(),
            "ff",
            "Name=Firefox\nGenericName=Web Browser\nKeywords=internet;www;\nExec=firefox\nActions=private;\n\n[Desktop Action private]\nName=New Private Window\nExec=firefox --private-window",
        );
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
    fn category_priority_and_other() {
        let mk = |c: &[&str]| App {
            categories: c.iter().map(|s| s.to_string()).collect(),
            ..App::default()
        };
        assert_eq!(
            category_of(&mk(&["Utility", "Development"])),
            "cat-development"
        );
        assert_eq!(
            category_of(&mk(&["Graphics", "3DGraphics"])),
            "cat-graphics"
        );
        assert_eq!(category_of(&mk(&["COSMIC"])), "cat-other");
    }

    #[test]
    fn sections_of_a_subset_only_lists_those_apps() {
        let mk = |n: &str| App {
            name: n.into(),
            ..App::default()
        };
        let apps = vec![mk("Alacritty"), mk("Blender"), mk("btop"), mk("Calc")];
        let s = sections_of(&apps, &[0, 2, 3]);
        assert_eq!(s, [('A', vec![0]), ('B', vec![2]), ('C', vec![3])]);
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
        let mk = |n: &str| App {
            name: n.into(),
            ..App::default()
        };
        let apps = vec![
            mk("0 A.D."),
            mk("Alacritty"),
            mk("audacity"),
            mk("Bitwarden"),
        ];
        let s = sections_excluding(&apps, &[]);
        let letters: Vec<char> = s.iter().map(|(c, _)| *c).collect();
        assert_eq!(letters, ['#', 'A', 'B']);
        assert_eq!(s[1].1, [1, 2]);
    }
}
