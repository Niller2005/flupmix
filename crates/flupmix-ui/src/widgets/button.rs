use crate::theme;
use gpui::{div, prelude::*, rgb, Div, ElementId, Stateful};

pub(crate) fn button(
    id: impl Into<ElementId>,
    label: impl Into<gpui::SharedString>,
    active: bool,
) -> Stateful<Div> {
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
        .border_color(rgb(if active { theme::MINT } else { theme::BORDER }))
        .bg(rgb(if active {
            theme::MINT_BG
        } else {
            theme::PANEL_RAISED
        }))
        .text_color(rgb(if active { theme::MINT } else { theme::TEXT }))
        .text_xs()
        .font_family(theme::FONT_MONO)
        .cursor_pointer()
        .hover(|s| s.border_color(rgb(theme::MINT)).bg(rgb(theme::MINT_BG)))
        .child(label.into())
}
