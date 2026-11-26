//! Whole-mixer presets with confirmations scoped to one action and filename.

use flupmix_core::{presets, MixerState};
use gpui::{div, prelude::*, px, rgb, App, Context, Entity, Render, Window};

use crate::{
    app::MixerModel,
    theme,
    widgets::{
        button::{button, tone_button},
        text_field::TextField,
    },
};

#[derive(Debug, PartialEq, Eq)]
enum Confirmation {
    Overwrite(String),
    Delete(String),
}

fn confirm(pending: &mut Option<Confirmation>, action: Confirmation) -> bool {
    if pending.as_ref() == Some(&action) {
        *pending = None;
        true
    } else {
        *pending = Some(action);
        false
    }
}

type OnLoad = Box<dyn Fn(MixerState, &mut Window, &mut App)>;

pub(crate) struct PresetPicker {
    model: Entity<MixerModel>,
    name: Entity<TextField>,
    names: Vec<String>,
    confirmation: Option<Confirmation>,
    error: Option<String>,
    on_load: OnLoad,
}

impl PresetPicker {
    pub(crate) fn new(
        model: Entity<MixerModel>,
        on_load: impl Fn(MixerState, &mut Window, &mut App) + 'static,
        cx: &mut Context<Self>,
    ) -> Self {
        let name =
            cx.new(|cx| TextField::new(String::new(), |_, _| {}, cx).placeholder("Preset name"));
        cx.observe(&name, |this, name, cx| {
            if let Some(Confirmation::Overwrite(confirmed)) = &this.confirmation {
                if presets::clean_name(name.read(cx).value()).as_ref() != Some(confirmed) {
                    this.confirmation = None;
                }
            }
            this.error = None;
            cx.notify();
        })
        .detach();
        Self {
            model,
            name,
            names: Vec::new(),
            confirmation: None,
            error: None,
            on_load: Box::new(on_load),
        }
    }

    pub(crate) fn open(&mut self, cx: &mut Context<Self>) {
        self.names = presets::list();
        self.confirmation = None;
        self.error = None;
        self.name.update(cx, |name, cx| name.clear(cx));
        cx.notify();
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        self.error = None;
        let Some(name) = presets::clean_name(self.name.read(cx).value()) else {
            self.confirmation = None;
            self.error = Some("Use 1–64 characters; no leading dot or filename symbols.".into());
            cx.notify();
            return;
        };
        self.names = presets::list();
        if self.names.contains(&name)
            && !confirm(
                &mut self.confirmation,
                Confirmation::Overwrite(name.clone()),
            )
        {
            cx.notify();
            return;
        }
        self.confirmation = None;
        if let Err(error) = presets::save(&name, &self.model.read(cx).state) {
            self.error = Some(format!("Could not save preset: {error}"));
        }
        self.names = presets::list();
        cx.notify();
    }

    fn delete(&mut self, name: &str, cx: &mut Context<Self>) {
        self.error = None;
        if !confirm(&mut self.confirmation, Confirmation::Delete(name.into())) {
            cx.notify();
            return;
        }
        if let Err(error) = presets::delete(name) {
            self.error = Some(format!("Could not delete preset: {error}"));
        }
        self.names = presets::list();
        cx.notify();
    }

    fn load(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.confirmation = None;
        self.error = None;
        match presets::load(name) {
            Ok(state) => {
                self.name.update(cx, |name, cx| name.clear(cx));
                (self.on_load)(state, window, cx);
            }
            Err(error) => {
                self.error = Some(format!("Could not load preset: {error}"));
                self.names = presets::list();
            }
        }
        cx.notify();
    }
}

impl Render for PresetPicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let overwriting = matches!(self.confirmation, Some(Confirmation::Overwrite(_)));
        div().flex().flex_col().gap_3()
            .child(div().font_family(theme::FONT_MONO).text_color(rgb(theme::colors().accent)).child("PRESETS"))
            .child(div().text_sm().text_color(rgb(theme::colors().text_dim)).child("Preset name"))
            .child(div().flex().items_center().gap_2()
                .child(div().flex_1().min_w_0().child(self.name.clone()))
                .child(tone_button("save-preset", if overwriting { "Overwrite?" } else { "Save" }, overwriting, theme::colors().orange)
                    .on_click(cx.listener(|this, _, _, cx| this.save(cx)))))
            .children(self.error.as_ref().map(|error| div().text_sm().text_color(rgb(theme::colors().red)).child(error.clone())))
            .child(div().id("saved-presets").h(px(300.)).overflow_y_scroll().flex().flex_col().gap_2()
                .when(self.names.is_empty(), |list| list.child(div().text_sm().text_color(rgb(theme::colors().text_dim)).child("No presets yet")))
                .children(self.names.iter().enumerate().map(|(index, name)| {
                    let deleting = matches!(self.confirmation.as_ref(), Some(Confirmation::Delete(pending)) if pending == name);
                    let load_name: gpui::SharedString = name.clone().into();
                    let delete_name = load_name.clone();
                    div().flex_none().flex().items_center().gap_2().py_2().border_b_1().border_color(rgb(theme::colors().border))
                        .child(div().flex_1().min_w_0().overflow_hidden().text_ellipsis().child(load_name.clone()))
                        .child(button(("load-preset", index), "Load", false).on_click(cx.listener(move |this, _, window, cx| this.load(&load_name, window, cx))))
                        .child(tone_button(("delete-preset", index), if deleting { "Delete?" } else { "Delete" }, deleting, theme::colors().red)
                            .on_click(cx.listener(move |this, _, _, cx| this.delete(&delete_name, cx))))
                })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overwrite_needs_two_clicks_for_the_same_name() {
        let mut pending = None;
        assert!(!confirm(
            &mut pending,
            Confirmation::Overwrite("Music".into())
        ));
        assert!(confirm(
            &mut pending,
            Confirmation::Overwrite("Music".into())
        ));
        assert_eq!(pending, None);
    }

    #[test]
    fn changing_name_or_action_never_reuses_a_confirmation() {
        let mut pending = None;
        assert!(!confirm(
            &mut pending,
            Confirmation::Overwrite("Music".into())
        ));
        assert!(!confirm(
            &mut pending,
            Confirmation::Overwrite("Games".into())
        ));
        assert!(!confirm(&mut pending, Confirmation::Delete("Games".into())));
        assert!(!confirm(&mut pending, Confirmation::Delete("Music".into())));
        assert!(confirm(&mut pending, Confirmation::Delete("Music".into())));
    }
}
