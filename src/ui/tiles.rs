//! The right-hand column: pinned tiles in named groups.
//!
//! Each group is a heading over a fixed-size box in which every tile is
//! pinned at the position `tile_layout::pack` gives it. Absolute placement
//! rather than `Grid`, which collapses Fill children and misplaces spans.

use std::collections::HashSet;

use cosmic::desktop::IconSourceExt;
use cosmic::iced::widget::{pin, stack};
use cosmic::iced::{Alignment, Length, Point};
use cosmic::widget::{
    button, column, container, icon, mouse_area, popover, row, scrollable, text, text_input, Space,
};
use cosmic::Element;

use crate::app::{Message, Target, TileField};
use crate::apps::App;
use crate::config::{Config, RightSide, TileFinish, TileRef, TileSize};
use crate::fl;
use crate::tile_layout;
use crate::ui::{
    colored_tile_class, header_text, image_tile_class, menu_card, parse_hex, quiet_button,
    selected_button, thin_scroll, tile_button_class, Spacing, SCROLL_GUTTER,
};

/// One grid cell. Small tiles are one cell, Medium 2×2, Wide 4×2. Every
/// tile is a whole number of these, so no tile is ever sized by its label
/// and every row in a group shares the same column edges. Trimmed from 46
/// to 42 so the wider app list does not push the popup past ~770 px.
pub const CELL: f32 = 42.0;

/// How far below a tile's top edge its icon starts.
const TILE_TOP: u16 = 16;
/// Inset of a tile's name from the left and bottom edges.
const TILE_INSET: u16 = 8;
/// Air above and below a group heading.
const HEADING_ABOVE: u16 = 20;
const HEADING_BELOW: u16 = 10;
/// Icon sizes: a 1×1 tile carries the icon alone, so it gets a bigger one
/// than the icon-over-a-name layout a 2×2 uses.
const ICON_SMALL: u16 = 32;
const ICON_LARGE: u16 = 48;

fn span(cells: u16, gap: f32) -> f32 {
    CELL * f32::from(cells) + gap * f32::from(cells.saturating_sub(1))
}

/// The width a group's grid takes, for sizing the column.
pub fn grid_width(spacing: Spacing, cells: u16) -> f32 {
    span(cells, f32::from(spacing.gap))
}

/// The whole right-hand column: the grid plus a gutter the scroll thumb
/// lives in. Reserved here *and* padded inside the scroll view, because an
/// overlay thumb otherwise draws straight over the rightmost tiles and
/// makes them read as clipped.
pub fn column_width(spacing: Spacing, cells: u16) -> f32 {
    grid_width(spacing, cells) + f32::from(SCROLL_GUTTER)
}

/// The rename input's id, so opening it can focus it.
pub fn rename_input_id() -> cosmic::widget::Id {
    cosmic::widget::Id::new("tile-rename")
}

/// Edit mode: tiles are picked up and dropped instead of launched.
#[derive(Debug, Clone, Copy, Default)]
pub struct Edit {
    pub on: bool,
    pub picked: Option<TileRef>,
}

/// Everything one tile needs to draw itself.
struct TileArgs<'a> {
    at: TileRef,
    app: &'a App,
    /// The user's label if set, else the app's name.
    name: &'a str,
    size: TileSize,
    dims: (f32, f32),
    finish: TileFinish,
    /// The tile's own fill, already parsed, over the finish.
    color: Option<cosmic::iced::Color>,
    /// A picture behind the tile, when its file exists.
    image: Option<&'a str>,
    /// Whether Medium and Wide tiles draw their name.
    show_name: bool,
    edit: Edit,
    /// The field and draft text while this tile's inline input is open.
    renaming: Option<(TileField, &'a str)>,
}

