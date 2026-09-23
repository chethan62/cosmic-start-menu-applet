//! Starting apps, the way COSMIC's own launcher does: through libcosmic's
//! `spawn_desktop_exec`, which strips field codes and puts the app in its own
//! systemd scope.

use crate::apps::{Action, App};
use crate::usage::Usage;

pub async fn app(app: App) {
    let Some(exec) = app.exec.clone() else {
        return;
    };
    cosmic::desktop::spawn_desktop_exec(
        exec,
        Vec::<(String, String)>::new(),
        Some(&app.id),
        app.terminal,
    )
    .await;
    record_blocking(app.id).await;
}

pub async fn action(app_id: String, action: Action, terminal: bool) {
    cosmic::desktop::spawn_desktop_exec(
        action.exec,
        Vec::<(String, String)>::new(),
        Some(&app_id),
        terminal,
    )
    .await;
    record_blocking(app_id).await;
}

async fn record_blocking(app_id: String) {
    let _ = tokio::task::spawn_blocking(move || record(&app_id)).await;
}

pub fn record(app_id: &str) {
    let Some(path) = Usage::path() else { return };
    let mut u = Usage::load_from(&path);
    u.record(app_id);
    if let Err(e) = u.save_to(&path) {
        tracing::warn!("could not save usage: {e}");
    }
}
