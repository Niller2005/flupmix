use crate::theme;
use gpui::{div, prelude::*, rgb, App, Context, FocusHandle, KeyDownEvent, Render, Window};

type Commit = Box<dyn Fn(&str, &mut App)>;

pub(crate) struct TextField {
    text: String,
    original: String,
    cursor: usize,
    focus: FocusHandle,
    editing: bool,
    commit: Commit,
}

impl TextField {
    pub(crate) fn new(
        text: String,
        commit: impl Fn(&str, &mut App) + 'static,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            cursor: text.len(),
            original: text.clone(),
            text,
            focus: cx.focus_handle(),
            editing: false,
            commit: Box::new(commit),
        }
    }

    pub(crate) fn value(&self) -> &str {
        &self.text
    }

    pub(crate) fn clear(&mut self, cx: &mut Context<Self>) {
        self.text.clear();
        self.original.clear();
        self.cursor = 0;
        cx.notify();
    }

    fn finish(&mut self, accept: bool, cx: &mut Context<Self>) {
        if !self.editing {
            return;
        }
        if accept {
            (self.commit)(&self.text, cx);
            self.original.clone_from(&self.text);
        } else {
            self.text.clone_from(&self.original);
        }
        self.cursor = self.text.len();
        self.editing = false;
        cx.notify();
    }

    fn key(&mut self, ev: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.editing {
            return;
        }
        let k = &ev.keystroke;
        match k.key.as_str() {
            "enter" => self.finish(true, cx),
            "escape" => {
                self.finish(false, cx);
                return;
            }
            "left" => {
                self.cursor = self.text[..self.cursor]
                    .char_indices()
                    .last()
                    .map_or(0, |(i, _)| i)
            }
            "right" => {
                self.cursor += self.text[self.cursor..]
                    .chars()
                    .next()
                    .map_or(0, char::len_utf8)
            }
            "home" => self.cursor = 0,
            "end" => self.cursor = self.text.len(),
            "backspace" => {
                let prev = self.text[..self.cursor]
                    .char_indices()
                    .last()
                    .map_or(0, |(i, _)| i);
                self.text.replace_range(prev..self.cursor, "");
                self.cursor = prev;
            }
            "delete" => {
                let next = self.cursor
                    + self.text[self.cursor..]
                        .chars()
                        .next()
                        .map_or(0, char::len_utf8);
                self.text.replace_range(self.cursor..next, "");
            }
            _ => {
                if !k.modifiers.control && !k.modifiers.platform {
                    if let Some(chars) = &k.key_char {
                        if !chars.chars().any(char::is_control) {
                            self.text.insert_str(self.cursor, chars);
                            self.cursor += chars.len();
                        }
                    }
                }
            }
        }
        cx.stop_propagation();
        cx.notify();
    }
}

impl Render for TextField {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let display = if self.editing {
            format!(
                "{}│{}",
                &self.text[..self.cursor],
                &self.text[self.cursor..]
            )
        } else if self.text.is_empty() {
            "Type pattern…".into()
        } else {
            self.text.clone()
        };
        div()
            .id("text-field")
            .track_focus(&self.focus)
            .min_h_8()
            .min_w_0()
            .px_2()
            .py_1()
            .rounded_sm()
            .border_1()
            .border_color(rgb(if self.editing {
                theme::MINT
            } else {
                theme::BORDER
            }))
            .bg(rgb(theme::PANEL_SUNKEN))
            .text_color(rgb(theme::TEXT))
            .text_sm()
            .cursor_text()
            .overflow_x_hidden()
            .on_click(cx.listener(|this, _, window, cx| {
                this.editing = true;
                this.focus.focus(window, cx);
                cx.notify();
            }))
            .on_mouse_down_out(cx.listener(|this, _, _, cx| this.finish(true, cx)))
            .on_key_down(cx.listener(Self::key))
            .child(display)
    }
}
