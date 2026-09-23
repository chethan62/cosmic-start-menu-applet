//! Start Menu — a Windows 10-style Start menu for the COSMIC panel.

// Modules are built bottom-up before the popup uses them; removed once it does.
#![allow(dead_code)]

mod app;
mod apps;
mod i18n;
mod process;

fn main() -> cosmic::iced::Result {
    // `RUST_LOG=debug` shows why an app was skipped or a file was unreadable.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();
    cosmic::applet::run::<app::App>(())
}
