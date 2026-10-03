//! One running copy per well-known bus name, and a way for later copies to
//! reach it. Shared by the Settings window (a second right-click raises the
//! open window) and the shortcut menu (a second Super press closes it).
//!
//! # Why a bus name and not a lock file
//!
//! A lock file answers "is one already running?" and nothing else. The useful
//! behaviour is the second half — tell the running one to *do* something — and
//! that needs the first process to be told. A well-known bus name gives both
//! from one mechanism: claiming it is the mutual exclusion, and a method call
//! on it is the request.
//!
//! It also fails in the right direction. If the session bus is unreachable,
//! the caller carries on as the only copy; a window that refuses to start
//! because it could not talk to D-Bus would be a worse bug than a duplicate.

use std::future::Future;
use tokio::sync::mpsc;

/// What [`claim`] found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Claim {
    /// Nothing else held the name. This process is the running copy.
    Ours,
    /// Another copy holds the name and has been poked. This process should
    /// exit without drawing anything.
    AlreadyOpen,
}

/// Claim `name`, serving the object `serve` builds at `path`; or, if another
/// process holds it, run `poke` against that one.
///
/// The object gets a sender; each method call it forwards arrives on the
/// returned receiver, which is `Some` only when the claim is ours.
///
/// Blocks briefly on a background thread that keeps running for the life of
/// the process: the connection has to stay alive to keep serving, and dropping
/// it would release the name and let the next copy think it is the first.
pub fn claim<I, P, F>(
    label: &'static str,
    name: &'static str,
    path: &'static str,
    serve: impl FnOnce(mpsc::UnboundedSender<()>) -> I + Send + 'static,
    poke: P,
) -> (Claim, Option<mpsc::UnboundedReceiver<()>>)
where
    I: zbus::object_server::Interface,
    P: FnOnce(zbus::Connection) -> F + Send + 'static,
    F: Future<Output = zbus::Result<()>>,
{
    let (answer_tx, answer_rx) = std::sync::mpsc::channel();
    let (tx, rx) = mpsc::unbounded_channel();

    let started = std::thread::Builder::new()
        .name(label.into())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(err) => {
                    tracing::warn!("{label}: no runtime: {err}");
                    let _ = answer_tx.send(Claim::Ours);
                    return;
                }
            };
            runtime.block_on(async move {
                match register(name, path, serve(tx), poke).await {
                    Ok(Some(connection)) => {
                        let _ = answer_tx.send(Claim::Ours);
                        // Hold the connection for the process's lifetime:
                        // dropping it releases the name. `pending` is what
                        // keeps this runtime, and the connection's reader, alive.
                        let _hold = connection;
                        std::future::pending::<()>().await;
                    }
                    Ok(None) => {
                        let _ = answer_tx.send(Claim::AlreadyOpen);
                    }
                    Err(err) => {
                        // No bus, or it refused: carry on as the only copy.
                        tracing::debug!("{label}: unavailable: {err}");
                        let _ = answer_tx.send(Claim::Ours);
                    }
                }
            });
        });

    if let Err(err) = started {
        tracing::warn!("{label}: could not start: {err}");
        return (Claim::Ours, None);
    }

    match answer_rx.recv().unwrap_or(Claim::Ours) {
        Claim::Ours => (Claim::Ours, Some(rx)),
        Claim::AlreadyOpen => (Claim::AlreadyOpen, None),
    }
}

/// How long to wait for a dying owner to drop the name before claiming it.
/// Long enough for a process already on its way out to finish releasing,
/// short enough that a press never feels dropped.
const RETRY_PAUSE: std::time::Duration = std::time::Duration::from_millis(120);

/// Whether a name request's reply means the name is ours now. Anything else
/// — in the queue, exists, or zbus 5's `NameTaken` error — means somebody
/// else holds it.
fn became_owner(reply: &zbus::Result<zbus::fdo::RequestNameReply>) -> bool {
    use zbus::fdo::RequestNameReply;
    matches!(
        reply,
        Ok(RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner)
    )
}

/// The connection when this process now owns the name; `None` when another
/// does, after poking it.
async fn register<I, P, F>(
    name: &'static str,
    path: &'static str,
    object: I,
    poke: P,
) -> zbus::Result<Option<zbus::Connection>>
where
    I: zbus::object_server::Interface,
    P: FnOnce(zbus::Connection) -> F,
    F: Future<Output = zbus::Result<()>>,
{
    use zbus::fdo::RequestNameFlags;

    let connection = zbus::Connection::session().await?;
    connection.object_server().at(path, object).await?;

    let request = || connection.request_name_with_flags(name, RequestNameFlags::DoNotQueue.into());

    // `DO_NOT_QUEUE` is what makes this a test rather than a wait: without it
    // a second copy would sit in the queue and take the name the moment the
    // first exited.
    //
    // zbus 5 turns REPLY_EXISTS into `Err(Error::NameTaken)` rather than an
    // `Ok(RequestNameReply::Exists)`, so "someone else has it" has to match on
    // the error. Reaching for the reply was why `--settings` once opened a new
    // window every time. Any other reply (`InQueue` should not happen with
    // `DO_NOT_QUEUE`) is treated as taken too.
    let reply = request().await;
    if became_owner(&reply) {
        return Ok(Some(connection));
    }
    if let Err(err) = reply {
        // Not "taken" at all: a real bus failure, which the caller reads as
        // "no bus, carry on as the only copy".
        if !matches!(err, zbus::Error::NameTaken) {
            return Err(err);
        }
    }
    let Err(err) = poke(connection.clone()).await else {
        return Ok(None);
    };
    // The owner could not be reached: almost always a copy that was exiting
    // as this press landed, so the press would otherwise do nothing at all.
    // Wait for the name to be released and try to become the owner instead.
    tracing::debug!("could not reach the running copy of {name}: {err}; claiming it instead");
    tokio::time::sleep(RETRY_PAUSE).await;
    if became_owner(&request().await) {
        return Ok(Some(connection));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::fdo::RequestNameReply;

    #[test]
    fn only_primary_or_already_owner_means_the_name_is_ours() {
        assert!(became_owner(&Ok(RequestNameReply::PrimaryOwner)));
        // Re-requesting a name we hold is still ours — the retry after a
        // failed poke can land on this.
        assert!(became_owner(&Ok(RequestNameReply::AlreadyOwner)));
        // Somebody else has it. zbus 5 reports the usual case as an error
        // rather than a reply, which is why both shapes are checked.
        assert!(!became_owner(&Ok(RequestNameReply::Exists)));
        assert!(!became_owner(&Ok(RequestNameReply::InQueue)));
        assert!(!became_owner(&Err(zbus::Error::NameTaken)));
    }
}
