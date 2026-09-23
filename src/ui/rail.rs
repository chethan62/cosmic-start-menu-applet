//! The thin left strip: account at the top; Files, Settings and Power at the
//! bottom, like Windows 10. Icons only; the name is in the tooltip.

use cosmic::iced::{Alignment, Length, Point};
use cosmic::widget::{button, column, container, icon, popover, row, text, tooltip, Space};
use cosmic::Element;

use crate::app::Message;
use crate::fl;
use crate::session::{self, Power};
use crate::ui::{menu_card, quiet_button, RAIL_WIDTH};

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

pub fn view<'a>(power_open: bool) -> Element<'a, Message> {
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
        rail_button(
            "avatar-default-symbolic",
            fl!("rail-account"),
            Message::OpenAccount,
        ),
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
