//! Middle column: an optional "Most used" block, then every app under its
//! letter header.

use cosmic::desktop::IconSourceExt;
use std::collections::HashSet;

use cosmic::iced::{Alignment, Length, Point};
use cosmic::widget::{
    button, column, container, icon, mouse_area, popover, row, scrollable, text, Space,
};
use cosmic::Element;

use crate::app::{Message, Target};
use crate::apps::{self, App};
use crate::config::ListMode;
use crate::fl;
use crate::folders::Folder;
use crate::launcher::{Item, Section};
use crate::ui::{
    menu_card, quiet_button, row_radius, selected_button, HEADER_HEIGHT, ICON, LIST_WIDTH,
    ROW_HEIGHT,
};

/// A fixed-height button lays its content out from the top; this centres it
/// vertically so icon and text sit on the row's midline.
fn centred<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(content)
        .height(Length::Fill)
        .align_y(Alignment::Center)
        .into()
}

pub fn app_row<'a>(app: &'a App, index: usize, selected: bool) -> Element<'a, Message> {
    let body = row::with_children(vec![
        icon(app.icon.as_cosmic_icon()).size(ICON).into(),
        text::body(&app.name)
            .wrapping(cosmic::iced::widget::text::Wrapping::None)
            .into(),
    ])
    .spacing(12)
    .align_y(Alignment::Center);
    let body = button::custom(centred(body))
        .class(if selected {
            selected_button()
        } else {
            quiet_button()
        })
        .padding([0, 10])
        .width(Length::Fill)
        .height(Length::Fixed(ROW_HEIGHT))
        .on_press(Message::Launch(index));
    mouse_area(body)
        .on_right_press(Message::OpenContext(Target::App(index)))
        .into()
}

pub fn section_label<'a>(label: String) -> Element<'a, Message> {
    container(text::caption_heading(label))
        .padding([0, 10])
        .height(Length::Fixed(HEADER_HEIGHT))
        .align_y(Alignment::End)
        .into()
}

pub fn letter_header<'a>(letter: char) -> Element<'a, Message> {
    button::custom(centred(text::heading(letter.to_string())))
        .class(quiet_button())
        .padding([0, 10])
        .height(Length::Fixed(HEADER_HEIGHT))
        .on_press(Message::LetterGrid(true))
        .into()
}

/// How far down the list `target`'s header sits. Rows and headers have fixed
/// heights, so this is exact rather than estimated.
pub fn offset_of<L: PartialEq>(
    sections: &[(L, Vec<usize>)],
    most_used_rows: usize,
    target: &L,
    row_h: f32,
    header_h: f32,
) -> f32 {
    let mut y = if most_used_rows > 0 {
        header_h + most_used_rows as f32 * row_h
    } else {
        0.0
    };
    for (label, rows) in sections {
        if label == target {
            return y;
        }
        y += header_h + rows.len() as f32 * row_h;
    }
    y
}

/// Tapping a letter header swaps the list for this grid of letters; tapping
/// one jumps there. Letters with no apps are shown but disabled, as Windows
/// does, so the grid keeps its shape.
pub fn letter_grid<'a>(present: &[char]) -> Element<'a, Message> {
    let mut letters: Vec<char> = std::iter::once('#').chain('A'..='Z').collect();
    let extra: Vec<char> = present
        .iter()
        .copied()
        .filter(|c| !letters.contains(c))
        .collect();
    letters.extend(extra);
    let rows = letters.chunks(4).map(|chunk| {
        let mut r = row::with_capacity(4).spacing(6);
        for &c in chunk {
            let b = button::custom(container(text::title4(c.to_string())).center(Length::Fill))
                .class(quiet_button())
                .padding(0)
                .width(Length::Fill)
                .height(Length::Fixed(48.0));
            r = r.push(if present.contains(&c) {
                b.on_press(Message::JumpTo(c))
            } else {
                b
            });
        }
        for _ in chunk.len()..4 {
            r = r.push(cosmic::widget::Space::new().width(Length::Fill));
        }
        r.into()
    });
    scrollable(column::with_children(rows.collect::<Vec<_>>()).spacing(6))
        .width(Length::Fixed(LIST_WIDTH))
        .height(Length::Fill)
        .into()
}

/// Everything the middle column needs, borrowed from the app state.
pub struct ListView<'a> {
    pub apps: &'a [App],
    pub most_used: &'a [String],
    pub show_most_used: bool,
    pub mode: ListMode,
    pub folders: &'a [Folder],
    /// Apps in no folder, for the Folders view.
    pub loose: &'a [usize],
    pub open_folders: &'a HashSet<usize>,
    pub list_id: cosmic::widget::Id,
}

