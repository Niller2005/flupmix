use crate::theme;
use gpui::{div, prelude::*, rgb, rgba, Div, ElementId, Stateful};

pub(crate) fn button(
    id: impl Into<ElementId>,
    label: impl Into<gpui::SharedString>,
    active: bool,
) -> Stateful<Div> {
    tone_button(id, label, active, theme::ACCENT)
}

pub(crate) fn tone_button(
    id: impl Into<ElementId>,
    label: impl Into<gpui::SharedString>,
    active: bool,
    color: u32,
) -> Stateful<Div> {
    let fill = if color == theme::ACCENT {
        rgb(theme::ACCENT_BG)
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
        .border_color(rgb(if active { color } else { theme::BORDER }))
        .bg(if active {
            fill
        } else {
            rgb(theme::PANEL_RAISED)
        })
        .text_color(rgb(if active { color } else { theme::TEXT }))
        .text_xs()
        .font_family(theme::FONT_MONO)
        .cursor_pointer()
        .hover(move |s| s.border_color(rgb(color)).bg(fill))
        .child(label.into())
}
