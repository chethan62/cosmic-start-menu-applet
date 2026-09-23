//! The right-hand column: pinned tiles in named groups.
//!
//! Each group is a heading over a fixed-size box in which every tile is
//! pinned at the position `tile_layout::pack` gives it. Absolute placement
//! rather than `Grid`, which collapses Fill children and misplaces spans.

use std::collections::HashSet;

use cosmic::desktop::IconSourceExt;
use cosmic::iced::widget::{pin, stack};
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{
    button, column, container, icon, mouse_area, row, scrollable, text, text_input, Space,
};
use cosmic::Element;

use crate::app::{Message, Target};
use crate::apps::App;
use crate::config::{Config, TileFinish, TileRef, TileSize};
use crate::fl;
use crate::tile_layout::{self, COLUMNS};
use crate::ui::{quiet_button, selected_button, tile_button_class, Spacing};

/// One grid cell. Small tiles are one cell, Medium 2×2, Wide 4×2.
pub const CELL: f32 = 44.0;

fn span(cells: u16, gap: f32) -> f32 {
    CELL * f32::from(cells) + gap * f32::from(cells.saturating_sub(1))
}

/// The width a group's grid takes, for sizing the column.
pub fn grid_width(spacing: Spacing) -> f32 {
    span(COLUMNS, f32::from(spacing.gap))
}

/// Edit mode: tiles are picked up and dropped instead of launched.
#[derive(Debug, Clone, Copy, Default)]
pub struct Edit {
    pub on: bool,
    pub picked: Option<TileRef>,
}

fn tile<'a>(
    at: TileRef,
    app: &'a App,
    size: TileSize,
    (w, h): (f32, f32),
    finish: TileFinish,
    edit: Edit,
) -> Element<'a, Message> {
    let glyph = icon(app.icon.as_cosmic_icon()).size(match size {
        TileSize::Small => 24,
        TileSize::Medium | TileSize::Wide => 32,
    });
    let content: Element<'a, Message> = match size {
        TileSize::Small => container(glyph).center(Length::Fill).into(),
        // Name along the bottom edge, icon centred above it, as Windows does.
        TileSize::Medium | TileSize::Wide => column::with_children(vec![
            container(glyph)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into(),
            text::caption(&app.name)
                .wrapping(cosmic::iced::widget::text::Wrapping::None)
                .into(),
        ])
        .align_x(Alignment::Start)
        .into(),
    };
    let class = if edit.picked == Some(at) {
        selected_button()
    } else if edit.on {
        // An edge on every tile says "these move now" without a new colour.
        tile_button_class(TileFinish::Outline)
    } else {
        tile_button_class(finish)
    };
    let button = button::custom(content)
        .class(class)
        .padding(match size {
            TileSize::Small => [0, 0],
            TileSize::Medium | TileSize::Wide => [6, 8],
        })
        .width(Length::Fixed(w))
        .height(Length::Fixed(h))
        .on_press(if edit.on {
            Message::TileClicked(at)
        } else {
            Message::LaunchId(app.id.clone())
        });
    if edit.on {
        return button.into();
    }
    mouse_area(button)
        .on_right_press(Message::OpenContext(Target::Tile(at)))
        .into()
}

fn heading<'a>(g: usize, name: &'a str, spacing: Spacing, edit: Edit) -> Element<'a, Message> {
    let pad = [spacing.section, 0, spacing.pad_y, 2];
    if !edit.on {
        return container(text::heading(name)).padding(pad).into();
    }
    if edit.picked.is_some() {
        return container(
            button::custom(
                row::with_children(vec![
                    text::heading(name).into(),
                    Space::new().width(Length::Fill).into(),
                    text::caption(fl!("drop-here")).into(),
                ])
                .align_y(Alignment::Center),
            )
            .class(selected_button())
            .padding([4, 8])
            .width(Length::Fill)
            .on_press(Message::DropEnd(g)),
        )
        .padding(pad)
        .into();
    }
    container(
        row::with_children(vec![
            text_input::text_input(fl!("group-name"), name)
                .on_input(move |v| Message::RenameGroup(g, v))
                .width(Length::Fill)
                .into(),
            button::icon(icon::from_name("user-trash-symbolic"))
                .tooltip(fl!("group-remove"))
                .on_press(Message::RemoveGroup(g))
                .into(),
        ])
        .spacing(4)
        .align_y(Alignment::Center),
    )
    .padding(pad)
    .into()
}

pub fn view<'a>(
    config: &'a Config,
    apps: &'a [App],
    spacing: Spacing,
    edit: Edit,
) -> Element<'a, Message> {
    let gap = f32::from(spacing.gap);
    let installed: HashSet<&str> = apps.iter().map(|a| a.id.as_str()).collect();
    let mut groups = column::with_capacity(config.groups.len() * 2).spacing(spacing.gap);
    for (g, group) in config.groups.iter().enumerate() {
        let visible = config.visible_tiles(g, &installed);
        let sizes: Vec<TileSize> = visible.iter().map(|(_, t)| t.size).collect();
        let packed = tile_layout::pack(&sizes);
        let mut layer: Vec<Element<'a, Message>> = Vec::with_capacity(visible.len());
        for ((ti, t), p) in visible.iter().zip(packed.tiles.iter()) {
            let Some(app) = apps.iter().find(|a| a.id == t.app) else {
                continue;
            };
            let at = (
                f32::from(p.col) * (CELL + gap),
                f32::from(p.row) * (CELL + gap),
            );
            let dims = (span(p.cols, gap), span(p.rows, gap));
            layer.push(
                pin(tile((g, *ti), app, t.size, dims, config.finish, edit))
                    .x(at.0)
                    .y(at.1)
                    .into(),
            );
        }
        groups = groups.push(heading(g, &group.name, spacing, edit));
        if visible.is_empty() {
            groups = groups.push(text::caption(fl!("empty-group")));
            continue;
        }
        groups = groups.push(
            container(stack(layer))
                .width(Length::Fixed(grid_width(spacing)))
                .height(Length::Fixed(span(packed.rows.max(1), gap))),
        );
    }
    if edit.on {
        let (label, msg) = if edit.picked.is_some() {
            (fl!("drop-new-group"), Message::DropNew)
        } else {
            (fl!("add-group"), Message::AddGroup)
        };
        groups = groups.push(
            container(
                button::custom(text::body(label))
                    .class(quiet_button())
                    .padding([10, 10])
                    .width(Length::Fill)
                    .on_press(msg),
            )
            .padding([spacing.section, 0, 0, 0]),
        );
    }

    let toggle = button::custom(text::body(if edit.on {
        fl!("edit-done")
    } else {
        fl!("edit-tiles")
    }))
    .class(if edit.on {
        selected_button()
    } else {
        quiet_button()
    })
    .padding([4, 10])
    .on_press(Message::ToggleEdit);

    column::with_children(vec![
        row::with_children(vec![Space::new().width(Length::Fill).into(), toggle.into()]).into(),
        scrollable(groups).height(Length::Fill).into(),
    ])
    .width(Length::Fixed(grid_width(spacing) + 12.0))
    .into()
}
