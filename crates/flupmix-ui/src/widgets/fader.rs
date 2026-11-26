use crate::{
    app::{MixerModel, Target},
    settings_model::SettingsModel,
    theme,
    widgets::drag_gain,
};
use flupmix_core::{db, settings::MeterStyle, EngineHandle};
use gpui::{
    canvas, div, prelude::*, px, relative, rgb, Bounds, Context, Div, MouseButton, MouseMoveEvent,
    MouseUpEvent, Pixels, Render, Window,
};
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};

pub(crate) struct Fader {
    model: gpui::Entity<MixerModel>,
    settings: gpui::Entity<SettingsModel>,
    target: Target,
    engine: EngineHandle,
    slot: usize,
    destination: bool,
    gain: f32,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    dragging: bool,
    levels: [f32; 2],
    holds: [f32; 2],
    hold_since: [Instant; 2],
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
        let settings = model.read(cx).settings.clone();
        cx.observe(&settings, |_, _, cx| cx.notify()).detach();
        let now = Instant::now();
        Self {
            model,
            settings,
            target,
            engine,
            slot,
            destination,
            gain,
            bounds: Rc::new(Cell::new(Bounds::default())),
            dragging: false,
            levels: [-60.; 2],
            holds: [-60.; 2],
            hold_since: [now; 2],
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
        let snap = self.settings.read(cx).values.snap_to_zero;
        self.set(drag_gain(db::db_from_pos(position), snap), cx);
    }

    fn sample(&mut self, cx: &mut Context<Self>) {
        let now = Instant::now();
        let preferences = self.settings.read(cx).values;
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
            let level = peak.max(self.levels[i] - preferences.meter_fall_db_per_sec * dt);
            if (level - self.levels[i]).abs() > 0.01 {
                changed = true;
            }
            self.levels[i] = level;
            let previous = self.holds[i];
            if peak >= self.holds[i] {
                self.holds[i] = peak;
                self.hold_since[i] = now;
            } else if now.duration_since(self.hold_since[i]).as_secs_f32()
                >= preferences.peak_hold_secs
            {
                self.holds[i] = self.holds[i].max(level) - preferences.meter_fall_db_per_sec * dt;
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
            let mut hz = self.settings.read(cx).values.meter_hz;
            cx.spawn_in(window, async move |this, cx| loop {
                cx.background_executor()
                    .timer(Duration::from_secs_f64(1. / f64::from(hz)))
                    .await;
                let rate = cx
                    .update(|window, cx| {
                        this.update(cx, |this, cx| {
                            if window.is_visible() && this.visible.get() {
                                this.sample(cx);
                            }
                            this.settings.read(cx).values.meter_hz
                        })
                        .ok()
                    })
                    .ok()
                    .flatten();
                if let Some(rate) = rate {
                    hz = rate;
                } else {
                    break;
                }
            })
            .detach();
        }
        let bounds = self.bounds.clone();
        let weak_move = cx.weak_entity();
        let weak_up = cx.weak_entity();
        let thumb_y = 1. - db::pos_from_db(self.gain);
        let clip_db = self.settings.read(cx).values.clip_db;
        let clip = self.holds.iter().any(|p| *p > clip_db);
        let led = self.settings.read(cx).values.meter_style == MeterStyle::Led;
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
                    .text_color(rgb(if clip {
                        theme::colors().red
                    } else {
                        theme::colors().text_dim
                    }))
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
                            .bg(rgb(theme::colors().border_strong)),
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
                                        theme::colors().text
                                    } else {
                                        theme::colors().text_dim
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
                            .border_color(rgb(theme::colors().border_strong))
                            .bg(rgb(theme::colors().panel_sunken))
                            .children(if led {
                                led_segments(self.levels[i], self.holds[i])
                            } else {
                                bar_segments(self.levels[i])
                            })
                            .when(!led, |track| {
                                track.child(
                                    div()
                                        .absolute()
                                        .top(relative(hold_y))
                                        .mt(px(-1.))
                                        .w_full()
                                        .h(px(2.))
                                        .bg(gpui::white()),
                                )
                            })
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
                            .border_color(rgb(theme::colors().accent))
                            .bg(rgb(theme::colors().text))
                            .child(
                                div()
                                    .mt(px(6.))
                                    .h(px(2.))
                                    .w_full()
                                    .bg(rgb(theme::colors().accent_dim)),
                            ),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, ev: &gpui::MouseDownEvent, _, cx| {
                            this.dragging = true;
                            if ev.click_count == 2 {
                                this.set(this.settings.read(cx).values.reset_db, cx);
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
                            let settings = this.settings.read(cx).values;
                            let step = if ev.modifiers.shift {
                                settings.wheel_fine_step_db
                            } else {
                                settings.wheel_step_db
                            };
                            this.set(this.gain + delta.signum() * step, cx);
                        }
                        cx.stop_propagation();
                    })),
            )
            .child(
                div()
                    .flex_none()
                    .font_family(theme::FONT_MONO)
                    .text_sm()
                    .text_color(rgb(theme::colors().accent))
                    .child(format!("{:+.1} dB", self.gain)),
            )
    }
}

const LED_SEGMENTS: usize = 36;

fn zone_color(db: f32) -> u32 {
    let colors = theme::colors();
    if db >= -3. {
        colors.red
    } else if db >= -12. {
        colors.orange
    } else {
        colors.green
    }
}

fn bar_segments(level: f32) -> Vec<Div> {
    [
        (db::MIN_DB, -12., theme::colors().green),
        (-12., -3., theme::colors().orange),
        (-3., db::MAX_DB, theme::colors().red),
    ]
    .into_iter()
    .map(|(low, high, color)| {
        let bottom = db::pos_from_db(low);
        let top = db::pos_from_db(level.clamp(low, high));
        div()
            .absolute()
            .bottom(relative(bottom))
            .w_full()
            .h(relative(top - bottom))
            .bg(rgb(color))
    })
    .collect()
}

/// A ladder of equal-height LEDs along the fader taper: lit up to the level,
/// with the held peak shown as one extra lit segment.
fn led_segments(level: f32, hold: f32) -> Vec<Div> {
    let step = 1. / LED_SEGMENTS as f32;
    let level_pos = db::pos_from_db(level);
    let hold_segment = (db::pos_from_db(hold) / step) as usize;
    (0..LED_SEGMENTS)
        .map(|k| {
            let low = k as f32 * step;
            let color = zone_color(db::db_from_pos(low + step / 2.));
            let lit = level_pos >= low + step / 2. || (k == hold_segment && hold > db::MIN_DB + 1.);
            div()
                .absolute()
                .bottom(relative(low))
                .w_full()
                .h(relative(step))
                .py(px(1.))
                .child(
                    div()
                        .size_full()
                        .bg(rgb(if lit { color } else { dim(color) })),
                )
        })
        .collect()
}

/// An unlit LED: the zone colour at about 18% over the dark track.
fn dim(color: u32) -> u32 {
    let base = theme::colors().panel_sunken;
    let mix = |shift: u32| {
        let c = (color >> shift) & 0xff;
        let b = (base >> shift) & 0xff;
        (b * 82 + c * 18) / 100
    };
    (mix(16) << 16) | (mix(8) << 8) | mix(0)
}
