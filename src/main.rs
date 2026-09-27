//! Start Menu — a Windows 10-style Start menu for the COSMIC panel.

mod app;
mod apps;
mod config;
mod favorites;
mod folders;
mod i18n;
mod instance;
mod launch;
mod launcher;
mod process;
mod remote;
mod search;
mod session;
mod settings;
mod shortcut;
mod single_instance;
mod tile_layout;
mod ui;
mod usage;

fn main() -> cosmic::iced::Result {
    // `RUST_LOG=debug` shows why an app was skipped or a file was unreadable.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();
    // `--settings` runs the same binary as an ordinary window. The panel
    // button spawns this on right-click; later invocations bring the open
    // window forward instead of opening another.
    if std::env::args().skip(1).any(|arg| arg == "--settings") {
        if single_instance::claim() == single_instance::Claim::AlreadyOpen {
            return Ok(());
        }
        return cosmic::app::run::<settings::Settings>(settings::window_settings(), ());
    }

    // `--toggle` is for keyboard shortcuts: the menu as its own surface, so
    // it gets the keyboard; or, if one is open, close it.
    if std::env::args().skip(1).any(|arg| arg == "--toggle") {
        if remote::claim() == remote::Claim::AlreadyOpen {
            return Ok(());
        }
        return cosmic::app::run::<app::App>(shortcut::window_settings(), app::Mode::Shortcut);
    }

    cosmic::applet::run::<app::App>(app::Mode::Panel)
}
