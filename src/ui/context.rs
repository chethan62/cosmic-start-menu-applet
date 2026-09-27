//! The right-click menu for an app row or a tile.

use cosmic::iced::{Alignment, Length};
use cosmic::widget::{button, column, container, divider, icon, row, text, Space};
use cosmic::Element;

use crate::app::{Context, Message, Target};
use crate::apps::App;
use crate::config::{Config, TileRef, TileSize};
use crate::fl;
use crate::ui::{menu_card, parse_hex, quiet_button, row_radius};

fn item<'a>(icon_name: &'static str, label: String, msg: Message) -> Element<'a, Message> {
    button::custom(
        row::with_children(vec![
            icon::from_name(icon_name).size(16).into(),
            text::body(label).into(),
        ])
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .class(quiet_button())
    .padding([7, 10])
    .width(Length::Fill)
    .on_press(msg)
    .into()
}

fn checked<'a>(label: String, on: bool, msg: Message) -> Element<'a, Message> {
    button::custom(
        row::with_children(vec![
            text::body(label).into(),
            Space::new().width(Length::Fill).into(),
            if on {
                icon::from_name("object-select-symbolic").size(14).into()
            } else {
                Space::new().width(14).into()
            },
        ])
        .align_y(Alignment::Center),
    )
    .class(quiet_button())
    .padding([7, 10])
    .width(Length::Fill)
    .on_press(msg)
    .into()
}

fn label<'a>(s: String) -> Element<'a, Message> {
    container(text::caption(s)).padding([6, 10, 2, 10]).into()
}

/// The Windows 10 tile palette, near enough. The first slot is "no colour":
/// back to the theme's finish.
const SWATCHES: &[&str] = &[
    "#E81123", "#F7630C", "#FFB900", "#107C10", "#00B7C3", "#0078D7", "#886CE4", "#4A4A4A",
];

const SWATCH: f32 = 22.0;

/// One colour well: filled with its colour, ringed when it is the tile's.
fn swatch<'a>(at: TileRef, hex: Option<&'static str>, current: bool) -> Element<'a, Message> {
    let fill = hex.and_then(parse_hex);
    button::custom(Space::new())
        .class(button::ButtonClass::Custom {
            active: Box::new(move |_f, theme| swatch_style(theme, fill, current, false)),
            disabled: Box::new(move |theme| swatch_style(theme, fill, current, false)),
            hovered: Box::new(move |_f, theme| swatch_style(theme, fill, current, true)),
            pressed: Box::new(move |_f, theme| swatch_style(theme, fill, current, true)),
        })
        .padding(0)
        .width(Length::Fixed(SWATCH))
        .height(Length::Fixed(SWATCH))
        .on_press(Message::TileColor(at, hex.map(str::to_owned)))
        .into()
}

fn swatch_style(
    theme: &cosmic::Theme,
    fill: Option<cosmic::iced::Color>,
    current: bool,
    hovered: bool,
) -> button::Style {
    let cosmic = theme.cosmic();
    // The blank well shows as an outline; a ring marks the tile's own pick,
    // and hover thickens the edge so the wells answer the pointer.
    let edge = if current {
        cosmic.accent_color().into()
    } else {
        cosmic.background(theme.transparent).component.divider.into()
    };
    button::Style {
        background: fill.map(cosmic::iced::Background::Color),
        border_radius: row_radius(theme).into(),
        border_width: if current || hovered { 2.0 } else { 1.0 },
        border_color: edge,
        ..button::Style::new()
    }
}

pub fn view<'a>(
    ctx: &Context,
    apps: &'a [App],
    config: &'a Config,
    favs: &[String],
) -> Option<Element<'a, Message>> {
    let mut items: Vec<Element<'a, Message>> = Vec::new();
    match ctx.target {
        Target::App(i) => {
            let app = apps.get(i)?;
            items.push(if config.is_pinned(&app.id) {
                item(
                    "view-pin-symbolic",
                    fl!("ctx-unpin"),
                    Message::Unpin(app.id.clone()),
                )
            } else {
                item(
                    "view-pin-symbolic",
                    fl!("ctx-pin"),
                    Message::Pin(app.id.clone()),
                )
            });
            items.push(if favs.contains(&app.id) {
                item(
                    "starred-symbolic",
                    fl!("fav-remove"),
                    Message::RemoveFavourite(app.id.clone()),
                )
            } else {
                item(
                    "non-starred-symbolic",
                    fl!("fav-add"),
                    Message::AddFavourite(app.id.clone()),
                )
            });
            if !app.actions.is_empty() {
                items.push(divider::horizontal::light().into());
                for (k, action) in app.actions.iter().enumerate() {
                    items.push(item(
                        "media-playback-start-symbolic",
                        action.name.clone(),
                        Message::RunAction(i, k),
                    ));
                }
            }
        }
        Target::Tile((g, t)) => {
            let tile = config.groups.get(g)?.tiles.get(t)?;
            items.push(item(
                "view-pin-symbolic",
                fl!("ctx-unpin"),
                Message::Unpin(tile.app.clone()),
            ));
            items.push(item(
                "document-edit-symbolic",
                fl!("ctx-rename"),
                Message::RenameTile((g, t)),
            ));
            items.push(divider::horizontal::light().into());
            items.push(label(fl!("ctx-colour")));
            let current = tile.color.as_deref();
            let mut wells: Vec<Element<'a, Message>> =
                vec![swatch((g, t), None, current.is_none())];
            wells.extend(
                SWATCHES
                    .iter()
                    .map(|&hex| swatch((g, t), Some(hex), current == Some(hex))),
            );
            let mut colour_rows = column::with_capacity(2).spacing(4);
            let mut wells = wells.into_iter().peekable();
            while wells.peek().is_some() {
                let mut r = row::with_capacity(5).spacing(4);
                for _ in 0..5 {
                    if let Some(w) = wells.next() {
                        r = r.push(w);
                    }
                }
                colour_rows = colour_rows.push(r);
            }
            items.push(container(colour_rows).padding([2, 10, 4, 10]).into());
            items.push(if tile.image.is_some() {
                item(
                    "image-x-generic-symbolic",
                    fl!("ctx-image-change"),
                    Message::EditTileImage((g, t)),
                )
            } else {
                item(
                    "image-x-generic-symbolic",
                    fl!("ctx-image-set"),
                    Message::EditTileImage((g, t)),
                )
            });
            if tile.image.is_some() {
                items.push(item(
                    "edit-clear-symbolic",
                    fl!("ctx-image-remove"),
                    Message::ClearTileImage((g, t)),
                ));
            }
            items.push(divider::horizontal::light().into());
            items.push(label(fl!("ctx-resize")));
            for (size, key) in [
                (TileSize::Small, "size-small"),
                (TileSize::Medium, "size-medium"),
                (TileSize::Wide, "size-wide"),
            ] {
                items.push(checked(
                    fl!(key),
                    tile.size == size,
                    Message::Resize((g, t), size),
                ));
            }
            items.push(divider::horizontal::light().into());
            items.push(label(fl!("ctx-move")));
            for (other, group) in config.groups.iter().enumerate() {
                if other != g {
                    items.push(item(
                        "go-next-symbolic",
                        group.name.clone(),
                        Message::MoveToGroup((g, t), other),
                    ));
                }
            }
            items.push(item(
                "list-add-symbolic",
                fl!("ctx-new-group"),
                Message::NewGroupWith((g, t)),
            ));
        }
    }
    Some(
        container(column::with_children(items).spacing(1))
            .padding(6)
            .width(Length::Fixed(220.0))
            .class(menu_card())
            .into(),
    )
}
