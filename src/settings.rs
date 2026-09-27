//! The Settings window: `cosmic-start-menu-applet --settings`, opened by
//! right-clicking the panel button. Every change is saved at once; the menu
//! re-reads the config each time it opens.

use cosmic::app::{Core, Task};
use cosmic::iced::{Alignment, Length, Subscription};
use cosmic::widget::{button, column, container, divider, radio, row, scrollable, text, toggler};
use cosmic::{Application, ApplicationExt, Element};

use crate::config::{Config, MenuPosition, Search, TileFinish};
use crate::fl;

const WINDOW_WIDTH: f32 = 440.0;
const WINDOW_HEIGHT: f32 = 640.0;

pub struct Settings {
    core: Core,
    config: Config,
    error: Option<String>,
    confirm_reset: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    SetFinish(TileFinish),
    SetMostUsed(bool),
    SetColumns(u8),
    SetPosition(MenuPosition),
    SetSearch(Search),
    SetIcon(String),
    AskReset(bool),
    Reset,
    Present,
    Close,
}

impl Settings {
    /// Change one thing in the file as it is now, so edits the menu made
    /// while this window was open (pins, moves, modes) are kept.
    fn change(&mut self, f: impl FnOnce(&mut Config)) {
        match Config::update(f) {
            Ok(saved) => {
                self.config = saved;
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
    }
}

impl Application for Settings {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    type Message = Message;

    const APP_ID: &'static str = "io.github.jjnuthuagen.StartMenuSettings";

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, _flags: ()) -> (Self, Task<Message>) {
        let mut settings = Self {
            core,
            config: Config::load(),
            error: None,
            confirm_reset: false,
        };
        settings.set_header_title(fl!("settings-title"));
        let title = match settings.core.main_window_id() {
            Some(id) => settings.set_window_title(fl!("settings-title"), id),
            None => Task::none(),
        };
        (settings, title)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SetFinish(finish) => {
                self.change(|c| c.finish = finish);
            }
            Message::SetColumns(n) => {
                self.change(|c| c.tile_columns = n);
            }
            Message::SetPosition(p) => {
                self.change(|c| c.menu_position = p);
            }
            Message::SetSearch(search) => {
                self.change(|c| c.search = search);
            }
            Message::SetIcon(name) => {
                self.change(|c| c.panel_icon = name);
            }
            Message::SetMostUsed(on) => {
                self.change(|c| c.show_most_used = on);
            }
            Message::AskReset(ask) => self.confirm_reset = ask,
            Message::Reset => {
                self.confirm_reset = false;
                let installed: Vec<String> =
                    crate::apps::load_all().into_iter().map(|a| a.id).collect();
                let favorites = crate::config::favorites_text();
                let groups = Config::seeded(&installed, favorites.as_deref()).groups;
                self.change(|c| c.groups = groups);
            }
            Message::Present => {
                // Raise the open window instead of opening a second one.
                if let Some(id) = self.core.main_window_id() {
                    return cosmic::iced::window::gain_focus(id);
                }
            }
            Message::Close => return cosmic::iced::exit(),
        }
        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        // A later `--settings` invocation asking this window to come forward.
        Subscription::run_with((), |()| {
            futures::stream::unfold(crate::single_instance::requests(), |requests| async move {
                let mut receiver = requests?;
                receiver
                    .recv()
                    .await
                    .map(|()| (Message::Present, Some(receiver)))
            })
        })
    }

    fn view(&self) -> Element<'_, Message> {
        let space = self.core.system_theme().cosmic().spacing.space_s;

        let mut finish = column::with_capacity(4)
            .spacing(space / 2)
            .push(text::title4(fl!("settings-finish")));
        for (value, key) in [
            (TileFinish::Frosted, "finish-frosted"),
            (TileFinish::Solid, "finish-solid"),
            (TileFinish::Outline, "finish-outline"),
            (TileFinish::Accent, "finish-accent"),
        ] {
            finish = finish.push(radio(
                text::body(fl!(key)),
                value,
                Some(self.config.finish),
                Message::SetFinish,
            ));
        }

        let current = if self.config.tile_columns == 2 { 2 } else { 3 };
        let mut columns = column::with_capacity(3)
            .spacing(space / 2)
            .push(text::title4(fl!("settings-columns")));
        for (value, key) in [(3u8, "columns-three"), (2u8, "columns-two")] {
            columns = columns.push(radio(
                text::body(fl!(key)),
                value,
                Some(current),
                Message::SetColumns,
            ));
        }

