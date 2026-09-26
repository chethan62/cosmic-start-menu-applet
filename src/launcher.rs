//! Everything COSMIC's launcher can find beyond apps: open windows, sums,
//! files, commands, web searches.
//!
//! The launcher's results come from `pop-launcher`, a service that speaks
//! one JSON message per line on stdin/stdout and runs a plugin per kind of
//! result. The menu keeps one running for its whole life and asks it the same
//! question the search box is asked. Apps are left out of what it returns: the
//! menu's own ranking already lists them, above everything here.
//!
//! If `pop-launcher` is missing or dies, search still finds apps; the extra
//! sections are simply empty until the next keystroke starts a fresh one.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::{Mutex, OnceLock};

use serde::Deserialize;
use tokio::sync::mpsc;

/// What a result is, by which plugin sent it. Also the order sections appear in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Section {
    Windows,
    Calculator,
    Files,
    Commands,
    Web,
    Sound,
    Other,
}

impl Section {
    /// Plugins mark each result with their own icon, which is the only thing
    /// that says which plugin it came from. `None` means an app: skip it.
    fn of(category_icon: Option<&str>) -> Option<Section> {
        Some(match category_icon? {
            "new-window-symbolic" => return None,
            "focus-windows-symbolic" => Section::Windows,
            "x-office-spreadsheet" | "accessories-calculator" => Section::Calculator,
            "system-file-manager" => Section::Files,
            "utilities-terminal" => Section::Commands,
            "system-search" => Section::Web,
            "multimedia-volume-control" => Section::Sound,
            _ => Section::Other,
        })
    }

    pub fn title_key(self) -> &'static str {
        match self {
            Section::Windows => "search-windows",
            Section::Calculator => "search-calculator",
            Section::Files => "search-files",
            Section::Commands => "search-commands",
            Section::Web => "search-web",
            Section::Sound => "search-sound",
            Section::Other => "search-other",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// The launcher's id, sent back to activate it.
    pub id: u32,
    pub name: String,
    pub description: String,
    pub icon: Option<String>,
    pub section: Section,
}

/// What the launcher sent back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    /// Results for the latest query, sectioned and in display order.
    Results(Vec<Item>),
    /// Replace the query with this text (tab-completion, e.g. a folder path).
    Fill(String),
    /// The action is done; close the menu.
    Close,
}

#[derive(Deserialize)]
enum Icon {
    Name(String),
    Mime(String),
}

impl Icon {
    fn name(self) -> String {
        match self {
            Icon::Name(n) => n,
            // `text/plain` is drawn by the icon named `text-plain`.
            Icon::Mime(m) => m.replace('/', "-"),
        }
    }
}

#[derive(Deserialize)]
struct RawItem {
    id: u32,
    name: String,
    #[serde(default)]
    description: String,
    icon: Option<Icon>,
    category_icon: Option<Icon>,
}

#[derive(Deserialize)]
enum RawReply {
    Update(Vec<RawItem>),
    Fill(String),
}

/// One line from the launcher. Lines the menu has no use for (desktop
/// entries, context menus) are `None`.
pub fn parse(line: &str) -> Option<Reply> {
    let line = line.trim();
    if line == "\"Close\"" {
        return Some(Reply::Close);
    }
    match serde_json::from_str::<RawReply>(line).ok()? {
        RawReply::Fill(text) => Some(Reply::Fill(text)),
        RawReply::Update(raw) => {
            let mut items: Vec<Item> = raw
                .into_iter()
                .filter_map(|r| {
                    let category = r.category_icon.map(Icon::name);
                    Some(Item {
                        section: Section::of(category.as_deref())?,
                        id: r.id,
                        name: r.name,
                        description: r.description,
                        icon: r.icon.map(Icon::name),
                    })
                })
                .collect();
            // Stable, so each section keeps the launcher's own ranking.
            items.sort_by_key(|i| i.section);
            Some(Reply::Results(items))
        }
    }
}

struct Service {
    child: Child,
    stdin: ChildStdin,
}

static SERVICE: Mutex<Option<Service>> = Mutex::new(None);
/// Replies flow from the reader thread (whichever service is current) to the
/// one subscription that takes the receiver.
struct Channel {
    tx: mpsc::UnboundedSender<Reply>,
    rx: Mutex<Option<mpsc::UnboundedReceiver<Reply>>>,
}

