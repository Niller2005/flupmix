use crate::{
    app::{MixerModel, Target},
    preset_picker::PresetPicker,
    theme,
    widgets::{
        button::{button, tone_button},
        fader::Fader,
        send_slider::SendSlider,
        text_field::TextField,
    },
};
use flupmix_core::{
    mix::{MAX_BUSES, MAX_STRIPS},
    rules, MixerState, Snapshot, StripSource,
};
use gpui::{
    div, prelude::*, px, rgb, App, Context, Entity, FocusHandle, IntoElement, Render, Window,
};
use std::{cell::Cell, collections::HashMap, rc::Rc, sync::Arc, time::Duration};

// Reserve identical routing/control space below every fader, including bus cards.
fn control_stack_height(bus_count: usize) -> gpui::Pixels {
    px(212. + bus_count as f32 * 64.)
}

fn bus_section_width(bus_count: usize, window_width: gpui::Pixels) -> gpui::Pixels {
    // Include the section's left gutter/border and every inter-card gap.
    let natural = 17. + (bus_count + 1) as f32 * 150. + bus_count as f32 * 12.;
    let available = (f32::from(window_width) - 32. - 16. - 164.).max(150.);
    px(natural.min(available))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Menu {
    AddStrip,
    Source(u32),
    Sink,
    Apps(u32),
    Presets,
}

pub(crate) struct MixerView {
    model: Entity<MixerModel>,
    snapshot: Arc<Snapshot>,
    faders: HashMap<Target, Entity<Fader>>,
    send_sliders: HashMap<(u32, Option<u32>), Entity<SendSlider>>,
    names: HashMap<Target, Entity<TextField>>,
    pattern: Entity<TextField>,
    presets: Entity<PresetPicker>,
    menu: Option<Menu>,
    focus: FocusHandle,
    visible: Rc<Cell<bool>>,
}

impl MixerView {
    pub(crate) fn new(model: Entity<MixerModel>, cx: &mut Context<Self>) -> Self {
        let snapshot = model.read(cx).engine.shared.snapshot();
        let view = cx.weak_entity();
        let preset_picker = cx.new(|cx| {
            PresetPicker::new(
                model.clone(),
                move |state, window, cx| {
                    let _ = view.update(cx, |view, cx| view.load_preset(state, window, cx));
                },
                cx,
            )
        });
        cx.observe(&model, |_, _, cx| cx.notify()).detach();
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(250))
                .await;
            if this
                .update(cx, |this, cx| {
                    let snapshot = this.model.read(cx).engine.shared.snapshot();
                    if snapshot.sources != this.snapshot.sources
                        || snapshot.sinks != this.snapshot.sinks
                        || snapshot.apps != this.snapshot.apps
                    {
                        this.snapshot = snapshot;
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
            snapshot,
            faders: HashMap::new(),
            send_sliders: HashMap::new(),
            names: HashMap::new(),
            pattern: cx.new(|cx| TextField::new(String::new(), |_, _| {}, cx)),
            presets: preset_picker,
            menu: None,
            focus: cx.focus_handle(),
            visible: Rc::new(Cell::new(true)),
        }
    }

    pub(crate) fn set_visible(&self, visible: bool) {
        self.visible.set(visible);
    }

    fn show(&mut self, menu: Menu, window: &mut Window, cx: &mut Context<Self>) {
        if menu == Menu::Presets {
            self.presets.update(cx, |presets, cx| presets.open(cx));
        }
        self.menu = Some(menu);
        self.focus.focus(window, cx);
        cx.notify();
    }

    fn change(&self, cx: &mut App, change: impl FnOnce(&mut flupmix_core::MixerState)) {
        self.model.update(cx, |model, cx| model.change(cx, change));
    }

    fn load_preset(&mut self, state: MixerState, window: &mut Window, cx: &mut Context<Self>) {
        // Cancel before dropping editors: an old name must never commit into a reused ID.
        for field in self.names.values() {
            field.update(cx, |field, cx| field.cancel(cx));
        }
        self.names.clear();
        self.faders.clear();
        self.send_sliders.clear();
        self.pattern.update(cx, |pattern, cx| pattern.clear(cx));
        self.menu = None;
        self.change(cx, |current| *current = state);
        self.focus.focus(window, cx);
        cx.notify();
    }

    fn name(&mut self, target: Target, text: &str, cx: &mut Context<Self>) -> Entity<TextField> {
        self.names
            .entry(target)
            .or_insert_with(|| {
                let model = self.model.clone();
                cx.new(|cx| {
                    TextField::new(
                        text.into(),
                        move |name, cx| {
                            model.update(cx, |model, cx| {
                                model.change(cx, |state| match target {
                                    Target::Strip(id) => {
                                        if let Some(s) = state.strip_mut(id) {
                                            s.name = name.into();
                                        }
                                    }
                                    Target::Bus(id) => {
                                        if let Some(b) = state.bus_mut(id) {
                                            b.name = name.into();
                                        }
                                    }
                                    Target::Hardware => state.hw.name = name.into(),
                                })
                            });
                        },
                        cx,
                    )
                })
            })
            .clone()
    }

    fn fader(
        &mut self,
        target: Target,
        slot: usize,
        destination: bool,
        gain: f32,
        cx: &mut Context<Self>,
    ) -> Entity<Fader> {
        self.faders
            .entry(target)
            .or_insert_with(|| {
                cx.new(|cx| {
                    Fader::new(
                        self.model.clone(),
                        target,
                        slot,
                        destination,
                        gain,
                        self.visible.clone(),
                        cx,
                    )
                })
            })
            .clone()
    }

    fn send_slider(
        &mut self,
        strip: u32,
        bus: Option<u32>,
        cx: &mut Context<Self>,
    ) -> Entity<SendSlider> {
        self.send_sliders
            .entry((strip, bus))
            .or_insert_with(|| cx.new(|cx| SendSlider::new(self.model.clone(), strip, bus, cx)))
            .clone()
    }

    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let state = &self.model.read(cx).state;
        let inputs = state.strips.len();
        let buses = state.buses.len();
        let last_strip = state.strips.last().map(|s| s.id);
        let last_bus = state.buses.last().map(|b| b.id);
        div()
            .flex()
            .items_center()
            .gap_3()
            .h(px(54.))
            .flex_none()
            .px_5()
            .child(
                div()
                    .font_family(theme::FONT_MONO)
                    .text_xs()
                    .text_color(rgb(theme::TEXT_DIM))
                    .child("INPUTS"),
            )
            .child(button("remove-input", "−", false).on_click(cx.listener(
                move |this, _, _, cx| {
                    if let Some(id) = last_strip {
                        this.change(cx, |s| s.remove_strip(id));
                    }
                },
            )))
            .child(format!("{inputs} / {MAX_STRIPS}"))
            .child(
                button("add-input", "+", false).on_click(cx.listener(move |this, _, w, cx| {
                    if inputs < MAX_STRIPS {
                        this.show(Menu::AddStrip, w, cx);
                    }
                })),
            )
            .child(
                button("presets", "Presets", self.menu == Some(Menu::Presets)).on_click(
                    cx.listener(|this, _, window, cx| this.show(Menu::Presets, window, cx)),
                ),
            )
            .child(div().flex_1())
            .child(
                div()
                    .font_family(theme::FONT_MONO)
                    .text_xs()
                    .text_color(rgb(theme::TEXT_DIM))
                    .child("BUSES"),
            )
            .child(
                button("remove-bus", "−", false).on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(id) = last_bus {
                        this.change(cx, |s| s.remove_bus(id));
                    }
                })),
            )
            .child(format!("{buses} / {MAX_BUSES}"))
            .child(
                button("add-bus", "+", false).on_click(cx.listener(move |this, _, _, cx| {
                    if buses < MAX_BUSES {
                        this.change(cx, |s| {
                            s.add_bus(&format!("Bus {}", buses + 1));
                        });
                    }
                })),
            )
    }

    fn strip(&mut self, id: u32, cx: &mut Context<Self>) -> gpui::AnyElement {
        let state = &self.model.read(cx).state;
        let Some(strip) = state.strip(id) else {
            return div().into_any_element();
        };
        let strip = strip.clone();
        let buses: Vec<_> = state
            .buses
            .iter()
            .map(|b| (b.id, b.name.to_uppercase()))
            .collect();
        let virtual_strip = matches!(strip.source, StripSource::Virtual);
        let (source, missing) = match &strip.source {
            StripSource::Virtual => ("VIRTUAL SINK".into(), false),
            StripSource::Device { node_name: None } => ("System default ▾".into(), false),
            StripSource::Device {
                node_name: Some(name),
            } => self
                .snapshot
                .sources
                .iter()
                .find(|d| &d.node_name == name)
                .map(|d| (format!("{} ▾", d.description), false))
                .unwrap_or_else(|| (format!("Missing: {name} ▾"), true)),
        };
        let name = self.name(Target::Strip(id), &strip.name, cx);
        let fader = self.fader(
            Target::Strip(id),
            strip.slot as usize,
            false,
            strip.gain_db,
            cx,
        );
        let hw_name = self.model.read(cx).state.hw.name.to_uppercase();
        let controls_height = control_stack_height(buses.len());
        let hw_slider = self.send_slider(id, None, cx);
        div()
            .id(("strip", id))
            .w(px(164.))
            .min_w(px(164.))
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .rounded_lg()
            .border_1()
            .border_color(rgb(theme::BORDER))
            .bg(rgb(theme::PANEL))
            .child(
                div()
                    .h_8()
                    .flex_none()
                    .flex()
                    .gap_1()
                    .child(div().flex_1().min_w_0().child(name))
                    .child(button(("delete-strip", id), "×", false).px_2().on_click(
                        cx.listener(move |this, _, _, cx| this.change(cx, |s| s.remove_strip(id))),
                    )),
            )
            .child(
                div()
                    .id(("source", id))
                    .h_8()
                    .flex_none()
                    .overflow_hidden()
                    .text_xs()
                    .text_color(rgb(if missing {
                        theme::ORANGE
                    } else {
                        theme::TEXT_DIM
                    }))
                    .when(!virtual_strip, |d| {
                        d.cursor_pointer().on_click(
                            cx.listener(move |this, _, w, cx| this.show(Menu::Source(id), w, cx)),
                        )
                    })
                    .child(source),
            )
            .child(fader)
            .child(
                div()
                    .h(controls_height)
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .h_4()
                            .flex_none()
                            .font_family(theme::FONT_MONO)
                            .text_xs()
                            .text_color(rgb(theme::TEXT_DIM))
                            .child("HW"),
                    )
                    .child(
                        button(("hw-send", id), hw_name, strip.hw).on_click(cx.listener(
                            move |this, _, _, cx| {
                                this.change(cx, |s| {
                                    if let Some(s) = s.strip_mut(id) {
                                        s.hw = !s.hw;
                                    }
                                })
                            },
                        )),
                    )
                    .child(hw_slider)
                    .child(
                        div()
                            .h_4()
                            .flex_none()
                            .mt_2()
                            .font_family(theme::FONT_MONO)
                            .text_xs()
                            .text_color(rgb(theme::TEXT_DIM))
                            .child("BUS"),
                    )
                    .children(buses.into_iter().map(|(bus, name)| {
                        let slider = self.send_slider(id, Some(bus), cx);
                        div()
                            .h(px(56.))
                            .flex_none()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                button(
                                    ("route", id as u64 * 32 + bus as u64),
                                    name,
                                    strip.buses.contains(&bus),
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.change(cx, |s| {
                                            if let Some(s) = s.strip_mut(id) {
                                                if s.buses.contains(&bus) {
                                                    s.buses.retain(|b| *b != bus);
                                                } else {
                                                    s.buses.push(bus);
                                                }
                                            }
                                        })
                                    },
                                )),
                            )
                            .child(slider)
                    }))
                    .child(div().h_2().flex_none())
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                tone_button(("mute", id), "M", strip.mute, theme::RED)
                                    .flex_1()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.change(cx, |s| {
                                            if let Some(s) = s.strip_mut(id) {
                                                s.mute = !s.mute;
                                            }
                                        })
                                    })),
                            )
                            .child(
                                tone_button(("solo", id), "S", strip.solo, theme::YELLOW)
                                    .flex_1()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.change(cx, |s| {
                                            if let Some(s) = s.strip_mut(id) {
                                                s.solo = !s.solo;
                                            }
                                        })
                                    })),
                            ),
                    )
                    .when(virtual_strip, |d| {
                        d.child(
                            tone_button(
                                ("apps", id),
                                if strip.catch_all {
                                    "APPS · ALL"
                                } else {
                                    "APPS"
                                },
                                strip.catch_all,
                                theme::PURPLE,
                            )
                            .on_click(
                                cx.listener(move |this, _, w, cx| this.show(Menu::Apps(id), w, cx)),
                            ),
                        )
                    })
                    .when(!virtual_strip, |d| {
                        d.child(
                            div()
                                .h_8()
                                .flex_none()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_md()
                                .border_1()
                                .border_color(rgb(theme::BORDER))
                                .font_family(theme::FONT_MONO)
                                .text_xs()
                                .text_color(rgb(theme::TEXT_FAINT))
                                .child("HARDWARE"),
                        )
                    }),
            )
            .into_any_element()
    }

    fn bus(&mut self, target: Target, cx: &mut Context<Self>) -> gpui::AnyElement {
        let state = &self.model.read(cx).state;
        let (name, gain, mute, slot) = match target {
            Target::Hardware => (state.hw.name.clone(), state.hw.gain_db, state.hw.mute, 0),
            Target::Bus(id) => {
                let Some(b) = state.bus(id) else {
                    return div().into_any_element();
                };
                (b.name.clone(), b.gain_db, b.mute, b.slot as usize)
            }
            Target::Strip(_) => unreachable!(),
        };
        let hardware = target == Target::Hardware;
        let sink = state
            .hw
            .sink
            .as_ref()
            .map(|name| {
                self.snapshot
                    .sinks
                    .iter()
                    .find(|s| &s.node_name == name)
                    .map(|s| format!("{} ▾", s.description))
                    .unwrap_or_else(|| format!("Missing: {name} ▾"))
            })
            .unwrap_or_else(|| "System default ▾".into());
        let sink_missing = state
            .hw
            .sink
            .as_ref()
            .is_some_and(|name| !self.snapshot.sinks.iter().any(|s| &s.node_name == name));
        let controls_height = control_stack_height(state.buses.len());
        let name = self.name(target, &name, cx);
        let fader = self.fader(target, slot, true, gain, cx);
        div()
            .w(px(150.))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .rounded_lg()
            .border_1()
            .border_color(rgb(if hardware {
                theme::ACCENT
            } else {
                theme::BORDER_STRONG
            }))
            .bg(rgb(if hardware {
                theme::PANEL_RAISED
            } else {
                theme::PANEL
            }))
            .child(div().h_8().flex_none().child(name))
            .child(
                div()
                    .id(("bus-device", slot))
                    .h_8()
                    .flex_none()
                    .text_xs()
                    .font_family(theme::FONT_MONO)
                    .text_color(rgb(if sink_missing && hardware {
                        theme::ORANGE
                    } else {
                        theme::ACCENT
                    }))
                    .when(hardware, |d| {
                        d.cursor_pointer()
                            .on_click(cx.listener(|this, _, w, cx| this.show(Menu::Sink, w, cx)))
                    })
                    .child(if hardware {
                        sink
                    } else {
                        format!("VIRTUAL OUT {slot:02}")
                    }),
            )
            .child(fader)
            .child(
                div()
                    .h(controls_height)
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(div().flex_1())
                    .child(
                        tone_button(("bus-mute", slot), "M", mute, theme::RED).on_click(
                            cx.listener(move |this, _, _, cx| {
                                this.change(cx, |state| match target {
                                    Target::Hardware => state.hw.mute = !state.hw.mute,
                                    Target::Bus(id) => {
                                        if let Some(b) = state.bus_mut(id) {
                                            b.mute = !b.mute;
                                        }
                                    }
                                    Target::Strip(_) => unreachable!(),
                                })
                            }),
                        ),
                    )
                    .when(!hardware, |d| {
                        d.child(button(("delete-bus", slot), "REMOVE", false).on_click(
                            cx.listener(move |this, _, _, cx| {
                                if let Target::Bus(id) = target {
                                    this.change(cx, |s| s.remove_bus(id));
                                }
                            }),
                        ))
                    })
                    .when(hardware, |d| d.child(div().h_8().flex_none())),
            )
            .into_any_element()
    }

    fn add_pattern(&self, id: u32, blocked: bool, pattern: &str, cx: &mut App) {
        self.change(cx, |state| {
            if let Some(strip) = state.strip_mut(id) {
                if blocked {
                    rules::add_pattern(&mut strip.blocked, pattern);
                } else {
                    strip
                        .blocked
                        .retain(|p| !p.0.eq_ignore_ascii_case(pattern.trim()));
                    rules::add_pattern(&mut strip.apps, pattern);
                }
            }
            if !blocked {
                // Assign here takes ownership from an older explicit assignment.
                for strip in &mut state.strips {
                    if strip.id != id {
                        strip
                            .apps
                            .retain(|p| !p.0.eq_ignore_ascii_case(pattern.trim()));
                    }
                }
            }
        });
    }

    fn apps(&mut self, id: u32, cx: &mut Context<Self>) -> gpui::AnyElement {
        let Some(strip) = self.model.read(cx).state.strip(id).cloned() else {
            return div().into_any_element();
        };
        let state = &self.model.read(cx).state;
        let running: Vec<_> = self
            .snapshot
            .apps
            .iter()
            .map(|app| {
                let route = app
                    .assigned
                    .and_then(|id| state.strip(id))
                    .map(|s| s.name.clone())
                    .unwrap_or_else(|| "System default".into());
                (app.info.clone(), route)
            })
            .collect();
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(format!("APP ROUTING / {}", strip.name))
            .child(
                tone_button(
                    "catch-all",
                    "Catch-all · receive unclaimed apps",
                    strip.catch_all,
                    theme::PURPLE,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.change(cx, |state| {
                        let enabled = !state.strip(id).is_some_and(|s| s.catch_all);
                        for s in &mut state.strips {
                            s.catch_all = enabled && s.id == id;
                        }
                    })
                })),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(theme::TEXT_DIM))
                    .child("RUNNING APPS"),
            )
            .when(running.is_empty(), |d| {
                d.child(
                    div()
                        .text_sm()
                        .text_color(rgb(theme::TEXT_DIM))
                        .child("No playback streams are running."),
                )
            })
            .children(running.into_iter().map(|(app, route)| {
                let assign = rules::pattern_for(&app);
                let block = assign.clone();
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .border_b_1()
                    .border_color(rgb(theme::BORDER))
                    .pb_2()
                    .child(
                        div().flex_1().min_w_0().child(app.name).child(
                            div()
                                .text_xs()
                                .text_color(rgb(theme::TEXT_DIM))
                                .child(format!("{} → {route}", app.binary)),
                        ),
                    )
                    .child(
                        button(("assign-app", app.node_id), "Assign here", false).on_click(
                            cx.listener(move |this, _, _, cx| {
                                this.add_pattern(id, false, &assign, cx)
                            }),
                        ),
                    )
                    .child(
                        button(("block-app", app.node_id), "Block here", false).on_click(
                            cx.listener(move |this, _, _, cx| {
                                this.add_pattern(id, true, &block, cx)
                            }),
                        ),
                    )
            }))
            .children(
                [
                    (false, "ASSIGNED", strip.apps),
                    (true, "BLOCKED", strip.blocked),
                ]
                .into_iter()
                .map(|(blocked, heading, patterns)| {
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(theme::TEXT_DIM))
                                .child(heading),
                        )
                        .children(patterns.into_iter().enumerate().map(|(i, p)| {
                            let pattern = p.0.clone();
                            button(
                                (
                                    if blocked {
                                        "blocked-chip"
                                    } else {
                                        "assigned-chip"
                                    },
                                    i,
                                ),
                                format!("{}  ×", p.0),
                                !blocked,
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.change(cx, |state| {
                                        if let Some(s) = state.strip_mut(id) {
                                            let list =
                                                if blocked { &mut s.blocked } else { &mut s.apps };
                                            list.retain(|p| p.0 != pattern);
                                        }
                                    })
                                },
                            ))
                        }))
                }),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(theme::TEXT_DIM))
                    .child("MANUAL PATTERN · binary equals / app name contains"),
            )
            .child(self.pattern.clone())
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        button("assign-pattern", "Assign pattern", false).on_click(cx.listener(
                            move |this, _, _, cx| {
                                let pattern = this.pattern.read(cx).value().to_string();
                                this.add_pattern(id, false, &pattern, cx);
                                this.pattern.update(cx, |p, cx| p.clear(cx));
                            },
                        )),
                    )
                    .child(
                        button("block-pattern", "Block pattern", false).on_click(cx.listener(
                            move |this, _, _, cx| {
                                let pattern = this.pattern.read(cx).value().to_string();
                                this.add_pattern(id, true, &pattern, cx);
                                this.pattern.update(cx, |p, cx| p.clear(cx));
                            },
                        )),
                    ),
            )
            .into_any_element()
    }

    fn popup(&mut self, menu: Menu, cx: &mut Context<Self>) -> gpui::AnyElement {
        let contents =
            match menu {
                Menu::AddStrip => div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child("ADD INPUT")
                    .child(
                        button("new-virtual", "Virtual · application audio", false).on_click(
                            cx.listener(|this, _, _, cx| {
                                this.change(cx, |state| {
                                    state.add_strip("New input", StripSource::Virtual);
                                });
                                this.menu = None;
                                cx.notify();
                            }),
                        ),
                    )
                    .child(
                        button("new-device", "Device · microphone / capture", false).on_click(
                            cx.listener(|this, _, _, cx| {
                                this.change(cx, |state| {
                                    state.add_strip(
                                        "Microphone",
                                        StripSource::Device { node_name: None },
                                    );
                                });
                                this.menu = None;
                                cx.notify();
                            }),
                        ),
                    )
                    .into_any_element(),
                Menu::Source(id) => {
                    let devices = self.snapshot.sources.clone();
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child("CAPTURE SOURCE")
                        .child(button("default-capture", "System default", false).on_click(
                            cx.listener(move |this, _, _, cx| {
                                this.change(cx, |state| {
                                    if let Some(s) = state.strip_mut(id) {
                                        s.source = StripSource::Device { node_name: None };
                                    }
                                });
                                this.menu = None;
                                cx.notify();
                            }),
                        ))
                        .children(devices.into_iter().enumerate().map(|(i, device)| {
                            button(("capture-option", i), device.description, false).on_click(
                                cx.listener(move |this, _, _, cx| {
                                    this.change(cx, |state| {
                                        if let Some(s) = state.strip_mut(id) {
                                            s.source = StripSource::Device {
                                                node_name: Some(device.node_name.clone()),
                                            };
                                        }
                                    });
                                    this.menu = None;
                                    cx.notify();
                                }),
                            )
                        }))
                        .into_any_element()
                }
                Menu::Sink => {
                    let devices = self.snapshot.sinks.clone();
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child("HARDWARE OUTPUT")
                        .child(button("default-sink", "System default", false).on_click(
                            cx.listener(|this, _, _, cx| {
                                this.change(cx, |state| state.hw.sink = None);
                                this.menu = None;
                                cx.notify();
                            }),
                        ))
                        .children(devices.into_iter().enumerate().map(|(i, device)| {
                            button(("sink-option", i), device.description, false).on_click(
                                cx.listener(move |this, _, _, cx| {
                                    this.change(cx, |state| {
                                        state.hw.sink = Some(device.node_name.clone())
                                    });
                                    this.menu = None;
                                    cx.notify();
                                }),
                            )
                        }))
                        .into_any_element()
                }
                Menu::Apps(id) => self.apps(id, cx),
                Menu::Presets => self.presets.clone().into_any_element(),
            };
        div()
            .id("mixer-modal")
            .occlude()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x00000099))
            .child(
                div()
                    .id("mixer-popover")
                    .occlude()
                    .w(px(if matches!(menu, Menu::Apps(_) | Menu::Presets) {
                        620.
                    } else {
                        420.
                    }))
                    .max_h(px(600.))
                    .overflow_y_scroll()
                    .p_5()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(theme::ACCENT_DIM))
                    .bg(rgb(theme::PANEL_RAISED))
                    .shadow_lg()
                    .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                        this.menu = None;
                        cx.notify();
                    }))
                    .child(div().flex().justify_end().mb_3().child(
                        button("close-popup", "Close · Esc", false).on_click(cx.listener(
                            |this, _, _, cx| {
                                this.menu = None;
                                cx.notify();
                            },
                        )),
                    ))
                    .child(contents),
            )
            .into_any_element()
    }
}

