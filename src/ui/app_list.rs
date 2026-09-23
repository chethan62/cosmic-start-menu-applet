//! Middle column: an optional "Most used" block, then every app under its
//! letter header.

use cosmic::desktop::IconSourceExt;
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{button, column, container, icon, row, scrollable, text};
use cosmic::Element;

use crate::app::Message;
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
    button::custom(body)
        .class(if selected {
            selected_button()
        } else {
            quiet_button()
        })
        .padding([0, 10])
        .width(Length::Fill)
        .height(Length::Fixed(ROW_HEIGHT))
        .on_press(Message::Launch(index))
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

pub fn view<'a>(
    list: &'a [App],
    most_used: &[String],
    show_most_used: bool,
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
        .width(Length::Fixed(LIST_WIDTH))
        .height(Length::Fill)
        .into()
}
