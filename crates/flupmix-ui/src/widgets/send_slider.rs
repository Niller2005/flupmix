use std::{cell::Cell, rc::Rc};

use flupmix_core::{db, settings::Settings};
use gpui::{
    canvas, div, prelude::*, px, relative, rgb, Bounds, Context, Entity, MouseButton,
    MouseMoveEvent, MouseUpEvent, Pixels, Render, Window,
};

use crate::{app::MixerModel, theme, widgets::drag_gain};

fn gain_at_x(x: Pixels, bounds: Bounds<Pixels>) -> f32 {
    let position = f32::from(x - bounds.origin.x) / f32::from(bounds.size.width).max(1.);
    db::db_from_pos(position)
}

pub(crate) struct SendSlider {
    model: Entity<MixerModel>,
    strip: u32,
    bus: Option<u32>,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    dragging: bool,
}

impl SendSlider {
    pub(crate) fn new(
        model: Entity<MixerModel>,
        strip: u32,
        bus: Option<u32>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&model, |_, _, cx| cx.notify()).detach();
        Self {
            model,
            strip,
            bus,
            bounds: Rc::new(Cell::new(Bounds::default())),
            dragging: false,
        }
    }

    fn gain(&self, cx: &gpui::App) -> f32 {
        self.model
            .read(cx)
            .state
            .strip(self.strip)
            .map_or(0., |strip| {
                self.bus.map_or(strip.hw_db, |bus| strip.bus_send_db(bus))
            })
    }

    fn preferences(&self, cx: &gpui::App) -> Settings {
        self.model.read(cx).settings.read(cx).values
    }

    fn set(&self, gain: f32, cx: &mut Context<Self>) {
        self.model.update(cx, |model, cx| {
            model.change(cx, |state| {
                if let Some(strip) = state.strip_mut(self.strip) {
                    let gain = gain.clamp(db::MIN_DB, db::MAX_DB);
                    match self.bus {
                        Some(bus) => strip.set_bus_send_db(bus, gain),
                        None => strip.hw_db = gain,
                    }
                }
            })
        });
    }

    fn at(&self, x: Pixels, cx: &mut Context<Self>) {
        let snap = self.preferences(cx).snap_to_zero;
        self.set(drag_gain(gain_at_x(x, self.bounds.get()), snap), cx);
    }
}

impl Render for SendSlider {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let gain = self.gain(cx);
        let enabled = self
            .model
            .read(cx)
            .state
            .strip(self.strip)
            .is_some_and(|strip| self.bus.map_or(strip.hw, |bus| strip.buses.contains(&bus)));
        let position = db::pos_from_db(gain);
        let bounds = self.bounds.clone();
        let on_move = cx.weak_entity();
        let on_up = cx.weak_entity();
        div()
            .h(px(20.))
            .flex_none()
            .flex()
            .items_center()
            .gap_1()
            .w_full()
            .opacity(if enabled { 1. } else { 0.45 })
            .child(
                div()
                    .id("send-level")
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .h(px(10.))
                    .border_1()
                    .border_color(rgb(theme::colors().border_strong))
                    .rounded_sm()
                    .bg(rgb(theme::colors().panel_sunken))
                    .cursor_pointer()
                    .child(
                        canvas(
                            move |b, _, _| bounds.set(b),
                            move |_, _, window, _| {
                                window.on_mouse_event(
                                    move |event: &MouseMoveEvent, phase, _, cx| {
                                        if phase.bubble() {
                                            let _ = on_move.update(cx, |this, cx| {
                                                if this.dragging && event.dragging() {
                                                    this.at(event.position.x, cx);
                                                }
                                            });
                                        }
                                    },
                                );
                                window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                                    if phase.bubble() {
                                        let _ = on_up.update(cx, |this, _| this.dragging = false);
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
                            .left_0()
                            .top_0()
                            .h_full()
                            .w(relative(position))
                            .bg(rgb(theme::colors().accent_dim)),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(relative(position))
                            .ml(px(-3.))
                            .top(px(-3.))
                            .w(px(6.))
                            .h(px(14.))
                            .rounded_sm()
                            .bg(rgb(theme::colors().accent)),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                            this.dragging = event.click_count != 2;
                            if event.click_count == 2 {
                                this.set(this.preferences(cx).reset_db, cx);
                            } else {
                                this.at(event.position.x, cx);
                            }
                            cx.stop_propagation();
                        }),
                    )
                    .on_scroll(cx.listener(|this, event: &gpui::ScrollEvent, _, cx| {
                        let delta = f32::from(event.delta.pixel_delta(px(20.)).y);
                        if delta != 0. {
                            let settings = this.preferences(cx);
                            let step = if event.modifiers.shift {
                                settings.wheel_fine_step_db
                            } else {
                                settings.wheel_step_db
                            };
                            this.set(this.gain(cx) + delta.signum() * step, cx);
                        }
                        cx.stop_propagation();
                    })),
            )
            .child(
                div()
                    .w(px(42.))
                    .flex_none()
                    .font_family(theme::FONT_MONO)
                    .text_xs()
                    .text_color(rgb(theme::colors().text))
                    .child(format!("{gain:+.1}")),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, size};

    #[test]
    fn pointer_position_uses_the_taper_and_clamps_outside_the_track() {
        let bounds = Bounds::new(point(px(40.), px(20.)), size(px(200.), px(10.)));
        assert!((gain_at_x(px(0.), bounds) - db::MIN_DB).abs() < 0.001);
        assert!((gain_at_x(px(300.), bounds) - db::MAX_DB).abs() < 0.001);
        let unity = bounds.origin.x + bounds.size.width * db::pos_from_db(0.);
        assert!(gain_at_x(unity, bounds).abs() < 0.001);
    }

    #[test]
    fn zero_width_track_still_produces_a_finite_gain() {
        assert!(gain_at_x(px(0.), Bounds::default()).is_finite());
    }
}
