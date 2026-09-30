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

/// Become the shortcut menu, or toggle the one already running.
pub fn claim() -> Claim {
    let (claim, requests) = crate::instance::claim(
        "start-menu-remote",
        NAME,
        PATH,
        |requests| Service { requests },
        |connection| async move { RemoteProxy::new(&connection).await?.toggle().await },
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
    let Ok(executable) = std::env::current_exe() else {
        tracing::error!("could not determine our own path; cannot open the menu");
        return;
    };
    let mut command = std::process::Command::new(executable);
    command.arg("--toggle");
    if let Err(err) = crate::process::spawn_and_reap(command) {
        tracing::error!("could not open the menu: {err}");
    }
}
