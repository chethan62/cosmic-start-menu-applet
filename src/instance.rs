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
    use zbus::fdo::{RequestNameFlags, RequestNameReply};

    let connection = zbus::Connection::session().await?;
    connection.object_server().at(path, object).await?;

    // `DO_NOT_QUEUE` is what makes this a test rather than a wait: without it
    // a second copy would sit in the queue and take the name the moment the
    // first exited.
    //
    // zbus 5 turns REPLY_EXISTS into `Err(Error::NameTaken)` rather than an
    // `Ok(RequestNameReply::Exists)`, so "someone else has it" has to match on
    // the error. Reaching for the reply was why `--settings` once opened a new
    // window every time. Any other reply (`InQueue` should not happen with
    // `DO_NOT_QUEUE`) is treated as taken too.
    match connection
        .request_name_with_flags(name, RequestNameFlags::DoNotQueue.into())
        .await
    {
        Ok(RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner) => Ok(Some(connection)),
        Ok(_) | Err(zbus::Error::NameTaken) => {
            if let Err(err) = poke(connection).await {
                tracing::debug!("could not reach the running copy of {name}: {err}");
            }
            Ok(None)
        }
        Err(err) => Err(err),
    }
}
