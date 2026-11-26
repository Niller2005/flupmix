//! Content-only live settings editor; the shell owns modal dismissal.

use flupmix_core::settings::{self, Palette, Settings};
use gpui::{div, prelude::*, rgb, Context, Div, Entity, Render, Window};

use crate::{settings_model::SettingsModel, theme, widgets::button::button};

pub(crate) struct SettingsDialog {
    settings: Entity<SettingsModel>,
    error: Option<String>,
}

#[derive(Clone, Copy)]
enum Toggle {
    CloseToTray,
    StartMinimised,
    Autostart,
    RememberWindowSize,
    AppRouting,
    SnapToZero,
    Shortcuts,
}

impl Toggle {
    fn field(self, settings: &mut Settings) -> &mut bool {
        match self {
            Self::CloseToTray => &mut settings.close_to_tray,
            Self::StartMinimised => &mut settings.start_minimised,
            Self::Autostart => &mut settings.autostart,
            Self::RememberWindowSize => &mut settings.remember_window_size,
            Self::AppRouting => &mut settings.app_routing,
            Self::SnapToZero => &mut settings.snap_to_zero,
            Self::Shortcuts => &mut settings.shortcuts,
        }
    }
}

#[derive(Clone, Copy)]
enum Number {
    PeakHold,
    MeterFall,
    Clip,
    Wheel,
    FineWheel,
    Reset,
}

impl Number {
    fn field(self, settings: &mut Settings) -> &mut f32 {
        match self {
            Self::PeakHold => &mut settings.peak_hold_secs,
            Self::MeterFall => &mut settings.meter_fall_db_per_sec,
            Self::Clip => &mut settings.clip_db,
            Self::Wheel => &mut settings.wheel_step_db,
            Self::FineWheel => &mut settings.wheel_fine_step_db,
            Self::Reset => &mut settings.reset_db,
        }
    }

    fn range(self) -> (f32, f32, f32) {
        match self {
            Self::PeakHold => (0.0, 10.0, 0.5),
            Self::MeterFall => (6.0, 120.0, 6.0),
            Self::Clip => (-12.0, 6.0, 1.0),
            Self::Wheel => (0.1, 6.0, 0.1),
            Self::FineWheel => (0.05, 2.0, 0.05),
            Self::Reset => (-60.0, 12.0, 1.0),
        }
    }

    fn display(self, value: f32) -> String {
        match self {
            Self::PeakHold => format!("{value:.1} s"),
            Self::MeterFall => format!("{value:.0} dB/s"),
            Self::Clip | Self::Reset => format!("{value:.0} dB"),
            Self::Wheel => format!("{value:.1} dB"),
            Self::FineWheel => format!("{value:.2} dB"),
        }
    }
}

fn stepped(value: f32, minimum: f32, maximum: f32, step: f32, direction: f32) -> f32 {
    ((value / step + direction).round() * step).clamp(minimum, maximum)
}

fn section(label: &'static str) -> Div {
    div().flex().flex_col().gap_2().child(
        div()
            .text_sm()
            .font_family(theme::FONT_MONO)
            .text_color(rgb(theme::colors().text_dim))
            .child(label),
    )
}

fn row(label: &'static str) -> Div {
    div()
        .flex()
        .items_center()
        .gap_3()
        .min_h_8()
        .child(div().flex_1().min_w_0().text_sm().child(label))
}

impl SettingsDialog {
    pub(crate) fn new(settings: Entity<SettingsModel>, cx: &mut Context<Self>) -> Self {
        cx.observe(&settings, |_, _, cx| cx.notify()).detach();
        Self {
            settings,
            error: None,
        }
    }

    pub(crate) fn open(&mut self, cx: &mut Context<Self>) {
        self.error = None;
        cx.notify();
    }

    fn toggle(&mut self, toggle: Toggle, cx: &mut Context<Self>) {
        let mut values = self.settings.read(cx).values;
        let enabled = !*toggle.field(&mut values);
        if matches!(toggle, Toggle::Autostart) {
            self.error = None;
            if let Err(error) = settings::set_autostart(enabled) {
                eprintln!("flupmix: setting autostart: {error}");
                self.error = Some(format!("Could not change autostart: {error}"));
                cx.notify();
                return;
            }
            cx.notify();
        }
        self.settings.update(cx, |settings, cx| {
            settings.change(cx, |values| *toggle.field(values) = enabled);
        });
    }

    fn toggle_row(&self, label: &'static str, toggle: Toggle, cx: &Context<Self>) -> Div {
        let mut values = self.settings.read(cx).values;
        let enabled = *toggle.field(&mut values);
        row(label).child(
            button(
                ("settings-toggle", toggle as usize),
                if enabled { "[x] On" } else { "[ ] Off" },
                enabled,
            )
            .on_click(cx.listener(move |this, _, _, cx| this.toggle(toggle, cx))),
        )
    }

    fn step(&mut self, number: Number, direction: f32, cx: &mut Context<Self>) {
        let (minimum, maximum, step) = number.range();
        self.settings.update(cx, |settings, cx| {
            settings.change(cx, |values| {
                let value = number.field(values);
                *value = stepped(*value, minimum, maximum, step, direction);
            })
        });
    }

