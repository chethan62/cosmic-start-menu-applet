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

/// What [`claim`] found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Claim {
    /// This process is the menu.
    Ours,
    /// A menu was already open and has been told to toggle.
    AlreadyOpen,
}

/// Become the shortcut menu, or toggle the one already running.
pub fn claim() -> Claim {
    let (answer_tx, answer_rx) = std::sync::mpsc::channel();
    let (tx, rx) = mpsc::unbounded_channel();
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
                    let _ = answer_tx.send(Claim::Ours);
                    return;
                }
            };
            runtime.block_on(async move {
                match register(tx).await {
                    Ok(Some(connection)) => {
                        let _ = answer_tx.send(Claim::Ours);
                        // Dropping the connection would release the name.
                        let _hold = connection;
                        std::future::pending::<()>().await;
                    }
                    Ok(None) => {
                        let _ = answer_tx.send(Claim::AlreadyOpen);
                    }
                    Err(err) => {
                        tracing::debug!("toggle service unavailable: {err}");
                        let _ = answer_tx.send(Claim::Ours);
                    }
                }
            });
        });
    if let Err(err) = started {
        tracing::warn!("could not start the toggle service: {err}");
        return Claim::Ours;
    }
    let claim = answer_rx.recv().unwrap_or(Claim::Ours);
    if claim == Claim::Ours {
        let _ = REQUESTS.set(Mutex::new(Some(rx)));
    }
    claim
}

/// The connection when this process now owns the name; `None` when another
/// menu does, after asking it to toggle.
async fn register(requests: mpsc::UnboundedSender<()>) -> zbus::Result<Option<zbus::Connection>> {
    use zbus::fdo::RequestNameFlags;
    let connection = zbus::Connection::session().await?;
    connection
        .object_server()
        .at(PATH, Service { requests })
        .await?;
    match connection
        .request_name_with_flags(NAME, RequestNameFlags::DoNotQueue.into())
        .await
    {
        Ok(
            zbus::fdo::RequestNameReply::PrimaryOwner | zbus::fdo::RequestNameReply::AlreadyOwner,
        ) => Ok(Some(connection)),
        // zbus 5 reports a taken name as this error, not as a reply.
        Ok(_) | Err(zbus::Error::NameTaken) => {
            if let Err(err) = RemoteProxy::new(&connection).await?.toggle().await {
                tracing::debug!("could not toggle the open menu: {err}");
            }
            Ok(None)
        }
        Err(err) => Err(err),
    }
}

/// Toggle requests, for the menu's subscription. `None` in the panel applet,
/// and after the first call: iced may build the subscription more than once.
pub fn requests() -> Option<mpsc::UnboundedReceiver<()>> {
    REQUESTS.get()?.lock().ok()?.take()
}
