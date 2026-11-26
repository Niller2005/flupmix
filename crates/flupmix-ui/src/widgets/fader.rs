use crate::{
    app::{MixerModel, Target},
    theme,
};
use flupmix_core::{db, EngineHandle};
use gpui::{
    canvas, div, prelude::*, px, relative, rgb, Bounds, Context, MouseButton, MouseMoveEvent,
    MouseUpEvent, Pixels, Render, Window,
};
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};

pub(crate) struct Fader {
    model: gpui::Entity<MixerModel>,
    target: Target,
    engine: EngineHandle,
    slot: usize,
    destination: bool,
    gain: f32,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    dragging: bool,
    levels: [f32; 2],
    holds: [f32; 2],
    hold_until: [Instant; 2],
    last: Instant,
    ticking: bool,
    visible: Rc<Cell<bool>>,
}

impl Fader {
    pub(crate) fn new(
        model: gpui::Entity<MixerModel>,
        target: Target,
        slot: usize,
        destination: bool,
        gain: f32,
        visible: Rc<Cell<bool>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let engine = model.read(cx).engine.clone();
        let now = Instant::now();
        Self {
            model,
            target,
            engine,
            slot,
            destination,
            gain,
            bounds: Rc::new(Cell::new(Bounds::default())),
            dragging: false,
            levels: [-60.; 2],
            holds: [-60.; 2],
            hold_until: [now; 2],
            last: now,
            ticking: false,
            visible,
        }
    }

    fn set(&mut self, gain: f32, cx: &mut Context<Self>) {
        self.gain = gain.clamp(db::MIN_DB, db::MAX_DB);
        self.model.update(cx, |model, cx| {
            model.change(cx, |state| self.target.set_gain(state, self.gain))
        });
        cx.notify();
    }

    fn at(&mut self, y: Pixels, cx: &mut Context<Self>) {
        let b = self.bounds.get();
        let position = 1. - f32::from(y - b.origin.y) / f32::from(b.size.height).max(1.);
        self.set(db::db_from_pos(position), cx);
    }

    fn sample(&mut self, cx: &mut Context<Self>) {
        let now = Instant::now();
        let dt = now.duration_since(self.last).as_secs_f32().min(0.1);
        self.last = now;
        let pair = if self.destination {
            self.engine.shared.meters.dests[self.slot].take()
        } else {
            self.engine.shared.meters.strips[self.slot].take()
        };
        let mut changed = false;
        for (i, sample) in [pair.0, pair.1].into_iter().enumerate() {
            let peak = db::gain_to_db(sample).min(12.);
            let level = peak.max(self.levels[i] - 28. * dt);
            if (level - self.levels[i]).abs() > 0.01 {
                changed = true;
            }
            self.levels[i] = level;
            let previous = self.holds[i];
            if peak >= self.holds[i] {
                self.holds[i] = peak;
                self.hold_until[i] = now + Duration::from_millis(1500);
            } else if now >= self.hold_until[i] {
                self.holds[i] = self.holds[i].max(level) - 15. * dt;
                self.holds[i] = self.holds[i].max(level);
            }
            changed |= (previous - self.holds[i]).abs() > 0.01;
        }
        if changed {
            cx.notify();
        }
    }
}

