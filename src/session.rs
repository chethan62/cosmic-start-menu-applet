//! Power actions. Log out / restart / shut down go through `cosmic-osd`, which
//! shows COSMIC's own confirm-with-countdown dialog — the same one the stock
//! power applet shows. Lock and suspend have no dialog and go to logind.

use crate::process::{host_command, spawn_and_reap};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Power {
    Lock,
    LogOut,
    Suspend,
    Restart,
    ShutDown,
}

pub const ALL: [Power; 5] = [
    Power::Lock,
    Power::LogOut,
    Power::Suspend,
    Power::Restart,
    Power::ShutDown,
];

impl Power {
    pub fn l10n_key(self) -> &'static str {
        match self {
            Power::Lock => "power-lock",
            Power::LogOut => "power-log-out",
            Power::Suspend => "power-suspend",
            Power::Restart => "power-restart",
            Power::ShutDown => "power-shut-down",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Power::Lock => "system-lock-screen-symbolic",
            Power::LogOut => "system-log-out-symbolic",
            Power::Suspend => "system-suspend-symbolic",
            Power::Restart => "system-reboot-symbolic",
            Power::ShutDown => "system-shutdown-symbolic",
        }
    }
}

/// The `cosmic-osd` subcommand for actions that confirm first.
pub fn osd_arg(p: Power) -> Option<&'static str> {
    match p {
        Power::LogOut => Some("log-out"),
        Power::Restart => Some("restart"),
        Power::ShutDown => Some("shutdown"),
        Power::Lock | Power::Suspend => None,
    }
}

#[zbus::proxy(
    interface = "org.freedesktop.login1.Manager",
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1"
)]
trait Login1 {
    fn suspend(&self, interactive: bool) -> zbus::Result<()>;
}

#[zbus::proxy(
    interface = "org.freedesktop.login1.Session",
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1/session/auto"
)]
trait Login1Session {
    fn lock(&self) -> zbus::Result<()>;
}

pub async fn run(p: Power) -> Result<(), String> {
    if let Some(arg) = osd_arg(p) {
        let mut cmd = host_command("cosmic-osd");
        cmd.arg(arg);
        return spawn_and_reap(cmd).map(|_| ()).map_err(|e| e.to_string());
    }
    let conn = zbus::Connection::system()
        .await
        .map_err(|e| e.to_string())?;
    match p {
        Power::Lock => Login1SessionProxy::new(&conn)
            .await
            .map_err(|e| e.to_string())?
            .lock()
            .await
            .map_err(|e| e.to_string()),
        Power::Suspend => Login1Proxy::new(&conn)
            .await
            .map_err(|e| e.to_string())?
            .suspend(true)
            .await
            .map_err(|e| e.to_string()),
        Power::LogOut | Power::Restart | Power::ShutDown => {
            unreachable!("confirming actions returned above")
        }
    }
}

/// Open COSMIC Settings, on `page` when given (e.g. `users`).
pub fn open_settings_page(page: Option<&str>) -> Result<(), String> {
    let mut cmd = host_command("cosmic-settings");
    if let Some(page) = page {
        cmd.arg(page);
    }
    spawn_and_reap(cmd).map(|_| ()).map_err(|e| e.to_string())
}

/// Open the home folder in the default file manager.
pub fn open_files() -> Result<(), String> {
    let mut cmd = host_command("xdg-open");
    cmd.arg(dirs::home_dir().ok_or("no home directory")?);
    spawn_and_reap(cmd).map(|_| ()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logout_restart_shutdown_go_through_cosmic_osd() {
        assert_eq!(osd_arg(Power::LogOut), Some("log-out"));
        assert_eq!(osd_arg(Power::Restart), Some("restart"));
        assert_eq!(osd_arg(Power::ShutDown), Some("shutdown"));
        assert_eq!(osd_arg(Power::Lock), None);
        assert_eq!(osd_arg(Power::Suspend), None);
    }

    #[test]
    fn every_power_label_is_translated() {
        for p in ALL {
            assert_ne!(crate::fl!(p.l10n_key()), p.l10n_key());
        }
    }
}
