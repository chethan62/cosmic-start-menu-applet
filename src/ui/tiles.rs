//! The right-hand column: pinned tiles in named groups.
//!
//! Each group is a heading over a fixed-size box in which every tile is
//! pinned at the position `tile_layout::pack` gives it. Absolute placement
//! rather than `Grid`, which collapses Fill children and misplaces spans.

use std::collections::HashSet;

use cosmic::desktop::IconSourceExt;
use cosmic::iced::widget::{pin, stack};
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{button, column, container, icon, scrollable, text};
use cosmic::Element;

use crate::app::Message;
use crate::apps::App;
use crate::config::{Config, TileFinish, TileSize};
use crate::tile_layout::{self, COLUMNS};
use crate::ui::{tile_button_class, Spacing};

/// One grid cell. Small tiles are one cell, Medium 2×2, Wide 4×2.
pub const CELL: f32 = 44.0;

fn span(cells: u16, gap: f32) -> f32 {
    CELL * f32::from(cells) + gap * f32::from(cells.saturating_sub(1))
}

/// The width a group's grid takes, for sizing the column.
pub fn grid_width(spacing: Spacing) -> f32 {
    span(COLUMNS, f32::from(spacing.gap))
}

fn tile<'a>(
    app: &'a App,
    size: TileSize,
    (w, h): (f32, f32),
    finish: TileFinish,
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
    button::custom(content)
        .class(tile_button_class(finish))
        .padding(match size {
            TileSize::Small => [0, 0],
            TileSize::Medium | TileSize::Wide => [6, 8],
        })
        .width(Length::Fixed(w))
        .height(Length::Fixed(h))
        .on_press(Message::LaunchId(app.id.clone()))
        .into()
}

pub fn view<'a>(config: &'a Config, apps: &'a [App], spacing: Spacing) -> Element<'a, Message> {
    let gap = f32::from(spacing.gap);
    let installed: HashSet<&str> = apps.iter().map(|a| a.id.as_str()).collect();
    let mut groups = column::with_capacity(config.groups.len() * 2).spacing(spacing.gap);
    for (g, group) in config.groups.iter().enumerate() {
        let visible = config.visible_tiles(g, &installed);
        let sizes: Vec<TileSize> = visible.iter().map(|(_, t)| t.size).collect();
        let packed = tile_layout::pack(&sizes);
        let mut layer: Vec<Element<'a, Message>> = Vec::with_capacity(visible.len());
        for ((_, t), p) in visible.iter().zip(packed.tiles.iter()) {
            let Some(app) = apps.iter().find(|a| a.id == t.app) else {
                continue;
            };
            let at = (
                f32::from(p.col) * (CELL + gap),
                f32::from(p.row) * (CELL + gap),
            );
            let dims = (span(p.cols, gap), span(p.rows, gap));
            layer.push(
                pin(tile(app, t.size, dims, config.finish))
                    .x(at.0)
                    .y(at.1)
                    .into(),
            );
        }
        groups = groups
            .push(container(text::heading(&group.name)).padding([
                spacing.section,
                0,
                spacing.pad_y,
                2,
            ]))
            .push(
                container(stack(layer))
                    .width(Length::Fixed(grid_width(spacing)))
                    .height(Length::Fixed(span(packed.rows.max(1), gap))),
            );
    }
    scrollable(groups).height(Length::Fill).into()
}