fn tile<'a>(args: TileArgs<'a>) -> Element<'a, Message> {
    let TileArgs {
        at,
        app,
        name,
        size,
        dims: (w, h),
        finish,
        color,
        image,
        show_name,
        edit,
        renaming,
    } = args;
    let glyph = icon(app.icon.as_cosmic_icon()).size(match size {
        TileSize::Small => ICON_SMALL,
        TileSize::Medium | TileSize::Wide => ICON_LARGE,
    });
    // Mid-edit the tile is the input, whatever its size.
    if let Some((field, draft)) = renaming {
        let placeholder = match field {
            TileField::Label => fl!("tile-name"),
            TileField::Image => fl!("tile-image"),
        };
        let input = text_input::text_input(placeholder, draft)
            .id(rename_input_id())
            .on_input(Message::RenameText)
            .on_submit(|_| Message::RenameDone);
        return container(input)
            .center_y(Length::Fixed(h))
            .width(Length::Fixed(w))
            .into();
    }
    let content: Element<'a, Message> = match size {
        TileSize::Small => container(glyph).center(Length::Fill).into(),
        // Name along the bottom edge, icon centred above it, as Windows does.
        TileSize::Medium | TileSize::Wide if show_name => column::with_children(vec![
            container(glyph)
                .center_x(Length::Fill)
                .padding([TILE_TOP, 0, 0, 0])
                .into(),
            Space::new().height(Length::Fill).into(),
            container(
                text::caption(name)
                    .font(cosmic::font::semibold())
                    .wrapping(cosmic::iced::widget::text::Wrapping::None),
            )
            .padding([0, TILE_INSET, TILE_INSET, TILE_INSET])
            .width(Length::Fill)
            .into(),
        ])
        .align_x(Alignment::Start)
        .into(),
        TileSize::Medium | TileSize::Wide => container(glyph).center(Length::Fill).into(),
    };
    let class = if edit.picked == Some(at) {
        selected_button()
    } else if edit.on {
        // An edge on every tile says "these move now" without a new colour.
        tile_button_class(TileFinish::Outline)
    } else if image.is_some() {
        image_tile_class()
    } else if let Some(fill) = color {
        colored_tile_class(fill)
    } else {
        tile_button_class(finish)
    };
    // Clipped to the tile: a square-canvas icon (OpenTTD's diamond, say)
    // otherwise draws past the fill and makes the grid look ragged.
    let content = container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .clip(true);
    let button = button::custom(content)
        .class(class)
        .padding(0)
        .width(Length::Fixed(w))
        .height(Length::Fixed(h))
        .on_press(if edit.on {
            Message::TileClicked(at)
        } else {
            Message::LaunchId(app.id.clone())
        });
    // The picture sits in a layer under the button, cropped to the tile.
    let button: Element<'a, Message> = match image {
        Some(path) => {
            let picture = container(
                cosmic::widget::image(cosmic::widget::image::Handle::from_path(path))
                    .content_fit(cosmic::iced::ContentFit::Cover)
                    .width(Length::Fixed(w))
                    .height(Length::Fixed(h)),
            )
            .clip(true)
            .width(Length::Fixed(w))
            .height(Length::Fixed(h));
            stack(vec![picture.into(), button.into()]).into()
        }
        None => button.into(),
    };
    if edit.on {
        return button;
    }
    mouse_area(button)
        .on_right_press(Message::OpenContext(Target::Tile(at)))
        .into()
}

fn heading<'a>(g: usize, name: &'a str, spacing: Spacing, edit: Edit) -> Element<'a, Message> {
    let pad = [spacing.section, 0, spacing.pad_y, 2];
    if !edit.on {
        // Full opacity, unlike a letter header: a group name is a title the
        // tiles under it belong to, not an index marker.
        return container(header_text(name))
            .padding([HEADING_ABOVE, 0, HEADING_BELOW, 2])
            .into();
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

/// Everything the right-hand column needs, borrowed from the app state.
pub struct RightView<'a> {
    pub config: &'a Config,
    pub apps: &'a [App],
    pub spacing: Spacing,
    pub edit: Edit,
    pub favs: &'a [String],
    pub recent: &'a [String],
    pub menu_open: bool,
    /// A tile field mid-edit, with the text as typed so far.
    pub renaming: Option<&'a (TileRef, TileField, String)>,
}

fn side_key(side: RightSide) -> &'static str {
    match side {
        RightSide::Tiles => "right-tiles",
        RightSide::Favourites => "right-favourites",
        RightSide::Recent => "right-recent",
    }
}