fn mode_key(mode: ListMode) -> &'static str {
    match mode {
        ListMode::Az => "mode-az",
        ListMode::Category => "mode-category",
        ListMode::Folders => "mode-folders",
    }
}

/// "All apps" and the switch between A–Z, Category and Folders.
pub fn list_bar<'a>(mode: ListMode, menu_open: bool) -> Element<'a, Message> {
    let label = row::with_children(vec![
        text::body(fl!(mode_key(mode))).into(),
        icon::from_name("pan-down-symbolic").size(12).into(),
    ])
    .spacing(4)
    .align_y(Alignment::Center);
    let switch = button::custom(label)
        .class(quiet_button())
        .padding([3, 8])
        .on_press(Message::ModeMenu(!menu_open));
    let mut switch = popover(switch)
        .position(popover::Position::Point(Point::new(0.0, 30.0)))
        .on_close(Message::ModeMenu(false));
    if menu_open {
        let items = [ListMode::Az, ListMode::Category, ListMode::Folders].map(|m| {
            button::custom(
                row::with_children(vec![
                    text::body(fl!(mode_key(m))).into(),
                    Space::new().width(Length::Fill).into(),
                    if m == mode {
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
            .on_press(Message::SetListMode(m))
            .into()
        });
        switch = switch.popup(
            container(column::with_children(items.into_iter().collect::<Vec<_>>()).spacing(1))
                .padding(6)
                .width(Length::Fixed(170.0))
                .class(menu_card()),
        );
    }
    row::with_children(vec![
        container(text::caption(fl!("all-apps")))
            .padding([0, 10])
            .into(),
        Space::new().width(Length::Fill).into(),
        switch.into(),
    ])
    .align_y(Alignment::Center)
    .width(Length::Fixed(LIST_WIDTH))
    .into()
}

fn category_header<'a>(key: &'static str) -> Element<'a, Message> {
    button::custom(centred(text::heading(fl!(key))))
        .class(quiet_button())
        .padding([0, 10])
        .height(Length::Fixed(HEADER_HEIGHT))
        .on_press(Message::LetterGrid(true))
        .into()
}

/// A folder row: folder glyph on a tinted base, name, count, chevron.
fn folder_row<'a>(index: usize, folder: &'a Folder, open: bool) -> Element<'a, Message> {
    let base = container(icon::from_name("folder-symbolic").size(16))
        .center(Length::Fixed(f32::from(ICON)))
        .class(cosmic::theme::Container::Custom(Box::new(|theme| {
            let mut tint = theme.cosmic().accent_color();
            tint.alpha = 0.22;
            container::Style {
                background: Some(cosmic::iced::Background::Color(tint.into())),
                border: cosmic::iced::Border {
                    radius: row_radius(theme).into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })));
    let body = row::with_children(vec![
        base.into(),
        text::body(&folder.name).width(Length::Fill).into(),
        text::caption(folder.apps.len().to_string()).into(),
        icon::from_name(if open {
            "pan-down-symbolic"
        } else {
            "pan-end-symbolic"
        })
        .size(12)
        .into(),
    ])
    .spacing(12)
    .align_y(Alignment::Center);
    button::custom(centred(body))
        .class(quiet_button())
        .padding([0, 10])
        .width(Length::Fill)
        .height(Length::Fixed(ROW_HEIGHT))
        .on_press(Message::ToggleFolder(index))
        .into()
}

pub fn view<'a>(v: ListView<'a>) -> Element<'a, Message> {
    let list = v.apps;
    let mut col = column::with_capacity(list.len() + 40).spacing(0);
    if v.show_most_used && !v.most_used.is_empty() {
        col = col.push(section_label(fl!("most-used")));
        for id in v.most_used {
            if let Some(i) = list.iter().position(|a| &a.id == id) {
                col = col.push(app_row(&list[i], i, false));
            }
        }
    }
    let az = |mut col: cosmic::widget::Column<'a, Message, cosmic::Theme>,
              sections: Vec<(char, Vec<usize>)>| {
        for (letter, idxs) in sections {
            col = col.push(letter_header(letter));
            for i in idxs {
                col = col.push(app_row(&list[i], i, false));
            }
        }
        col
    };
    col = match v.mode {
        ListMode::Az => az(col, apps::sections(list)),
        ListMode::Category => {
            for (key, idxs) in apps::category_sections(list) {
                col = col.push(category_header(key));
                for i in idxs {
                    col = col.push(app_row(&list[i], i, false));
                }
            }
            col
        }
        // No folders found (no App Library file, or an unreadable one) means
        // the plain A–Z list rather than an empty section.
        ListMode::Folders if v.folders.is_empty() => az(col, apps::sections(list)),
        ListMode::Folders => {
            col = col.push(section_label(fl!("folders")));
            for (fi, folder) in v.folders.iter().enumerate() {
                let open = v.open_folders.contains(&fi);
                col = col.push(folder_row(fi, folder, open));
                if open {
                    let inner = folder
                        .apps
                        .iter()
                        .map(|&i| app_row(&list[i], i, false))
                        .collect::<Vec<_>>();
                    col = col.push(container(column::with_children(inner)).padding([0, 0, 0, 16]));
                }
            }
            az(col, apps::sections_of(list, v.loose))
        }
    };
    scrollable(container(col).padding([0, 8, 0, 0]))
        .id(v.list_id)
        .width(Length::Fixed(LIST_WIDTH))
        .height(Length::Fill)
        .into()
}

