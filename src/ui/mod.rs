//! Shared styling for the popup.
//!
//! Nothing here picks a colour, radius or font size by hand: everything comes
//! from the active COSMIC theme, so the menu follows light/dark, accent,
//! roundness and frosted glass without knowing which is in use. The tile
//! painting is adapted from cosmic-control-center-applet, where the reasons
//! behind each choice were found the hard way.

pub mod app_list;
pub mod context;
pub mod rail;
pub mod tiles;

use cosmic::iced::{Background, Border, Color};
use cosmic::widget::button;

use crate::config::TileFinish;

pub const RAIL_WIDTH: f32 = 44.0;
pub const LIST_WIDTH: f32 = 272.0;
/// App icon size in a list row.
pub const ICON: u16 = 24;
/// Fixed so a letter jump can compute exact scroll offsets.
pub const ROW_HEIGHT: f32 = 36.0;
pub const HEADER_HEIGHT: f32 = 32.0;

/// The popup's spacing scale, read from the theme. COSMIC ships three
/// densities with different values for the same token, so these are looked
/// up rather than assumed.
#[derive(Debug, Clone, Copy)]
pub struct Spacing {
    pub gap: u16,
    pub pad_y: u16,
    pub pad_x: u16,
    pub section: u16,
}

impl Spacing {
    pub fn from_theme(theme: &cosmic::Theme) -> Self {
        let spacing = theme.cosmic().spacing;
        Self {
            gap: spacing.space_xxs,
            pad_y: spacing.space_xxs,
            pad_x: spacing.space_xs,
            section: spacing.space_xs,
        }
    }
}

/// The colour any card on the popup is painted in: COSMIC's component on the
/// background layer. It is frost-aware (its alpha follows the frosted flags),
/// and it is *not* `Container::Primary`, which under frost is the popup's own
/// glass and makes cards read as holes.
fn tile_component(theme: &cosmic::Theme) -> cosmic::cosmic_theme::Component {
    theme
        .cosmic()
        .background(theme.transparent)
        .component
        .clone()
}

/// How much of the card colour a `Frosted` tile keeps: dense enough to read
/// as a surface, thin enough that the blur behind still shows.
const FROSTED_TILE_ALPHA: f32 = 0.68;

/// Fill and edge width for a finish. One place, so a button tile and a
/// container tile cannot disagree about what "frosted" means.
fn finish_paint(
    _theme: &cosmic::Theme,
    finish: TileFinish,
    fill: cosmic::cosmic_theme::palette::Srgba,
) -> (Option<Color>, f32) {
    let mut fill = fill;
    match finish {
        TileFinish::Solid => (Some(Color::from(fill)), 0.0),
        TileFinish::Frosted => {
            fill.alpha *= FROSTED_TILE_ALPHA;
            (Some(Color::from(fill)), 0.0)
        }
        TileFinish::Outline => (None, 1.0),
    }
}

/// Tiles take the desktop's medium radius: 16 on Round, 8 on Slightly round,
/// 2 on Square. A pill (CCCA's rule) would turn a 2×2 tile into a circle.
pub fn tile_radius(theme: &cosmic::Theme) -> f32 {
    theme.cosmic().corner_radii.radius_m[0]
}

/// Rows, rail buttons and menu items take the small radius.
pub fn row_radius(theme: &cosmic::Theme) -> f32 {
    theme.cosmic().corner_radii.radius_s[0]
}

/// Half of `height`, capped by the largest radius the desktop's roundness
/// allows: Round lets a pill through, Slightly round trims it, Square keeps it
/// square.
pub fn pill_radius(theme: &cosmic::Theme, height: f32) -> f32 {
    (height / 2.0).min(theme.cosmic().corner_radii.radius_xl[0])
}

fn tile_border(theme: &cosmic::Theme, width: f32) -> Border {
    let mut edge = theme
        .cosmic()
        .background(theme.transparent)
        .component
        .divider;
    edge.alpha *= 0.7;
    Border {
        radius: tile_radius(theme).into(),
        width,
        color: if width > 0.0 {
            Color::from(edge)
        } else {
            Color::TRANSPARENT
        },
    }
}

