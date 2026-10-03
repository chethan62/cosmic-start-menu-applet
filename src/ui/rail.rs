//! The thin left strip: account at the top; Files, Settings and Power at the
//! bottom, like Windows 10. Icons only; the name is in the tooltip.

use cosmic::iced::{Alignment, Length, Point};
use cosmic::widget::{button, column, container, icon, popover, row, text, tooltip, Space};
use cosmic::Element;

use crate::app::{Avatar, Message};
use crate::config::{Slot, SystemPanel};
use crate::fl;
use crate::session::{self, Power};
use crate::ui::{menu_card, quiet_button, row_radius, selected_button, v_hairline, RAIL_WIDTH};

const BUTTON: f32 = 40.0;
/// Vertical rhythm between rail glyphs, and the pad at the rail's ends.
const RHYTHM: u16 = 8;
const RAIL_PAD: u16 = 12;
/// Air between the avatar and the icon cluster under it. The rail used to
/// push them apart with all the slack in the column, which left the avatar
/// marooned at the top of an empty strip.
const AVATAR_GAP: f32 = 16.0;

/// A rail button, top to bottom in the order `view` draws them. The keyboard
/// walks this list, so it is the list `view` itself is built from — the two
/// cannot drift apart into a highlight that lands on the wrong glyph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
    Account,
    Slot(Slot),
    Settings,
    Power,
}

pub fn items(panel: &SystemPanel) -> Vec<Item> {
    let mut out = vec![Item::Account];
    out.extend(
        crate::config::SLOTS
            .into_iter()
            .filter(|&s| panel.shown(s))
            .map(Item::Slot),
    );
    out.push(Item::Settings);
    out.push(Item::Power);
    out
}

/// What pressing a rail button does. Power is a toggle, so it needs to know
/// whether its menu is already up.
pub fn message(item: Item, power_open: bool) -> Message {
    match item {
        Item::Account => Message::OpenAccount,
        Item::Slot(slot) => Message::OpenSlot(slot),
        Item::Settings => Message::OpenSettingsApp,
        Item::Power => Message::PowerMenu(!power_open),
    }
}

fn rail_button<'a>(
    icon_name: &'static str,
    label: String,
    msg: Message,
    selected: bool,
) -> Element<'a, Message> {
    tooltip(
        button::custom(container(icon::from_name(icon_name).size(18)).center(Length::Fill))
            .class(if selected {
                selected_button()
            } else {
                quiet_button()
            })
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

const AVATAR: f32 = 32.0;

/// Round gives a circle; Slightly round and Square give the small radius
/// (8 px and 2 px), so the picture matches every other corner on screen.
fn avatar_radius(theme: &cosmic::Theme) -> f32 {
    if theme.cosmic().corner_radii.radius_xl[0] >= AVATAR / 2.0 {
        AVATAR / 2.0
    } else {
        row_radius(theme)
    }
}

fn avatar<'a>(a: &Avatar, selected: bool) -> Element<'a, Message> {
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
    // The picture *is* the control: no rounded plate behind it, which read
    // as a stuck selected state at the top of the rail.
    tooltip(
        button::custom(face)
            .class(if selected {
                selected_button()
            } else {
                quiet_button()
            })
            .padding(0)
            .width(Length::Fixed(AVATAR))
            .height(Length::Fixed(AVATAR))
            .on_press(Message::OpenAccount),
        text::body(fl!("rail-account")),
        tooltip::Position::Right,
    )
    .into()
}

/// The symbolic glyph and label key for a default-app slot.
pub fn slot_face(slot: Slot) -> (&'static str, &'static str) {
    match slot {
        Slot::Browser => ("web-browser-symbolic", "rail-browser"),
        Slot::Files => ("folder-symbolic", "rail-files"),
        Slot::Terminal => ("utilities-terminal-symbolic", "rail-terminal"),
        Slot::TaskManager => ("utilities-system-monitor-symbolic", "rail-task-manager"),
    }
}

pub fn view<'a>(
    power_open: bool,
    account: &Avatar,
    panel: &SystemPanel,
    focus: Option<usize>,
) -> Element<'a, Message> {
    // Avatar, then the default-app cluster right under it, then all the
    // slack, then Settings and Power as one bottom cluster. Built by walking
    // `items`, so the keyboard's n'th rail button is this one.
    let mut col = column::with_capacity(8);
    for (n, item) in items(panel).into_iter().enumerate() {
        let on = Some(n) == focus;
        let msg = message(item, power_open);
        col = match item {
            Item::Account => col
                .push(avatar(account, on))
                .push(Space::new().height(Length::Fixed(AVATAR_GAP - f32::from(RHYTHM)))),
            Item::Slot(slot) => {
                let (glyph, key) = slot_face(slot);
                col.push(rail_button(glyph, fl!(key), msg, on))
            }
            Item::Settings => col
                // All the slack above the bottom cluster.
                .push(Space::new().height(Length::Fill))
                .push(rail_button(
                    "preferences-system-symbolic",
                    fl!("rail-settings"),
                    msg,
                    on,
                )),
            // Power is hard-anchored at the foot of the rail. Its menu is
            // anchored at the button's bottom-right; popover flips it upward
            // when it would run off the bottom of the popup, which at the
            // rail's foot it does.
            Item::Power => {
                let power = rail_button("system-shutdown-symbolic", fl!("power"), msg, on);
                let mut power = popover(power)
                    .position(popover::Position::Point(Point::new(BUTTON + 4.0, BUTTON)))
                    .on_close(Message::PowerMenu(false));
                if power_open {
                    power = power.popup(power_menu());
                }
                col.push(power)
            }
        };
    }
    let col = col
        .spacing(RHYTHM)
        .align_x(Alignment::Center)
        .width(Length::Fill)
        .height(Length::Fill);
    // The rail is part of the popup's frame: no surface of its own, no
    // corner radius of its own, separated from the list by one hairline.
    row::with_children(vec![
        container(col)
            .padding([RAIL_PAD, 0, RAIL_PAD, 0])
            .width(Length::Fixed(RAIL_WIDTH))
            .height(Length::Fill)
            .into(),
        v_hairline(),
    ])
    .height(Length::Fill)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rail_lists_account_shown_slots_settings_and_power() {
        let mut panel = SystemPanel::default();
        for slot in crate::config::SLOTS {
            panel.set_shown(slot, true);
        }
        let all = items(&panel);
        assert_eq!(all.first(), Some(&Item::Account));
        assert_eq!(all.last(), Some(&Item::Power));
        assert_eq!(all[all.len() - 2], Item::Settings);
        assert_eq!(all.len(), crate::config::SLOTS.len() + 3);
        // A hidden slot leaves the list, so the keyboard cannot land on a
        // button that is not drawn.
        panel.set_shown(Slot::Terminal, false);
        let fewer = items(&panel);
        assert_eq!(fewer.len(), all.len() - 1);
        assert!(!fewer.contains(&Item::Slot(Slot::Terminal)));
    }

    #[test]
    fn a_rail_item_reuses_the_action_its_button_already_had() {
        assert!(matches!(
            message(Item::Account, false),
            Message::OpenAccount
        ));
        assert!(matches!(
            message(Item::Settings, false),
            Message::OpenSettingsApp
        ));
        assert!(matches!(
            message(Item::Slot(Slot::Files), false),
            Message::OpenSlot(Slot::Files)
        ));
        // Power toggles its own menu, as the button always did.
        assert!(matches!(
            message(Item::Power, false),
            Message::PowerMenu(true)
        ));
        assert!(matches!(
            message(Item::Power, true),
            Message::PowerMenu(false)
        ));
    }
}