        let mut position = column::with_capacity(5)
            .spacing(space / 2)
            .push(text::title4(fl!("settings-position")));
        for (value, key) in [
            (MenuPosition::Corner, "pos-corner"),
            (MenuPosition::Top, "pos-top"),
            (MenuPosition::Centre, "pos-centre"),
        ] {
            position = position.push(radio(
                text::body(fl!(key)),
                value,
                Some(self.config.menu_position),
                Message::SetPosition,
            ));
        }
        position = position.push(text::caption(fl!("settings-position-hint")));

        let search = self.config.search;
        let mut sections = column::with_capacity(6)
            .spacing(space / 2)
            .push(text::title4(fl!("settings-search")));
        type Get = fn(&mut Search) -> &mut bool;
        let toggles: [(&str, Get); 5] = [
            ("toggle-windows", |s| &mut s.windows),
            ("toggle-calc", |s| &mut s.calculator),
            ("toggle-files", |s| &mut s.files),
            ("toggle-commands", |s| &mut s.commands),
            ("toggle-web", |s| &mut s.web),
        ];
        for (key, get) in toggles {
            let mut when_on = search;
            *get(&mut when_on) = true;
            let mut when_off = search;
            *get(&mut when_off) = false;
            sections = sections.push(
                row::with_capacity(2)
                    .align_y(Alignment::Center)
                    .push(text::body(fl!(key)).width(Length::Fill))
                    .push(toggler(*get(&mut { search })).on_toggle(move |on| {
                        Message::SetSearch(if on { when_on } else { when_off })
                    })),
            );
        }

        let icon_field = cosmic::widget::text_input::text_input(
            fl!("settings-icon-hint"),
            &self.config.panel_icon,
        )
        .on_input(Message::SetIcon);
        let mut icon_pick = column::with_capacity(4)
            .spacing(space / 2)
            .push(text::title4(fl!("settings-icon")));
        for (name, key) in [
            ("start-here-symbolic", "icon-cosmic"),
            ("view-app-grid-symbolic", "icon-grid"),
            ("open-menu-symbolic", "icon-menu"),
        ] {
            icon_pick = icon_pick.push(radio(
                text::body(fl!(key)),
                name,
                Some(self.config.panel_icon.as_str()),
                |n| Message::SetIcon(n.to_owned()),
            ));
        }
        icon_pick = icon_pick.push(icon_field);

        let most_used = row::with_capacity(2)
            .align_y(Alignment::Center)
            .push(text::body(fl!("settings-most-used")).width(Length::Fill))
            .push(toggler(self.config.show_most_used).on_toggle(Message::SetMostUsed));

        let reset: Element<'_, Message> = if self.confirm_reset {
            row::with_capacity(3)
                .spacing(space)
                .align_y(Alignment::Center)
                .push(text::body(fl!("settings-reset-confirm")).width(Length::Fill))
                .push(button::standard(fl!("settings-cancel")).on_press(Message::AskReset(false)))
                .push(button::destructive(fl!("settings-reset-yes")).on_press(Message::Reset))
                .into()
        } else {
            column::with_capacity(2)
                .spacing(space / 2)
                .push(button::destructive(fl!("settings-reset")).on_press(Message::AskReset(true)))
                .push(text::caption(fl!("settings-reset-hint")))
                .into()
        };

        let status = match &self.error {
            Some(e) => text::caption(fl!("settings-save-failed", reason = e.clone())),
            None => text::caption(fl!("settings-saved")),
        };

        let body = column::with_capacity(16)
            .spacing(space)
            .padding(space)
            .push(finish)
            .push(divider::horizontal::default())
            .push(columns)
            .push(divider::horizontal::default())
            .push(position)
            .push(divider::horizontal::default())
            .push(sections)
            .push(divider::horizontal::default())
            .push(icon_pick)
            .push(divider::horizontal::default())
            .push(most_used)
            .push(divider::horizontal::default())
            .push(reset)
            .push(status)
            .push(
                row::with_capacity(2)
                    .push(cosmic::widget::Space::new().width(Length::Fill))
                    .push(button::suggested(fl!("close")).on_press(Message::Close)),
            );

        container(scrollable(body))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

/// Window settings for `--settings`.
pub fn window_settings() -> cosmic::app::Settings {
    cosmic::app::Settings::default()
        .size(cosmic::iced::Size::new(WINDOW_WIDTH, WINDOW_HEIGHT))
        .resizable(Some(8.0))
        .debug(false)
}

/// Spawn this binary with `--settings`. A separate process, because an
/// applet is a layer-shell client and a normal window does not belong in its
/// event loop.
pub fn open_window() {
    let Ok(executable) = std::env::current_exe() else {
        tracing::error!("could not determine our own path; cannot open Settings");
        return;
    };
    let mut command = std::process::Command::new(executable);
    command.arg("--settings");
    if let Err(err) = crate::process::spawn_and_reap(command) {
        tracing::error!("could not open Settings: {err}");
    }
}
