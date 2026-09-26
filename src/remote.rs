//! `cosmic-start-menu-applet --toggle`: open or close the menu from outside,
//! so a keyboard shortcut can do what clicking the panel button does.
//!
//! The running applet owns a well-known bus name and serves `Toggle` on it;
//! `--toggle` is a one-shot call to that method. Same shape as the Settings
//! window's single-instance name, and it fails the same quiet way: an applet
//! that cannot reach the bus still works from the panel.

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

/// Start serving `Toggle` on a background thread for the life of the applet.
/// Only the first applet instance gets the name; the others stay click-only.
pub fn serve() {
    let (tx, rx) = mpsc::unbounded_channel();
    let _ = REQUESTS.set(Mutex::new(Some(rx)));
    let started = std::thread::Builder::new()
        .name("start-menu-remote".into())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(err) => {
                    tracing::warn!("no runtime for the toggle service: {err}");
                    return;
                }
            };
            runtime.block_on(async move {
                match register(tx).await {
                    Ok(connection) => {
                        // Dropping the connection would release the name.
                        let _hold = connection;
                        std::future::pending::<()>().await;
                    }
                    Err(err) => tracing::debug!("toggle service unavailable: {err}"),
                }
            });
        });
    if let Err(err) = started {
        tracing::warn!("could not start the toggle service: {err}");
    }
}

async fn register(requests: mpsc::UnboundedSender<()>) -> zbus::Result<zbus::Connection> {
    use zbus::fdo::RequestNameFlags;
    let connection = zbus::Connection::session().await?;
    connection
        .object_server()
        .at(PATH, Service { requests })
        .await?;
    connection
        .request_name_with_flags(NAME, RequestNameFlags::DoNotQueue.into())
        .await?;
    Ok(connection)
}

/// Toggle requests, for the applet's subscription. `None` after the first
/// call: iced may build the subscription more than once.
pub fn requests() -> Option<mpsc::UnboundedReceiver<()>> {
    REQUESTS.get()?.lock().ok()?.take()
}

/// `--toggle`: ask the running applet to open or close its menu.
pub fn send_toggle() -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    runtime
        .block_on(async {
            let connection = zbus::Connection::session().await?;
            RemoteProxy::new(&connection).await?.toggle().await
        })
        .map_err(|e| format!("is the Start Menu in the panel? {e}"))
}