    fn number_row(&self, label: &'static str, number: Number, cx: &Context<Self>) -> Div {
        let mut values = self.settings.read(cx).values;
        let value = *number.field(&mut values);
        row(label).child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    button(("settings-decrease", number as usize), "−", false)
                        .on_click(cx.listener(move |this, _, _, cx| this.step(number, -1.0, cx))),
                )
                .child(
                    div()
                        .w_24()
                        .text_center()
                        .text_xs()
                        .font_family(theme::FONT_MONO)
                        .child(number.display(value)),
                )
                .child(
                    button(("settings-increase", number as usize), "+", false)
                        .on_click(cx.listener(move |this, _, _, cx| this.step(number, 1.0, cx))),
                ),
        )
    }

    fn meter_rate(&self, cx: &Context<Self>) -> Div {
        let rate = self.settings.read(cx).values.meter_hz;
        row("Refresh rate").child(div().flex().gap_1().children([30_u32, 60].map(|hz| {
            button(
                ("settings-meter-hz", hz as usize),
                format!("{hz} Hz"),
                rate == hz,
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.settings.update(cx, |settings, cx| {
                    settings.change(cx, |values| values.meter_hz = hz);
                });
            }))
        })))
    }

    fn appearance(&self, cx: &Context<Self>) -> Div {
        let selected = self.settings.read(cx).values.palette;
        section("Appearance").child(div().flex().flex_wrap().gap_1().children(Palette::ALL.map(
            |palette| {
                button(
                    ("settings-palette", palette as usize),
                    palette.label(),
                    selected == palette,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings.update(cx, |settings, cx| {
                        settings.change(cx, |values| values.palette = palette);
                    });
                }))
            },
        )))
    }
}

impl Render for SettingsDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("settings-content")
            .w_full()
            .max_h(gpui::px(500.))
            .min_h_0()
            .overflow_y_scroll()
            .text_color(rgb(theme::colors().text))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_5()
                    .pr_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(theme::colors().text_dim))
                            .child("Changes apply immediately and are saved automatically."),
                    )
                    .child(
                        section("General")
                            .child(self.toggle_row("Close to tray", Toggle::CloseToTray, cx))
                            .child(self.toggle_row("Start minimised", Toggle::StartMinimised, cx))
                            .child(self.toggle_row("Start at login", Toggle::Autostart, cx))
                            .children(self.error.as_ref().map(|error| {
                                div()
                                    .text_sm()
                                    .text_color(rgb(theme::colors().red))
                                    .child(error.clone())
                            }))
                            .child(self.toggle_row(
                                "Remember window size",
                                Toggle::RememberWindowSize,
                                cx,
                            ))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(theme::colors().text_dim))
                                    .child(
                                    "Close to tray and start minimised require a system tray host.",
                                ),
                            ),
                    )
                    .child(section("Routing").child(self.toggle_row(
                        "Route application audio",
                        Toggle::AppRouting,
                        cx,
                    )))
                    .child(
                        section("Meters")
                            .child(self.meter_rate(cx))
                            .child(self.number_row("Peak hold", Number::PeakHold, cx))
                            .child(self.number_row("Fall-off", Number::MeterFall, cx))
                            .child(self.number_row("Clip threshold", Number::Clip, cx)),
                    )
                    .child(
                        section("Faders")
                            .child(self.number_row("Wheel step", Number::Wheel, cx))
                            .child(self.number_row(
                                "Fine wheel step (Shift)",
                                Number::FineWheel,
                                cx,
                            ))
                            .child(self.number_row("Double-click reset level", Number::Reset, cx))
                            .child(self.toggle_row("Snap to 0 dB", Toggle::SnapToZero, cx)),
                    )
                    .child(self.appearance(cx))
                    .child(
                        section("Shortcuts")
                            .child(self.toggle_row(
                                "Enable keyboard shortcuts",
                                Toggle::Shortcuts,
                                cx,
                            ))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(theme::colors().text_dim))
                                    .child("Alt+1–9: mute the Nth input."),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(theme::colors().text_dim))
                                    .child(
                                    "Ctrl+Shift+1–9: load the Nth preset in alphabetical order.",
                                ),
                            ),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steppers_clamp_at_both_limits() {
        for number in [
            Number::PeakHold,
            Number::MeterFall,
            Number::Clip,
            Number::Wheel,
            Number::FineWheel,
            Number::Reset,
        ] {
            let (minimum, maximum, step) = number.range();
            assert_eq!(stepped(minimum, minimum, maximum, step, -1.0), minimum);
            assert_eq!(stepped(maximum, minimum, maximum, step, 1.0), maximum);
        }
    }

    #[test]
    fn fractional_steps_do_not_accumulate_drift() {
        let mut value = 0.05;
        for _ in 0..20 {
            value = stepped(value, 0.05, 2.0, 0.05, 1.0);
        }
        assert!((value - 1.05).abs() < 0.00001);
        for _ in 0..20 {
            value = stepped(value, 0.05, 2.0, 0.05, -1.0);
        }
        assert!((value - 0.05).abs() < 0.00001);
    }

    #[test]
    fn stepper_crosses_zero_and_formats_fine_values() {
        assert_eq!(stepped(-1.0, -12.0, 6.0, 1.0, 1.0), 0.0);
        assert_eq!(stepped(0.0, -12.0, 6.0, 1.0, -1.0), -1.0);
        assert_eq!(Number::FineWheel.display(0.05), "0.05 dB");
    }
}