/// A tile button in the frost-aware card colour, with that component's hover
/// and pressed washes. Text and icon colours are left unset so they inherit.
pub fn tile_button_class(finish: TileFinish) -> button::ButtonClass {
    fn style(
        theme: &cosmic::Theme,
        finish: TileFinish,
        fill: cosmic::cosmic_theme::palette::Srgba,
    ) -> button::Style {
        let (fill, edge) = finish_paint(theme, finish, fill);
        let border = tile_border(theme, edge);
        button::Style {
            background: fill.map(Background::Color),
            border_radius: border.radius,
            border_width: border.width,
            border_color: border.color,
            text_color: None,
            icon_color: None,
            ..button::Style::new()
        }
    }

    button::ButtonClass::Custom {
        active: Box::new(move |_focused, theme| style(theme, finish, tile_component(theme).base)),
        disabled: Box::new(move |theme| style(theme, finish, tile_component(theme).disabled)),
        hovered: Box::new(move |_focused, theme| {
            style(theme, TileFinish::Solid, tile_component(theme).hover)
        }),
        pressed: Box::new(move |_focused, theme| {
            style(theme, TileFinish::Solid, tile_component(theme).pressed)
        }),
    }
}

/// No fill at rest, the card component's washes under the pointer. Used for
/// list rows, rail buttons and menu items. `Transparent` is no good: it zeroes
/// the text colour rather than leaving it alone.
pub fn quiet_button() -> button::ButtonClass {
    fn base(
        theme: &cosmic::Theme,
        fill: Option<cosmic::cosmic_theme::palette::Srgba>,
    ) -> button::Style {
        button::Style {
            background: fill.map(|c| Background::Color(Color::from(c))),
            border_radius: row_radius(theme).into(),
            text_color: None,
            icon_color: None,
            ..button::Style::new()
        }
    }

    button::ButtonClass::Custom {
        active: Box::new(|_focused, theme| base(theme, None)),
        disabled: Box::new(|theme| base(theme, None)),
        hovered: Box::new(|_focused, theme| base(theme, Some(tile_component(theme).hover))),
        pressed: Box::new(|_focused, theme| base(theme, Some(tile_component(theme).pressed))),
    }
}

/// The keyboard-selected search result: an accent tint, like COSMIC's own
/// list selection, with the text left in its normal colour.
pub fn selected_button() -> button::ButtonClass {
    fn base(theme: &cosmic::Theme, alpha: f32) -> button::Style {
        let mut tint = theme.cosmic().accent_color();
        tint.alpha = alpha;
        button::Style {
            background: Some(Background::Color(Color::from(tint))),
            border_radius: row_radius(theme).into(),
            text_color: None,
            icon_color: None,
            ..button::Style::new()
        }
    }

    button::ButtonClass::Custom {
        active: Box::new(|_focused, theme| base(theme, 0.24)),
        disabled: Box::new(|theme| base(theme, 0.24)),
        hovered: Box::new(|_focused, theme| base(theme, 0.32)),
        pressed: Box::new(|_focused, theme| base(theme, 0.4)),
    }
}

/// A floating menu's card: the popup's own layer colour, opaque enough to
/// read over the list beneath it, with the theme's medium radius.
pub fn menu_card<'a>() -> cosmic::theme::Container<'a> {
    cosmic::theme::Container::Custom(Box::new(|theme| {
        let cosmic = theme.cosmic();
        let layer = cosmic.background(false);
        let mut bg = layer.base;
        bg.alpha = bg.alpha.max(0.96);
        cosmic::widget::container::Style {
            background: Some(Background::Color(Color::from(bg))),
            text_color: Some(layer.on.into()),
            icon_color: Some(layer.on.into()),
            border: Border {
                radius: cosmic.corner_radii.radius_m.into(),
                width: 1.0,
                color: Color::from(layer.divider),
            },
            ..Default::default()
        }
    }))
}
