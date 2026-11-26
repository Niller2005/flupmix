use crate::{
    mixer_view::MixerView, patchbay::PatchbayView, theme, widgets::button::button, Tab, TrayEvent,
    UiOptions,
};
use flupmix_core::{config, EngineHandle, MixerState};
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
    save_task: Option<Task<()>>,
}

impl MixerModel {
    fn new(mut state: MixerState, engine: EngineHandle) -> Self {
        state.sanitize();
        engine.set_state(&state);
        Self {
            state,
            engine,
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
}

impl Render for RootView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(theme::BG))
            .text_color(rgb(theme::TEXT))
            .font_family(theme::FONT_UI)
            .track_focus(&self.focus)
            .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                let key = &event.keystroke;
                if key.modifiers.control && !key.modifiers.alt && !key.modifiers.platform {
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
                    .border_color(rgb(theme::BORDER))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .mr_6()
                            .text_2xl()
                            .child("flup")
                            .child(div().text_color(rgb(theme::ACCENT)).child("·mix")),
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
                                theme::GREEN
                            } else {
                                theme::BORDER
                            }))
                            .text_color(rgb(if self.connected {
                                theme::GREEN
                            } else {
                                theme::TEXT_DIM
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
    let bounds = Bounds::centered(None, size(px(1400.), px(800.)), cx);
    match cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(860.), px(650.))),
            app_id: Some("flupmix".into()),
            ..Default::default()
        },
        |window, cx| cx.new(|cx| RootView::new(model.clone(), initial_tab, window, cx)),
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

pub(crate) fn launch(opts: UiOptions, cx: &mut App) {
    theme::load_fonts(cx);
    let model = cx.new(|_| MixerModel::new(opts.initial, opts.engine));
    let tray = opts.tray;
    let has_tray = tray.is_some();
    let window = open(&model, opts.initial_tab, cx);
    let save_model = model.clone();
    cx.on_app_quit(move |cx| {
        save_model.read(cx).save();
        async {}
    })
    .detach();
    let close_model = model.clone();
    cx.on_window_closed(move |cx, _| {
        close_model.read(cx).save();
        if !has_tray && cx.windows().is_empty() {
            cx.quit();
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
                    model.read(cx).save();
                    cx.quit();
                }
            });
        }
    })
    .detach();
}