impl Render for Fader {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.ticking {
            self.ticking = true;
            cx.spawn_in(window, async move |this, cx| loop {
                cx.background_executor()
                    .timer(Duration::from_millis(33))
                    .await;
                let alive = cx
                    .update(|window, cx| {
                        this.update(cx, |this, cx| {
                            if window.is_visible() && this.visible.get() {
                                this.sample(cx);
                            }
                        })
                        .is_ok()
                    })
                    .unwrap_or(false);
                if !alive {
                    break;
                }
            })
            .detach();
        }
        let bounds = self.bounds.clone();
        let weak_move = cx.weak_entity();
        let weak_up = cx.weak_entity();
        let thumb_y = 1. - db::pos_from_db(self.gain);
        let clip = self.holds.iter().any(|p| *p > 0.);
        div()
            .flex_1()
            .min_h(px(344.))
            .flex()
            .flex_col()
            .items_center()
            .gap_2()
            .py_2()
            .child(
                div()
                    .flex_none()
                    .font_family(theme::FONT_MONO)
                    .text_xs()
                    .text_color(rgb(if clip { theme::RED } else { theme::TEXT_DIM }))
                    .child(if clip { "● CLIP" } else { "● PEAK" }),
            )
            .child(
                div()
                    .id("fader")
                    .relative()
                    .w(px(112.))
                    .flex_1()
                    .min_h(px(260.))
                    .my_2()
                    .cursor_pointer()
                    .child(
                        canvas(
                            move |b, _, _| {
                                bounds.set(b);
                            },
                            move |_, _, window, _| {
                                window.on_mouse_event(
                                    move |event: &MouseMoveEvent, phase, _, cx| {
                                        if phase.bubble() {
                                            let _ = weak_move.update(cx, |this, cx| {
                                                if this.dragging && event.dragging() {
                                                    this.at(event.position.y, cx);
                                                }
                                            });
                                        }
                                    },
                                );
                                window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                                    if phase.bubble() {
                                        let _ = weak_up.update(cx, |this, _| this.dragging = false);
                                    }
                                });
                            },
                        )
                        .absolute()
                        .inset_0(),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(px(24.))
                            .top_0()
                            .w(px(4.))
                            .h_full()
                            .rounded_full()
                            .bg(rgb(theme::BORDER_STRONG)),
                    )
                    .children(
                        [12., 6., 0., -6., -20., -40., -60.]
                            .into_iter()
                            .map(|tick| {
                                div()
                                    .absolute()
                                    .left(px(39.))
                                    .top(relative(1. - db::pos_from_db(tick)))
                                    .mt(px(-7.))
                                    .text_color(rgb(if tick == 0. {
                                        theme::TEXT
                                    } else {
                                        theme::TEXT_DIM
                                    }))
                                    .font_family(theme::FONT_MONO)
                                    .text_xs()
                                    .child(format!("{tick:+.0}"))
                            }),
                    )
                    .children((0..2).map(|i| {
                        let hold_y = 1. - db::pos_from_db(self.holds[i]);
                        div()
                            .absolute()
                            .left(px(82. + i as f32 * 12.))
                            .top_0()
                            .w(px(10.))
                            .h_full()
                            .border_1()
                            .border_color(rgb(theme::BORDER_STRONG))
                            .bg(rgb(theme::PANEL_SUNKEN))
                            .children(
                                [
                                    (db::MIN_DB, -12., theme::GREEN),
                                    (-12., -3., theme::ORANGE),
                                    (-3., db::MAX_DB, theme::RED),
                                ]
                                .into_iter()
                                .map(|(low, high, color)| {
                                    let bottom = db::pos_from_db(low);
                                    let top = db::pos_from_db(self.levels[i].clamp(low, high));
                                    div()
                                        .absolute()
                                        .bottom(relative(bottom))
                                        .w_full()
                                        .h(relative(top - bottom))
                                        .bg(rgb(color))
                                }),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .top(relative(hold_y))
                                    .mt(px(-1.))
                                    .w_full()
                                    .h(px(2.))
                                    .bg(gpui::white()),
                            )
                    }))
                    .child(
                        div()
                            .absolute()
                            .top(relative(thumb_y))
                            .mt(px(-7.))
                            .left(px(15.))
                            .w(px(22.))
                            .h(px(14.))
                            .rounded_sm()
                            .border_1()
                            .border_color(rgb(theme::ACCENT))
                            .bg(rgb(theme::TEXT))
                            .child(
                                div()
                                    .mt(px(6.))
                                    .h(px(2.))
                                    .w_full()
                                    .bg(rgb(theme::ACCENT_DIM)),
                            ),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, ev: &gpui::MouseDownEvent, _, cx| {
                            this.dragging = true;
                            if ev.click_count == 2 {
                                this.set(0., cx);
                                this.dragging = false;
                            } else {
                                this.at(ev.position.y, cx);
                            }
                            cx.stop_propagation();
                        }),
                    )
                    .on_scroll(cx.listener(|this, ev: &gpui::ScrollEvent, _, cx| {
                        let delta = f32::from(ev.delta.pixel_delta(px(20.)).y);
                        if delta != 0. {
                            this.set(
                                this.gain
                                    + delta.signum() * if ev.modifiers.shift { 0.1 } else { 0.5 },
                                cx,
                            );
                        }
                        cx.stop_propagation();
                    })),
            )
            .child(
                div()
                    .flex_none()
                    .font_family(theme::FONT_MONO)
                    .text_sm()
                    .text_color(rgb(theme::ACCENT))
                    .child(format!("{:+.1} dB", self.gain)),
            )
    }
}