impl Render for MixerView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = &self.model.read(cx).state;
        let strip_ids: Vec<_> = state.strips.iter().map(|s| s.id).collect();
        let bus_ids: Vec<_> = state.buses.iter().map(|b| b.id).collect();
        let minimum_height = px(576.) + control_stack_height(state.buses.len());
        let bus_section_width = bus_section_width(state.buses.len(), window.viewport_size().width);
        self.names.retain(|key, _| match key {
            Target::Strip(id) => strip_ids.contains(id),
            Target::Bus(id) => bus_ids.contains(id),
            Target::Hardware => true,
        });
        self.faders.retain(|key, _| match key {
            Target::Strip(id) => strip_ids.contains(id),
            Target::Bus(id) => bus_ids.contains(id),
            Target::Hardware => true,
        });
        self.send_sliders.retain(|(strip, bus), _| {
            strip_ids.contains(strip) && bus.is_none_or(|id| bus_ids.contains(&id))
        });
        let strips: Vec<_> = strip_ids.into_iter().map(|id| self.strip(id, cx)).collect();
        let hw = self.bus(Target::Hardware, cx);
        let buses: Vec<_> = bus_ids
            .into_iter()
            .map(|id| self.bus(Target::Bus(id), cx))
            .collect();
        let popup = self.menu.map(|menu| self.popup(menu, cx));
        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, ev: &gpui::KeyDownEvent, _, cx| {
                if ev.keystroke.key == "escape" {
                    this.menu = None;
                    cx.notify();
                }
            }))
            .child(self.header(cx))
            .child(
                div()
                    .id("mixer-vertical-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px_4()
                    .pb_4()
                    .child(
                        div()
                            .flex()
                            .h_full()
                            .gap_4()
                            .min_h(minimum_height)
                            .child(
                                div()
                                    .id("inputs-scroll")
                                    .h_full()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_x_scroll()
                                    .flex()
                                    .gap_3()
                                    .children(strips),
                            )
                            .child(
                                div()
                                    .id("buses-scroll")
                                    .h_full()
                                    .flex_none()
                                    .w(bus_section_width)
                                    .overflow_x_scroll()
                                    .flex()
                                    .gap_3()
                                    .pl_4()
                                    .border_l_1()
                                    .border_color(rgb(theme::BORDER_STRONG))
                                    .child(hw)
                                    .children(buses),
                            ),
                    ),
            )
            .children(popup)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buses_use_their_natural_width_when_there_is_room() {
        assert_eq!(bus_section_width(4, px(2552.)), px(815.));
    }

    #[test]
    fn overflowing_buses_leave_room_for_a_strip_and_window_gutters() {
        assert_eq!(bus_section_width(MAX_BUSES, px(860.)), px(648.));
    }

    #[test]
    fn every_bus_adds_a_button_and_slider_row_to_the_shared_control_stack() {
        for count in 0..MAX_BUSES {
            assert_eq!(
                control_stack_height(count + 1) - control_stack_height(count),
                px(64.)
            );
        }
    }
}
