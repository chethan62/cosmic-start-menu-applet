//! One Settings window, however many times you right-click the panel button.
//!
//! The applet opens Settings by spawning itself with `--settings`, which is the
//! simplest thing that works and, on its own, opens a new window every time.
//! Ten right-clicks left ten identical windows to close. The first window now
//! owns a bus name (see [`crate::instance`]); later ones ask it to come
//! forward and exit.

use std::sync::{Mutex, OnceLock};
use tokio::sync::mpsc;

pub use crate::instance::Claim;

const NAME: &str = "io.github.jjnuthuagen.StartMenuSettings";
const PATH: &str = "/io/github/jjnuthuagen/StartMenuSettings";

/// Requests from later invocations, read by the window's subscription.
static PRESENT: OnceLock<Mutex<Option<mpsc::UnboundedReceiver<()>>>> = OnceLock::new();

#[zbus::proxy(
    interface = "io.github.jjnuthuagen.StartMenuSettings",
    default_service = "io.github.jjnuthuagen.StartMenuSettings",
    default_path = "/io/github/jjnuthuagen/StartMenuSettings"
)]
trait Present {
    /// Ask the running window to raise and focus itself.
    fn present(&self) -> zbus::Result<()>;
}

/// The object served by whichever process owns the name.
struct Service {
    requests: mpsc::UnboundedSender<()>,
}

#[zbus::interface(name = "io.github.jjnuthuagen.StartMenuSettings")]
impl Service {
    fn present(&self) {
        // A closed receiver means the window is on its way out; the caller is
        // about to find the name free and open its own.
        let _ = self.requests.send(());
    }
}

/// Claim the name, or ask the existing window to come forward.
pub fn claim() -> Claim {
    let (claim, requests) = crate::instance::claim(
        "settings-single-instance",
        NAME,
        PATH,
        |requests| Service { requests },
        |connection| async move { PresentProxy::new(&connection).await?.present().await },
    );
    if let Some(requests) = requests {
        let _ = PRESENT.set(Mutex::new(Some(requests)));
    }
    claim
}

/// A stream of "come forward" requests from later invocations.
///
/// Yields nothing at all when this process did not claim the name, or when the
/// receiver has already been taken — the subscription is built once, but iced
/// is free to call `subscription()` as often as it likes.
pub fn requests() -> Option<mpsc::UnboundedReceiver<()>> {
    PRESENT.get()?.lock().ok()?.take()
}