/// The "Tiles ▾" switch at the top of the column.
fn side_switch<'a>(side: RightSide, open: bool) -> Element<'a, Message> {
    let label = row::with_children(vec![
        text::body(fl!(side_key(side))).into(),
        icon::from_name("pan-down-symbolic").size(12).into(),
    ])
    .spacing(4)
    .align_y(Alignment::Center);
    let switch = button::custom(label)
        .class(quiet_button())
        .padding([4, 10])
        .on_press(Message::RightMenu(!open));
    let mut switch = popover(switch)
        .position(popover::Position::Point(Point::new(0.0, 30.0)))
        .on_close(Message::RightMenu(false));
    if open {
        let items = [RightSide::Tiles, RightSide::Favourites, RightSide::Recent].map(|m| {
            button::custom(
                row::with_children(vec![
                    text::body(fl!(side_key(m))).into(),
                    Space::new().width(Length::Fill).into(),
                    if m == side {
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
            .on_press(Message::SetRightSide(m))
            .into()
        });
        switch = switch.popup(
            container(column::with_children(items.into_iter().collect::<Vec<_>>()).spacing(1))
                .padding(6)
                .width(Length::Fixed(170.0))
                .class(menu_card()),
        );
    }
    switch.into()
}

/// Favourites or Recent: four across, icon over name, at most 16.
fn app_grid<'a>(ids: &[String], apps: &'a [App], note: String) -> Element<'a, Message> {
    let found: Vec<(usize, &'a App)> = ids
        .iter()
        .filter_map(|id| apps.iter().enumerate().find(|(_, a)| &a.id == id))
        .take(16)
        .collect();
    let mut col = column::with_capacity(6).spacing(4);
    if found.is_empty() {
        col = col.push(container(text::body(fl!("nothing-yet"))).padding([12, 2]));
    }
    for chunk in found.chunks(4) {
        let mut r = row::with_capacity(4).spacing(4);
        for &(i, app) in chunk {
            let cell = button::custom(
                column::with_children(vec![
                    icon(app.icon.as_cosmic_icon()).size(40).into(),
                    text::caption(&app.name)
                        .wrapping(cosmic::iced::widget::text::Wrapping::None)
                        .into(),
                ])
                .spacing(6)
                .align_x(Alignment::Center),
            )
            .class(quiet_button())
            .padding([10, 4, 8, 4])
            .width(Length::Fill)
            .on_press(Message::Launch(i));
            r = r.push(mouse_area(cell).on_right_press(Message::OpenContext(Target::App(i))));
        }
        for _ in chunk.len()..4 {
            r = r.push(Space::new().width(Length::Fill));
        }
        col = col.push(r);
    }
    col = col.push(container(text::caption(note)).padding([8, 2]));
    thin_scroll(scrollable(container(col).padding([
        0,
        SCROLL_GUTTER,
        12,
        0,
    ])))
    .height(Length::Fill)
    .into()
}

pub fn view<'a>(v: RightView<'a>) -> Element<'a, Message> {
    let RightView {
        config,
        apps,
        spacing,
        edit,
        favs,
        recent,
        menu_open,
        renaming,
    } = v;
    let cells = config.tile_cells();
    let width = Length::Fixed(column_width(spacing, cells));
    // Locked, the column keeps its content but loses its controls: no
    // Tiles/Favourites/Recent switch and no Edit toggle. Settings owns them.
    let switch = |grid: Element<'a, Message>| -> Element<'a, Message> {
        let mut col = column::with_capacity(2).spacing(spacing.gap);
        if !config.locked {
            col = col.push(side_switch(config.right_side, menu_open));
        }
        col.push(grid).width(width).into()
    };
    match config.right_side {
        RightSide::Tiles => {}
        RightSide::Favourites => {
            return switch(app_grid(favs, apps, fl!("fav-note")));
        }
        RightSide::Recent => {
            return switch(app_grid(recent, apps, fl!("recent-note")));
        }
    }
    let gap = f32::from(spacing.gap);
    let installed: HashSet<&str> = apps.iter().map(|a| a.id.as_str()).collect();
    let mut groups = column::with_capacity(config.groups.len() * 2).spacing(spacing.gap);
    for (g, group) in config.groups.iter().enumerate() {
        let visible = config.visible_tiles(g, &installed);
        let sizes: Vec<TileSize> = visible.iter().map(|(_, t)| t.size).collect();
        let packed = tile_layout::pack(&sizes, cells);
        let mut layer: Vec<Element<'a, Message>> = Vec::with_capacity(visible.len());
        for ((ti, t), p) in visible.iter().zip(packed.tiles.iter()) {
            let Some(app) = apps.iter().find(|a| a.id == t.app) else {
                continue;
            };
            let name = t.label.as_deref().unwrap_or(&app.name);
            let draft = renaming
                .filter(|(rat, _, _)| *rat == (g, *ti))
                .map(|(_, field, s)| (*field, s.as_str()));
            let at = (
                f32::from(p.col) * (CELL + gap),
                f32::from(p.row) * (CELL + gap),
            );
            let dims = (span(p.cols, gap), span(p.rows, gap));
            // Only a picture that is actually there: a moved or deleted
            // file falls back to the finish rather than an empty tile.
            let picture = t
                .image
                .as_deref()
                .filter(|p| std::path::Path::new(p).is_file());
            layer.push(
                pin(tile(TileArgs {
                    at: (g, *ti),
                    app,
                    name,
                    size: t.size,
                    dims,
                    finish: config.finish,
                    color: t.color.as_deref().and_then(parse_hex),
                    image: picture,
                    show_name: config.show_tile_names,
                    edit,
                    renaming: draft,
                }))
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
                .width(Length::Fixed(grid_width(spacing, cells)))
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

    if config.locked {
        return column::with_children(vec![tile_scroll(groups)])
            .width(width)
            .into();
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
        row::with_children(vec![
            side_switch(config.right_side, menu_open),
            Space::new().width(Length::Fill).into(),
            toggle.into(),
        ])
        .into(),
        tile_scroll(groups),
    ])
    .width(width)
    .into()
}

/// The tile column's scroll view. The grid is padded right by the gutter
/// `column_width` reserved, so the overlay thumb rides beside the tiles
/// instead of over them — the one thing that made the pane read as broken.
fn tile_scroll<'a>(
    groups: cosmic::widget::Column<'a, Message, cosmic::Theme>,
) -> Element<'a, Message> {
    // 12 px of bottom padding so the last heading or tile row never clips
    // against the popup's bottom edge.
    thin_scroll(scrollable(container(groups).padding([
        0,
        SCROLL_GUTTER,
        12,
        0,
    ])))
    .height(Length::Fill)
    .into()
}
