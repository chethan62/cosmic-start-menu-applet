//! Start Menu — a Windows 10-style Start menu for the COSMIC panel.

mod app;
mod apps;
mod config;
mod favorites;
mod folders;
mod i18n;
mod launch;
mod launcher;
mod process;
mod remote;
mod search;
mod session;
mod settings;
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

    // `--toggle` is for keyboard shortcuts: open or close the running menu.
    if std::env::args().skip(1).any(|arg| arg == "--toggle") {
        if let Err(err) = remote::send_toggle() {
            eprintln!("{err}");
            std::process::exit(1);
        }
        return Ok(());
    }

    remote::serve();
    cosmic::applet::run::<app::App>(())
}
