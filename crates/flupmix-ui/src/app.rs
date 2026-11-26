use crate::{
    mixer_view::MixerView, patchbay::PatchbayView, settings_model::SettingsModel, theme,
    widgets::button::button, Tab, TrayEvent, UiOptions,
};
use flupmix_core::{config, presets, settings, EngineHandle, MixerState};
use gpui::{
    div, prelude::*, px, rgb, size, App, Bounds, Context, Entity, FocusHandle, Render, Task,
    Window, WindowBounds, WindowHandle, WindowOptions,
};
use std::{sync::mpsc::Receiver, time::Duration};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Target {
    Strip(u32),
    Bus(u32),
    Hardware,
}

impl Target {
    pub(crate) fn set_gain(self, state: &mut MixerState, gain: f32) {
        match self {
            Self::Strip(id) => {
                if let Some(s) = state.strip_mut(id) {
                    s.gain_db = gain;
                }
            }
            Self::Bus(id) => {
                if let Some(b) = state.bus_mut(id) {
                    b.gain_db = gain;
                }
            }
            Self::Hardware => state.hw.gain_db = gain,
        }
    }
}

pub(crate) struct MixerModel {
    pub(crate) state: MixerState,
    pub(crate) engine: EngineHandle,
    pub(crate) settings: Entity<SettingsModel>,
    save_task: Option<Task<()>>,
}

impl MixerModel {
    fn new(mut state: MixerState, engine: EngineHandle, settings: Entity<SettingsModel>) -> Self {
        state.sanitize();
        engine.set_state(&state);
        Self {
            state,
            engine,
            settings,
            save_task: None,
        }
    }

    pub(crate) fn change(&mut self, cx: &mut Context<Self>, change: impl FnOnce(&mut MixerState)) {
        change(&mut self.state);
        self.engine.set_state(&self.state);
        self.save_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(500))
                .await;
            let _ = this.update(cx, |this, _| this.save());
        }));
        cx.notify();
    }

    fn save(&self) {
        if let Err(error) = config::save(&self.state) {
            eprintln!("flupmix: saving configuration: {error}");
        }
    }
}

struct RootView {
    mixer: Entity<MixerView>,
    model: Entity<MixerModel>,
    settings: Entity<SettingsModel>,
    patchbay: Entity<PatchbayView>,
    tab: Tab,
    focus: FocusHandle,
    connected: bool,
}

impl RootView {
    fn new(
        model: Entity<MixerModel>,
        tab: Tab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let engine = model.read(cx).engine.clone();
        let settings = model.read(cx).settings.clone();
        let connected = engine.shared.snapshot().connected;
        let mixer = cx.new(|cx| MixerView::new(model.clone(), cx));
        mixer.update(cx, |mixer, _| mixer.set_visible(tab == Tab::Mixer));
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let patchbay = cx.new(|cx| PatchbayView::new(engine.clone(), cx));
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(250))
                .await;
            if this
                .update(cx, |this, cx| {
                    let connected = engine.shared.snapshot().connected;
                    if this.connected != connected {
                        this.connected = connected;
                        cx.notify();
                    }
                })
                .is_err()
            {
                break;
            }
        })
        .detach();
        Self {
            model,
            settings,
            mixer,
            patchbay,
            tab,
            focus,
            connected,
        }
    }

    fn select_tab(&mut self, tab: Tab, window: &mut Window, cx: &mut Context<Self>) {
        if self.tab != tab {
            self.tab = tab;
            self.mixer
                .update(cx, |mixer, _| mixer.set_visible(tab == Tab::Mixer));
            cx.notify();
            self.focus.focus(window, cx);
        }
    }

    fn shortcut(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.settings.read(cx).values.shortcuts {
            return false;
        }
        let key = &event.keystroke;
        let Some(index) = shortcut_index(&key.key) else {
            return false;
        };
        if key.modifiers.alt
            && !key.modifiers.control
            && !key.modifiers.platform
            && !key.modifiers.shift
        {
            let strip = self
                .model
                .read(cx)
                .state
                .strips
                .get(index)
                .map(|strip| strip.id);
            if let Some(id) = strip {
                self.model.update(cx, |model, cx| {
                    model.change(cx, |state| {
                        if let Some(strip) = state.strip_mut(id) {
                            strip.mute = !strip.mute;
                        }
                    })
                });
            }
            return true;
        }
        if key.modifiers.control
            && key.modifiers.shift
            && !key.modifiers.alt
            && !key.modifiers.platform
        {
            if let Some(name) = presets::list().get(index) {
                match presets::load(name) {
                    Ok(state) => self
                        .mixer
                        .update(cx, |mixer, cx| mixer.load_preset(state, window, cx)),
                    Err(error) => eprintln!("flupmix: loading preset {name}: {error}"),
                }
            }
            self.focus.focus(window, cx);
            return true;
        }
        false
    }
}