static REPLIES: OnceLock<Channel> = OnceLock::new();

fn replies() -> &'static Channel {
    REPLIES.get_or_init(|| {
        let (tx, rx) = mpsc::unbounded_channel();
        Channel {
            tx,
            rx: Mutex::new(Some(rx)),
        }
    })
}

/// Replies, for the applet's subscription. `None` after the first call.
pub fn receiver() -> Option<mpsc::UnboundedReceiver<Reply>> {
    replies().rx.lock().ok()?.take()
}

fn start() -> Option<Service> {
    let mut child = crate::process::host_command("pop-launcher")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| tracing::warn!("pop-launcher unavailable: {e}"))
        .ok()?;
    let stdin = child.stdin.take()?;
    let stdout = child.stdout.take()?;
    let tx = replies().tx.clone();
    let _ = std::thread::Builder::new()
        .name("pop-launcher-reader".into())
        .spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if let Some(reply) = parse(&line) {
                    let _ = tx.send(reply);
                }
            }
        });
    Some(Service { child, stdin })
}

/// Send one request, starting the service if needed and once more if the
/// running one has gone away.
fn send(request: &str) {
    let Ok(mut guard) = SERVICE.lock() else {
        return;
    };
    for _ in 0..2 {
        if guard.is_none() {
            *guard = start();
        }
        let Some(service) = guard.as_mut() else {
            return;
        };
        if writeln!(service.stdin, "{request}")
            .and_then(|()| service.stdin.flush())
            .is_ok()
        {
            return;
        }
        // Broken pipe: reap the dead one and try a fresh one.
        let mut dead = guard.take().map(|s| s.child);
        if let Some(child) = dead.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub fn search(query: &str) {
    if let Ok(json) = serde_json::to_string(&serde_json::json!({ "Search": query })) {
        send(&json);
    }
}

pub fn activate(id: u32) {
    send(&format!("{{\"Activate\":{id}}}"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apps_are_dropped_and_sections_ordered() {
        let line = r#"{"Update":[
            {"id":0,"name":"DuckDuckGo: rust","description":"https://duckduckgo.com/?q=rust","category_icon":{"Name":"system-search"}},
            {"id":1,"name":"Chromium","description":"System","icon":{"Name":"chromium"},"category_icon":{"Name":"new-window-symbolic"}},
            {"id":2,"name":"Terminal - rust","icon":{"Name":"cosmic-term"},"category_icon":{"Name":"focus-windows-symbolic"}},
            {"id":3,"name":"notes.txt","icon":{"Mime":"text/plain"},"category_icon":{"Name":"system-file-manager"}}
        ]}"#
        .replace('\n', "");
        let Some(Reply::Results(items)) = parse(&line) else {
            panic!("not results");
        };
        let order: Vec<(u32, Section)> = items.iter().map(|i| (i.id, i.section)).collect();
        assert_eq!(
            order,
            [
                (2, Section::Windows),
                (3, Section::Files),
                (0, Section::Web)
            ]
        );
        assert_eq!(items[1].icon.as_deref(), Some("text-plain"));
    }

    #[test]
    fn calculator_answer_is_its_own_section() {
        let line = r#"{"Update":[{"id":0,"name":"84","description":"","icon":{"Name":"accessories-calculator"},"category_icon":{"Name":"x-office-spreadsheet"}}]}"#;
        let Some(Reply::Results(items)) = parse(line) else {
            panic!("not results");
        };
        assert_eq!(items[0].section, Section::Calculator);
        assert_eq!(items[0].name, "84");
    }

    #[test]
    fn close_and_fill_are_understood_and_the_rest_ignored() {
        assert_eq!(parse("\"Close\""), Some(Reply::Close));
        assert_eq!(
            parse(r#"{"Fill":"~/Documents/"}"#),
            Some(Reply::Fill("~/Documents/".into()))
        );
        assert_eq!(parse(r#"{"DesktopEntry":{"path":"/x.desktop"}}"#), None);
        assert_eq!(parse("garbage"), None);
    }
}
