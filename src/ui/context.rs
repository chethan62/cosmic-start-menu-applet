//! The right-click menu for an app row or a tile.

use cosmic::iced::{Alignment, Length};
use cosmic::widget::{button, column, container, divider, icon, row, text, Space};
use cosmic::Element;

use crate::app::{Context, Message, Target};
use crate::apps::App;
use crate::config::{Config, TileSize};
use crate::fl;
use crate::ui::{menu_card, quiet_button};

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