fn shortcut_index(key: &str) -> Option<usize> {
    // Some keyboard backends report the shifted character for Ctrl+Shift+digit.
    match key {
        "1" | "!" => Some(0),
        "2" | "@" => Some(1),
        "3" | "#" => Some(2),
        "4" | "$" => Some(3),
        "5" | "%" => Some(4),
        "6" | "^" => Some(5),
        "7" | "&" => Some(6),
        "8" | "*" => Some(7),
        "9" | "(" => Some(8),
        _ => None,
    }
}

impl Render for RootView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(theme::colors().bg))
            .text_color(rgb(theme::colors().text))
            .font_family(theme::FONT_UI)
            .track_focus(&self.focus)
            .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if this.shortcut(event, window, cx) {
                    cx.stop_propagation();
                    return;
                }
                let key = &event.keystroke;
                if key.modifiers.control
                    && !key.modifiers.shift
                    && !key.modifiers.alt
                    && !key.modifiers.platform
                {
                    let tab = match key.key.as_str() {
                        "1" => Some(Tab::Mixer),
                        "2" => Some(Tab::Patchbay),
                        _ => None,
                    };
                    if let Some(tab) = tab {
                        this.select_tab(tab, window, cx);
                        cx.stop_propagation();
                    }
                }
            }))
            .child(
                div()
                    .h(px(68.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_4()
                    .px_6()
                    .border_b_1()
                    .border_color(rgb(theme::colors().border))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .mr_6()
                            .text_2xl()
                            .child("flup")
                            .child(div().text_color(rgb(theme::colors().accent)).child("·mix")),
                    )
                    .child(
                        button("mixer-tab", "Mixer", self.tab == Tab::Mixer).on_click(cx.listener(
                            |this, _, window, cx| this.select_tab(Tab::Mixer, window, cx),
                        )),
                    )
                    .child(
                        button("patchbay-tab", "Patchbay", self.tab == Tab::Patchbay).on_click(
                            cx.listener(|this, _, window, cx| {
                                this.select_tab(Tab::Patchbay, window, cx)
                            }),
                        ),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .rounded_full()
                            .px_3()
                            .py_2()
                            .text_xs()
                            .border_1()
                            .border_color(rgb(if self.connected {
                                theme::colors().green
                            } else {
                                theme::colors().border
                            }))
                            .text_color(rgb(if self.connected {
                                theme::colors().green
                            } else {
                                theme::colors().text_dim
                            }))
                            .child(if self.connected {
                                "● PipeWire connected"
                            } else {
                                "● Connecting…"
                            }),
                    ),
            )
            .child(div().flex_1().min_h_0().child(match self.tab {
                Tab::Patchbay => self.patchbay.clone().into_any_element(),
                Tab::Mixer => self.mixer.clone().into_any_element(),
            }))
    }
}

