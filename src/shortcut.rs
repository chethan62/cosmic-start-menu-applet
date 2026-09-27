//! The menu opened by the Super shortcut (`--toggle`): a layer surface of
//! its own rather than the panel button's popup, because a popup only gets
//! the keyboard from a click on the panel. The state and messages stay in
//! `app.rs`, shared with the panel popup; this is how the surface is made
//! and painted.

use cosmic::app::Task;
use cosmic::iced::window::Id;

use crate::app::{popup_limits, Message, POPUP_HEIGHT};
use crate::config::MenuPosition;

/// Window settings for [`Mode::Shortcut`](crate::app::Mode::Shortcut): no window of its own, only the
/// layer surface the menu opens.
pub fn window_settings() -> cosmic::app::Settings {
    cosmic::app::Settings::default()
        .no_main_window(true)
        .transparent(true)
        .exit_on_close(false)
        .debug(false)
}

/// How long a closed shortcut menu lingers before exiting, so a launch it
/// started has been handed off, and a quick second press can reopen it.
pub const LINGER: std::time::Duration = std::time::Duration::from_millis(1500);

/// The shortcut menu's surface: bottom-left, just above the panel (the
/// compositor keeps it out of the panel's reserved strip). It has to hold
/// the keyboard outright: asked for on demand, COSMIC left focus where it
/// was. A full-screen transparent surface catching click-away was tried and
/// dropped: COSMIC blurred the whole screen behind it.
pub fn surface(id: Id, width: f32, position: MenuPosition) -> Task<Message> {
    use cosmic::iced::platform_specific::shell::commands::layer_surface::{
        get_layer_surface, Anchor, KeyboardInteractivity, Layer,
    };
    use cosmic::iced::runtime::platform_specific::wayland::layer_surface::{
        IcedMargin, IcedOutput, SctkLayerSurfaceSettings,
    };
    get_layer_surface(SctkLayerSurfaceSettings {
        id,
        layer: Layer::Top,
        keyboard_interactivity: KeyboardInteractivity::Exclusive,
        input_zone: None,
        // Anchoring one edge alone centres along it.
        anchor: match position {
            MenuPosition::Corner => Anchor::BOTTOM | Anchor::LEFT,
            MenuPosition::Top => Anchor::TOP,
            MenuPosition::Centre => Anchor::BOTTOM,
        },
        output: IcedOutput::Active,
        namespace: "start-menu".into(),
        margin: IcedMargin {
            top: if position == MenuPosition::Top {
                EDGE
            } else {
                0
            },
            right: 0,
            bottom: if position == MenuPosition::Top {
                0
            } else {
                EDGE
            },
            left: if position == MenuPosition::Corner {
                EDGE
            } else {
                0
            },
        },
        size: Some((Some(width as u32), Some(POPUP_HEIGHT as u32))),
        exclusive_zone: 0,
        size_limits: popup_limits(width),
    })
}

/// Gap between the shortcut menu and the screen edge / panel.
const EDGE: i32 = 4;

/// The menu's card, as `popup_container` paints it, for the shortcut menu:
/// `popup_container` is an autosize widget that resizes the panel's popup,
/// and has no business resizing a layer surface.
///
/// Always the opaque background: the blur request that frosts the panel
/// popup does not take on this layer surface, and the frosted colour with
/// nothing blurred behind it left the menu hard to read over windows.
pub fn card_style(theme: &cosmic::Theme) -> cosmic::widget::container::Style {
    let cosmic = theme.cosmic();
    let background = cosmic.background(false);
    cosmic::widget::container::Style {
        text_color: Some(background.on.into()),
        icon_color: Some(background.on.into()),
        background: Some(cosmic::iced::Color::from(background.base).into()),
        border: cosmic::iced::Border {
            radius: cosmic.corner_radii.radius_m.into(),
            width: 1.0,
            color: background.divider.into(),
        },
        ..Default::default()
    }
}
