//! Start Menu — a Windows 10-style Start menu for the COSMIC panel.

mod app;
mod apps;
mod brand;
mod config;
mod fade;
mod favorites;
mod folders;
mod gif;
mod i18n;
mod instance;
mod keynav;
mod launch;
mod launcher;
mod motion;
mod process;
mod remote;
mod search;
mod session;
mod settings;
mod shortcut;
mod single_instance;
mod tile_layout;
mod tileimage;
mod tilemotion;
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

    // `--toggle` is for keyboard shortcuts and the panel button: the menu as
    // its own surface, so it gets the keyboard; or, if one is open, close it.
    // `--prewarm` is the same process started early and left closed.
    let args: Vec<String> = std::env::args().skip(1).collect();
    match remote::start_mode(args.iter().map(String::as_str)) {
        remote::Start::Toggle => {
            if remote::claim() == remote::Claim::AlreadyOpen {
                return Ok(());
            }
            return cosmic::app::run::<app::App>(
                shortcut::window_settings(),
                app::Mode::Shortcut { shown: true },
            );
        }
        remote::Start::Prewarm => {
            // Claimed without poking: poking would open a menu nobody asked
            // for, at login.
            if remote::claim_silently() == remote::Claim::AlreadyOpen {
                return Ok(());
            }
            return cosmic::app::run::<app::App>(
                shortcut::window_settings(),
                app::Mode::Shortcut { shown: false },
            );
        }
        remote::Start::No => {}
    }

    cosmic::applet::run::<app::App>(app::Mode::Panel)
}
