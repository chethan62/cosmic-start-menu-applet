//! The applet: a panel button and the Start menu popup it opens.

use cosmic::app::{Core, Task};
use cosmic::iced::window::{self, Id};
use cosmic::iced::{Length, Limits};
use cosmic::widget::{container, text};
use cosmic::{Application, Element};

use crate::fl;

/// Rail + list + a six-cell tile column, with padding. Fixed like Windows 10's
/// menu; each column scrolls inside it.
pub const POPUP_WIDTH: f32 = 680.0;
pub const POPUP_HEIGHT: f32 = 600.0;

pub struct App {
    core: Core,
    popup: Option<Id>,
}

#[derive(Debug, Clone)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
}

impl Application for App {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    type Message = Message;

    const APP_ID: &'static str = "io.github.jjnuthuagen.StartMenu";

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, _flags: ()) -> (Self, Task<Message>) {
        (Self { core, popup: None }, Task::none())
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::TogglePopup => {
                if let Some(id) = self.popup.take() {
                    return cosmic::iced::platform_specific::shell::commands::popup::destroy_popup(
                        id,
                    );
                }
                let id = window::Id::unique();
                self.popup = Some(id);
                let mut settings = self.core.applet.get_popup_settings(
                    self.core.main_window_id().unwrap_or(id),
                    id,
                    None,
                    None,
                    None,
                );
                settings.positioner.size_limits = Limits::NONE
                    .min_width(POPUP_WIDTH)
                    .max_width(POPUP_WIDTH)
                    .min_height(POPUP_HEIGHT)
                    .max_height(POPUP_HEIGHT);
                let popup =
                    cosmic::iced::platform_specific::shell::commands::popup::get_popup(settings);

                // libcosmic only blurs surfaces it tracks in `surface_views`,
                // and a popup made with `get_popup` is not one of them, so the
                // theme's frosted styling would give a translucent popup with
                // nothing blurred behind it. Ask for the blur ourselves, and
                // through the Wayland command rather than `window::enable_blur`:
                // the popup's surface does not exist yet in this batch, and
                // only the Wayland path parks the request until it does.
                // (Same fix as cosmic-control-center-applet.)
                if self.core.frosted(self.core.system_theme().cosmic()) {
                    let blur = cosmic::iced::platform_specific::shell::commands::blur::blur(
                        id,
                        Some(vec![cosmic::iced::Rectangle {
                            x: 0.0,
                            y: 0.0,
                            width: f32::MAX,
                            height: f32::MAX,
                        }]),
                    );
                    Task::batch([popup, blur.discard()])
                } else {
                    popup
                }
            }
            Message::PopupClosed(id) => {
                if self.popup == Some(id) {
                    self.popup = None;
                }
                Task::none()
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        self.core
            .applet
            .icon_button("start-here-symbolic")
            .on_press(Message::TogglePopup)
            .into()
    }

    fn view_window(&self, _id: Id) -> Element<'_, Message> {
        let body = container(text(fl!("app-name")))
            .width(Length::Fixed(POPUP_WIDTH))
            .height(Length::Fixed(POPUP_HEIGHT));
        self.core.applet.popup_container(body).into()
    }
}
