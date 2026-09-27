//! The thin left strip: account at the top; Files, Settings and Power at the
//! bottom, like Windows 10. Icons only; the name is in the tooltip.

use cosmic::iced::{Alignment, Length, Point};
use cosmic::widget::{button, column, container, icon, popover, row, text, tooltip, Space};
use cosmic::Element;

use crate::app::{Avatar, Message};
use crate::fl;
use crate::session::{self, Power};
use crate::ui::{menu_card, quiet_button, row_radius, RAIL_WIDTH};

const BUTTON: f32 = 40.0;

fn rail_button<'a>(icon_name: &'static str, label: String, msg: Message) -> Element<'a, Message> {
    tooltip(
        button::custom(container(icon::from_name(icon_name).size(18)).center(Length::Fill))
            .class(quiet_button())
            .padding(0)
            .width(Length::Fixed(BUTTON))
            .height(Length::Fixed(BUTTON))
            .on_press(msg),
        text::body(label),
        tooltip::Position::Right,
    )
    .into()
}

fn power_menu<'a>() -> Element<'a, Message> {
    let items = session::ALL.iter().map(|&p: &Power| {
        button::custom(
            row::with_children(vec![
                icon::from_name(p.icon()).size(16).into(),
                text::body(fl!(p.l10n_key())).into(),
            ])
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .class(quiet_button())
        .padding([7, 10])
        .width(Length::Fill)
        .on_press(Message::Power(p))
        .into()
    });
    container(column::with_children(items.collect::<Vec<_>>()).spacing(1))
        .padding(6)
        .width(Length::Fixed(190.0))
        .class(menu_card())
        .into()
}

const AVATAR: f32 = 28.0;

/// Round gives a circle; Slightly round and Square give the small radius
/// (8 px and 2 px), so the picture matches every other corner on screen.
fn avatar_radius(theme: &cosmic::Theme) -> f32 {
    if theme.cosmic().corner_radii.radius_xl[0] >= AVATAR / 2.0 {
        AVATAR / 2.0
    } else {
        row_radius(theme)
    }
}

fn avatar<'a>(a: &Avatar) -> Element<'a, Message> {
    let inner: Element<'a, Message> = match &a.image {
        Some(handle) => cosmic::widget::image(handle.clone())
            .content_fit(cosmic::iced::ContentFit::Cover)
            .width(Length::Fixed(AVATAR))
            .height(Length::Fixed(AVATAR))
            .into(),
        None => text::body(a.initial.clone()).into(),
    };
    let has_image = a.image.is_some();
    let face = container(inner)
        .center(Length::Fixed(AVATAR))
        .clip(true)
        .class(cosmic::theme::Container::Custom(Box::new(move |theme| {
            let cosmic = theme.cosmic();
            cosmic::widget::container::Style {
                background: (!has_image)
                    .then(|| cosmic::iced::Background::Color(cosmic.accent_color().into())),
                text_color: Some(cosmic.on_accent_color().into()),
                border: cosmic::iced::Border {
                    radius: avatar_radius(theme).into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })));
    tooltip(
        button::custom(container(face).center(Length::Fill))
            .class(quiet_button())
            .padding(0)
            .width(Length::Fixed(BUTTON))
            .height(Length::Fixed(BUTTON))
            .on_press(Message::OpenAccount),
        text::body(fl!("rail-account")),
        tooltip::Position::Right,
    )
    .into()
}

pub fn view<'a>(power_open: bool, account: &Avatar) -> Element<'a, Message> {
    let power = rail_button(
        "system-shutdown-symbolic",
        fl!("power"),
        Message::PowerMenu(!power_open),
    );
    // Anchored at the button's bottom-right; popover flips it upward when it
    // would run off the bottom of the popup, which at the rail's foot it does.
    let mut power = popover(power)
        .position(popover::Position::Point(Point::new(BUTTON + 4.0, BUTTON)))
        .on_close(Message::PowerMenu(false));
    if power_open {
        power = power.popup(power_menu());
    }

    column::with_children(vec![
        avatar(account),
        Space::new().height(Length::Fill).into(),
        rail_button("folder-symbolic", fl!("rail-files"), Message::OpenFiles),
        rail_button(
            "preferences-system-symbolic",
            fl!("rail-settings"),
            Message::OpenSettingsApp,
        ),
        power.into(),
    ])
    .spacing(4)
    .align_x(Alignment::Center)
    .width(Length::Fixed(RAIL_WIDTH))
    .height(Length::Fill)
    .into()
}
