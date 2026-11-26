use crate::theme;
use gpui::{div, prelude::*, rgb, rgba, Div, ElementId, Stateful};

pub(crate) fn button(
    id: impl Into<ElementId>,
    label: impl Into<gpui::SharedString>,
    active: bool,
) -> Stateful<Div> {
    tone_button(id, label, active, theme::colors().accent)
}

pub(crate) fn tone_button(
    id: impl Into<ElementId>,
    label: impl Into<gpui::SharedString>,
    active: bool,
    color: u32,
) -> Stateful<Div> {
    let fill = if color == theme::colors().accent {
        rgb(theme::colors().accent_bg)
    } else {
        rgba((color << 8) | 0x33)
    };
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .min_h_8()
        .px_3()
        .py_1()
        .rounded_md()
        .border_1()
        .border_color(rgb(if active {
            color
        } else {
            theme::colors().border
        }))
        .bg(if active {
            fill
        } else {
            rgb(theme::colors().panel_raised)
        })
        .text_color(rgb(if active { color } else { theme::colors().text }))
        .text_xs()
        .font_family(theme::FONT_MONO)
        .cursor_pointer()
        .hover(move |s| s.border_color(rgb(color)).bg(fill))
        .child(label.into())
}