/// The Category view's jump grid: two columns of category names.
pub fn category_grid<'a>(present: &[&'static str]) -> Element<'a, Message> {
    let rows = present.chunks(2).map(|chunk| {
        let mut r = row::with_capacity(2).spacing(6);
        for &key in chunk {
            r = r.push(
                button::custom(container(text::body(fl!(key))).center(Length::Fill))
                    .class(quiet_button())
                    .padding(0)
                    .width(Length::Fill)
                    .height(Length::Fixed(44.0))
                    .on_press(Message::JumpToCategory(key)),
            );
        }
        if chunk.len() < 2 {
            r = r.push(Space::new().width(Length::Fill));
        }
        r.into()
    });
    scrollable(column::with_children(rows.collect::<Vec<_>>()).spacing(6))
        .width(Length::Fixed(LIST_WIDTH))
        .height(Length::Fill)
        .into()
}

/// One of the launcher's results: icon, name, and its detail underneath.
fn found_row<'a>(item: &'a Item, selected: bool) -> Element<'a, Message> {
    let glyph: Element<'a, Message> = match &item.icon {
        Some(name) => icon::from_name(name.as_str()).size(ICON).into(),
        None => Space::new().width(ICON).into(),
    };
    let mut words = column::with_capacity(2)
        .push(text::body(&item.name).wrapping(cosmic::iced::widget::text::Wrapping::None));
    if !item.description.is_empty() {
        words = words.push(
            text::caption(&item.description).wrapping(cosmic::iced::widget::text::Wrapping::None),
        );
    }
    let body = row::with_children(vec![glyph, words.into()])
        .spacing(12)
        .align_y(Alignment::Center);
    button::custom(centred(body))
        .class(if selected {
            selected_button()
        } else {
            quiet_button()
        })
        .padding([0, 10])
        .width(Length::Fill)
        .height(Length::Fixed(ROW_HEIGHT + 12.0))
        .on_press(Message::LauncherActivate(item.id))
        .into()
}

/// Search results: matching apps first, then what the launcher found, one
/// headed section per kind. `selected` counts down through both.
pub fn results_view<'a>(
    list: &'a [App],
    hits: &[usize],
    found: &'a [Item],
    selected: usize,
    query: &str,
) -> Element<'a, Message> {
    if hits.is_empty() && found.is_empty() {
        return container(text::body(fl!("no-results", query = query.trim())))
            .padding([12, 10])
            .into();
    }
    let mut rows: Vec<Element<'a, Message>> = Vec::with_capacity(hits.len() + found.len() + 8);
    if !hits.is_empty() && !found.is_empty() {
        rows.push(section_label(fl!("search-apps")));
    }
    rows.extend(
        hits.iter()
            .enumerate()
            .map(|(n, &i)| app_row(&list[i], i, n == selected)),
    );
    let mut last: Option<Section> = None;
    for (n, item) in found.iter().enumerate() {
        if last != Some(item.section) {
            rows.push(section_label(fl!(item.section.title_key())));
            last = Some(item.section);
        }
        rows.push(found_row(item, hits.len() + n == selected));
    }
    scrollable(column::with_children(rows))
        .height(Length::Fill)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offset_counts_headers_and_rows_before_the_letter() {
        let s = vec![('#', vec![0]), ('A', vec![1, 2]), ('B', vec![3])];
        // Most used: heading + 2 rows. Then '#': header+1 row, 'A': header+2 rows.
        assert_eq!(
            offset_of(&s, 2, &'B', 10.0, 20.0),
            20.0 + 2.0 * 10.0 + 20.0 + 10.0 + 20.0 + 2.0 * 10.0
        );
        assert_eq!(offset_of(&s, 0, &'#', 10.0, 20.0), 0.0);
    }
}