fn open(
    model: &Entity<MixerModel>,
    initial_tab: Tab,
    cx: &mut App,
) -> Option<WindowHandle<RootView>> {
    let settings = model.read(cx).settings.clone();
    let values = settings.read(cx).values;
    let (width, height) = values
        .window_size
        .filter(|_| values.remember_window_size)
        .unwrap_or((1400., 800.));
    let bounds = Bounds::centered(None, size(px(width.max(860.)), px(height.max(650.))), cx);
    match cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(860.), px(650.))),
            app_id: Some("flupmix".into()),
            ..Default::default()
        },
        |window, cx| {
            window.on_window_should_close(cx, move |window, cx| {
                remember_window_size(&settings, window, cx);
                true
            });
            cx.new(|cx| RootView::new(model.clone(), initial_tab, window, cx))
        },
    ) {
        Ok(window) => {
            cx.activate(true);
            Some(window)
        }
        Err(error) => {
            eprintln!("flupmix: opening window: {error:#}");
            None
        }
    }
}

fn remember_window_size(settings: &Entity<SettingsModel>, window: &Window, cx: &mut App) {
    let size = window.viewport_size();
    let logical = (f32::from(size.width), f32::from(size.height));
    let current = settings.read(cx).values;
    if current.remember_window_size && current.window_size != Some(logical) {
        settings.update(cx, |settings, cx| {
            settings.change(cx, |values| values.window_size = Some(logical))
        });
    }
}

pub(crate) fn launch(opts: UiOptions, cx: &mut App) {
    theme::load_fonts(cx);
    let settings = cx.new(|cx| SettingsModel::new(settings::load(), opts.engine.clone(), cx));
    let model = cx.new(|_| MixerModel::new(opts.initial, opts.engine, settings.clone()));
    let tray = opts.tray;
    let has_tray = tray.is_some();
    let window = if has_tray && settings.read(cx).values.start_minimised {
        None
    } else {
        open(&model, opts.initial_tab, cx)
    };
    let save_model = model.clone();
    let save_settings = settings.clone();
    cx.on_app_quit(move |cx| {
        for window in cx.windows() {
            let _ = window.update(cx, |_, window, cx| {
                remember_window_size(&save_settings, window, cx)
            });
        }
        save_settings.read(cx).save();
        save_model.read(cx).save();
        async {}
    })
    .detach();
    let close_model = model.clone();
    cx.on_window_closed(move |cx, _| {
        let settings = close_model.read(cx).settings.clone();
        if !has_tray || !settings.read(cx).values.close_to_tray {
            if cx.windows().is_empty() {
                cx.quit();
            }
        } else {
            close_model.read(cx).save();
            settings.read(cx).save();
        }
    })
    .detach();
    if let Some(tray) = tray {
        poll_tray(tray, model, window, opts.initial_tab, cx);
    } else if window.is_none() {
        cx.quit();
    }
}

fn poll_tray(
    tray: Receiver<TrayEvent>,
    model: Entity<MixerModel>,
    mut window: Option<WindowHandle<RootView>>,
    initial_tab: Tab,
    cx: &mut App,
) {
    cx.spawn(async move |cx| loop {
        cx.background_executor()
            .timer(Duration::from_millis(100))
            .await;
        while let Ok(event) = tray.try_recv() {
            cx.update(|cx| match event {
                TrayEvent::Show => {
                    let existing = window
                        .and_then(|w| w.update(cx, |_, window, _| window.activate_window()).ok());
                    if existing.is_none() {
                        window = open(&model, initial_tab, cx);
                    }
                }
                TrayEvent::Quit => {
                    cx.quit();
                }
            });
        }
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbered_shortcuts_accept_digit_and_shifted_keys_only() {
        for (index, key) in ["1", "2", "3", "4", "5", "6", "7", "8", "9"]
            .into_iter()
            .enumerate()
        {
            assert_eq!(shortcut_index(key), Some(index));
        }
        assert_eq!(shortcut_index("!"), Some(0));
        assert_eq!(shortcut_index("("), Some(8));
        assert_eq!(shortcut_index("0"), None);
        assert_eq!(shortcut_index("a"), None);
    }
}
