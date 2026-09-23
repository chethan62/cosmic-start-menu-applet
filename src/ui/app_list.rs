//! Middle column: an optional "Most used" block, then every app under its
//! letter header.

use cosmic::desktop::IconSourceExt;
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{button, column, container, icon, mouse_area, row, scrollable, text};
use cosmic::Element;

use crate::app::{Message, Target};
use crate::apps::{self, App};
use crate::fl;
use crate::ui::{quiet_button, selected_button, HEADER_HEIGHT, ICON, LIST_WIDTH, ROW_HEIGHT};

pub fn app_row<'a>(app: &'a App, index: usize, selected: bool) -> Element<'a, Message> {
    let body = row::with_children(vec![
        icon(app.icon.as_cosmic_icon()).size(ICON).into(),
        text::body(&app.name)
            .wrapping(cosmic::iced::widget::text::Wrapping::None)
            .into(),
    ])
    .spacing(12)
    .align_y(Alignment::Center);
    let body = button::custom(body)
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
    button::custom(text::heading(letter.to_string()))
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

pub fn view<'a>(
    list: &'a [App],
    most_used: &[String],
    show_most_used: bool,
    list_id: cosmic::widget::Id,
) -> Element<'a, Message> {
    let mut col = column::with_capacity(list.len() + 40).spacing(0);
    if show_most_used && !most_used.is_empty() {
        col = col.push(section_label(fl!("most-used")));
        for id in most_used {
            if let Some(i) = list.iter().position(|a| &a.id == id) {
                col = col.push(app_row(&list[i], i, false));
            }
        }
    }
    for (letter, idxs) in apps::sections(list) {
        col = col.push(letter_header(letter));
        for i in idxs {
            col = col.push(app_row(&list[i], i, false));
        }
    }
    scrollable(container(col).padding([0, 8, 0, 0]))
        .id(list_id)
        .width(Length::Fixed(LIST_WIDTH))
        .height(Length::Fill)
        .into()
}

/// Search results, best first, with the keyboard selection highlighted.
pub fn results_view<'a>(
    list: &'a [App],
    hits: &[usize],
    selected: usize,
    query: &str,
) -> Element<'a, Message> {
    if hits.is_empty() {
        return container(text::body(fl!("no-results", query = query.trim())))
            .padding([12, 10])
            .into();
    }
    let rows = hits
        .iter()
        .enumerate()
        .map(|(n, &i)| app_row(&list[i], i, n == selected))
        .collect::<Vec<_>>();
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
