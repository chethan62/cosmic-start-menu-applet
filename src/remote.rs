//! `cosmic-start-menu-applet --toggle`: the Start menu from a keyboard
//! shortcut.
//!
//! A menu opened from the panel is a popup, and a popup only gets the
//! keyboard when a click on the panel hands it over. A shortcut has no such
//! click, so the menu opened but typing went to the window underneath. So
//! `--toggle` shows the menu as its own layer surface, which the compositor
//! gives the keyboard to directly, the way COSMIC's launcher does.
//!
//! The first `--toggle` becomes that menu and owns a well-known bus name;
//! a second press finds the name taken and asks the open one to close. If the
//! bus is unreachable the menu opens anyway: a second press then opens a second
//! menu, which is a smaller bug than a shortcut that does nothing.

use std::sync::{Mutex, OnceLock};
use tokio::sync::mpsc;

const NAME: &str = "io.github.jjnuthuagen.StartMenu";
const PATH: &str = "/io/github/jjnuthuagen/StartMenu";

#[zbus::proxy(
    interface = "io.github.jjnuthuagen.StartMenu",
    default_service = "io.github.jjnuthuagen.StartMenu",
    default_path = "/io/github/jjnuthuagen/StartMenu"
)]
trait Remote {
    fn toggle(&self) -> zbus::Result<()>;
}

struct Service {
    requests: mpsc::UnboundedSender<()>,
}

#[zbus::interface(name = "io.github.jjnuthuagen.StartMenu")]
impl Service {
    fn toggle(&self) {
        let _ = self.requests.send(());
    }
}

static REQUESTS: OnceLock<Mutex<Option<mpsc::UnboundedReceiver<()>>>> = OnceLock::new();

pub use crate::instance::Claim;

/// What a command line asks of the menu process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    /// `--toggle`: show the menu, or close the one already up.
    Toggle,
    /// `--prewarm`: become the menu process and wait, with nothing on
    /// screen, so the first real press has nothing to load.
    Prewarm,
    /// Neither: this invocation is the panel applet.
    No,
}

/// Read the two menu flags off a command line. An explicit press wins over a
/// pre-warm, so a stray `--prewarm` can never swallow a `--toggle`.
pub fn start_mode<'a>(args: impl IntoIterator<Item = &'a str>) -> Start {
    let mut found = Start::No;
    for arg in args {
        match arg {
            "--toggle" => return Start::Toggle,
            "--prewarm" => found = Start::Prewarm,
            _ => {}
        }
    }
    found
}

/// Become the shortcut menu, or toggle the one already running.
pub fn claim() -> Claim {
    claim_name(true)
}

/// Become the menu process without disturbing one that is already there.
///
/// The pre-warm at panel start must not poke: the poke *is* the toggle, so a
/// second panel in the session, or an applet restart, would open a menu
/// nobody asked for.
pub fn claim_silently() -> Claim {
    claim_name(false)
}

fn claim_name(poke_it: bool) -> Claim {
    let (claim, requests) = crate::instance::claim(
        "start-menu-remote",
        NAME,
        PATH,
        |requests| Service { requests },
        move |connection| async move {
            if !poke_it {
                return Ok(());
            }
            RemoteProxy::new(&connection).await?.toggle().await
        },
    );
    if let Some(requests) = requests {
        let _ = REQUESTS.set(Mutex::new(Some(requests)));
    }
    claim
}

/// Toggle requests, for the menu's subscription. `None` in the panel applet,
/// and after the first call: iced may build the subscription more than once.
pub fn requests() -> Option<mpsc::UnboundedReceiver<()>> {
    REQUESTS.get()?.lock().ok()?.take()
}

/// Spawn this binary with `--toggle`: open the menu, or close the one that is
/// already up.
///
/// The panel button goes through the same door as the keyboard shortcut. The
/// menu has to be a layer surface to take the keyboard, an applet cannot host
/// one (asking for it from inside the panel's event loop draws nothing), and
/// two ways in that each made their own menu could put two on screen at once.
/// One process, claimed by name, is therefore the only menu there can be, and
/// a second press of either trigger closes it.
pub fn spawn_menu() {
    spawn_with("--toggle");
}

/// Start the menu process at panel start, closed and invisible, so the first
/// press finds the app index already read. A cold open showed a blank card
/// for ~350 ms while it built one.
pub fn prewarm() {
    spawn_with("--prewarm");
}

fn spawn_with(arg: &'static str) {
    let Ok(executable) = std::env::current_exe() else {
        tracing::error!("could not determine our own path; cannot open the menu");
        return;
    };
    let mut command = std::process::Command::new(executable);
    command.arg(arg);
    if let Err(err) = crate::process::spawn_and_reap(command) {
        tracing::error!("could not start the menu ({arg}): {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_line_says_whether_to_show_the_menu() {
        assert_eq!(start_mode(["--toggle"]), Start::Toggle);
        assert_eq!(start_mode(["--prewarm"]), Start::Prewarm);
        assert_eq!(start_mode([]), Start::No);
        assert_eq!(start_mode(["--settings"]), Start::No);
        // A press beats a pre-warm whichever order they arrive in, so a
        // stray flag can never leave a press with nothing on screen.
        assert_eq!(start_mode(["--prewarm", "--toggle"]), Start::Toggle);
        assert_eq!(start_mode(["--toggle", "--prewarm"]), Start::Toggle);
    }
}
