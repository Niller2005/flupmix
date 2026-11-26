//! Interactive PipeWire graph; geometry is in canvas-local logical pixels.

use std::{
    cell::Cell,
    cmp::Ordering,
    collections::{HashMap, HashSet},
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use flupmix_core::{
    shared::{Command, GraphNode, GraphPort, PortDir, Snapshot},
    EngineHandle,
};
use gpui::{
    canvas, div, point, prelude::*, px, rgb, Bounds, Context, FocusHandle, IntoElement,
    KeyDownEvent, MouseButton, PathBuilder, Pixels, Point, Render, Window,
};

use crate::theme;

const CARD_WIDTH: f32 = 260.0;
const HEADER_HEIGHT: f32 = 56.0;
const PORT_HEIGHT: f32 = 24.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Position {
    x: f32,
    y: f32,
}

impl Position {
    fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    fn plus(self, other: Self) -> Self {
        Self::new(self.x + other.x, self.y + other.y)
    }

    fn minus(self, other: Self) -> Self {
        Self::new(self.x - other.x, self.y - other.y)
    }

    fn pixels(self) -> Point<Pixels> {
        point(px(self.x), px(self.y))
    }
}

fn column(media_class: &str, ours: bool) -> usize {
    if ours {
        if media_class.starts_with("Audio/Sink") {
            1
        } else if media_class == "Audio/Source/Virtual" {
            3
        } else {
            2
        }
    } else if media_class.starts_with("Audio/Source")
        || media_class.starts_with("Stream/Output/Audio")
    {
        0
    } else if media_class.starts_with("Audio/Sink") || media_class.starts_with("Stream/Input/Audio")
    {
        4
    } else {
        2
    }
}

fn curve(start: Position, end: Position) -> [Position; 4] {
    let reach = ((end.x - start.x).abs() * 0.5).max(60.0);
    [
        start,
        Position::new(start.x + reach, start.y),
        Position::new(end.x - reach, end.y),
        end,
    ]
}

fn sample_bezier(points: [Position; 4], t: f32) -> Position {
    let u = 1.0 - t;
    let weights = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
    points
        .iter()
        .zip(weights)
        .fold(Position::default(), |sum, (p, w)| {
            sum.plus(Position::new(p.x * w, p.y * w))
        })
}

fn segment_distance(p: Position, a: Position, b: Position) -> f32 {
    let d = b.minus(a);
    let length_squared = d.x * d.x + d.y * d.y;
    let t = if length_squared > 0.0 {
        (((p.x - a.x) * d.x + (p.y - a.y) * d.y) / length_squared).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p.x - a.x - t * d.x).hypot(p.y - a.y - t * d.y)
}

fn curve_distance(p: Position, points: [Position; 4]) -> f32 {
    // Scale the subdivision count with the control polygon so long links retain
    // the same hit-test tolerance as short links.
    let length: f32 = points
        .windows(2)
        .map(|pair| (pair[1].x - pair[0].x).hypot(pair[1].y - pair[0].y))
        .sum();
    let steps = ((length / 8.0).ceil() as usize).clamp(32, 512);
    let mut distance = f32::INFINITY;
    let mut previous = points[0];
    for step in 1..=steps {
        let next = sample_bezier(points, step as f32 / steps as f32);
        distance = distance.min(segment_distance(p, previous, next));
        previous = next;
    }
    distance
}

#[derive(Clone, Copy)]
struct PortHit {
    id: u32,
    dir: PortDir,
    position: Position,
}

struct Wire {
    id: u32,
    points: [Position; 4],
    ours: bool,
}

fn natural_name_cmp(mut left: &str, mut right: &str) -> Ordering {
    while !left.is_empty() && !right.is_empty() {
        let a = left.as_bytes()[0];
        let b = right.as_bytes()[0];
        if a.is_ascii_digit() && b.is_ascii_digit() {
            let a_end = left.bytes().take_while(u8::is_ascii_digit).count();
            let b_end = right.bytes().take_while(u8::is_ascii_digit).count();
            let a_number = left[..a_end].trim_start_matches('0');
            let b_number = right[..b_end].trim_start_matches('0');
            let order = a_number
                .len()
                .cmp(&b_number.len())
                .then_with(|| a_number.cmp(b_number));
            if order != Ordering::Equal {
                return order;
            }
            left = &left[a_end..];
            right = &right[b_end..];
        } else {
            let order = a.cmp(&b);
            if order != Ordering::Equal {
                return order;
            }
            // Compare nonnumeric runs bytewise, preserving UTF-8 boundaries.
            let a_char = left.chars().next().expect("nonempty name");
            let b_char = right.chars().next().expect("nonempty name");
            let order = a_char.cmp(&b_char);
            if order != Ordering::Equal {
                return order;
            }
            left = &left[a_char.len_utf8()..];
            right = &right[b_char.len_utf8()..];
        }
    }
    left.len().cmp(&right.len())
}

enum Gesture {
    Pan {
        pointer: Position,
        origin: Position,
    },
    Node {
        name: String,
        pointer: Position,
        origin: Position,
    },
    Link {
        port: PortHit,
        pointer: Position,
    },
}

/// Session-local graph layout and interaction state.
pub struct PatchbayView {
    engine: EngineHandle,
    snapshot: Arc<Snapshot>,
    positions: HashMap<String, Position>,
    pan: Position,
    scoped: bool,
    query: String,
    query_cursor: usize,
    search_focus: FocusHandle,
    canvas_focus: FocusHandle,
    canvas_bounds: Rc<Cell<Bounds<Pixels>>>,
    selected: Option<u32>,
    gesture: Option<Gesture>,
    target: Option<u32>,
    ports: Vec<PortHit>,
    wires: Vec<Wire>,
    visible: HashSet<u32>,
    port_rows: HashMap<u32, Vec<usize>>,
}

impl PatchbayView {
    pub fn new(engine: EngineHandle, cx: &mut Context<Self>) -> Self {
        let snapshot = engine.shared.snapshot();
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(100))
                .await;
            if this
                .update(cx, |this, cx| {
                    let snapshot = this.engine.shared.snapshot();
                    if !Arc::ptr_eq(&snapshot, &this.snapshot) {
                        this.snapshot = snapshot;
                        this.rebuild_port_rows();
                        this.layout_new_nodes();
                        if !this
                            .snapshot
                            .graph
                            .links
                            .iter()
                            .any(|link| Some(link.id) == this.selected)
                        {
                            this.selected = None;
                        }
                        let missing_drag_port = match &this.gesture {
                            Some(Gesture::Link { port, .. }) => !this
                                .snapshot
                                .graph
                                .ports
                                .iter()
                                .any(|p| p.id == port.id && p.dir == port.dir),
                            _ => false,
                        };
                        if missing_drag_port {
                            this.gesture = None;
                            this.target = None;
                        }
                        this.rebuild_geometry();
                        cx.notify();
                    }
                })
                .is_err()
            {
                break;
            }
        })
        .detach();
        let mut view = Self {
            engine,
            snapshot,
            positions: HashMap::new(),
            pan: Position::default(),
            scoped: true,
            query: String::new(),
            query_cursor: 0,
            search_focus: cx.focus_handle().tab_stop(true),
            canvas_focus: cx.focus_handle().tab_stop(true),
            canvas_bounds: Rc::new(Cell::new(Bounds::default())),
            selected: None,
            gesture: None,
            target: None,
            ports: Vec::new(),
            wires: Vec::new(),
            visible: HashSet::new(),
            port_rows: HashMap::new(),
        };
        view.rebuild_port_rows();
        view.layout_new_nodes();
        view.rebuild_geometry();
        view
    }

    fn rebuild_port_rows(&mut self) {
        let graph = &self.snapshot.graph;
        let mixer = graph
            .nodes
            .iter()
            .find(|node| node.name == "flupmix.mixer")
            .map(|node| node.id);
        let linked: HashSet<_> = graph
            .links
            .iter()
            .flat_map(|link| [link.out_port, link.in_port])
            .collect();
        self.port_rows.clear();
        for (index, port) in graph.ports.iter().enumerate() {
            if self.scoped && Some(port.node_id) == mixer && !linked.contains(&port.id) {
                continue;
            }
            self.port_rows.entry(port.node_id).or_default().push(index);
        }
        if let Some(rows) = mixer.and_then(|id| self.port_rows.get_mut(&id)) {
            rows.sort_by(|a, b| natural_name_cmp(&graph.ports[*a].name, &graph.ports[*b].name));
        }
    }

    fn node_ports(&self, id: u32) -> impl Iterator<Item = &GraphPort> {
        self.port_rows
            .get(&id)
            .into_iter()
            .flatten()
            .map(|index| &self.snapshot.graph.ports[*index])
    }

    fn node_height(&self, id: u32) -> f32 {
        let mut counts = [0usize; 2];
        for port in self.node_ports(id) {
            counts[usize::from(port.dir == PortDir::Out)] += 1;
        }
        HEADER_HEIGHT + counts[0].max(counts[1]).max(1) as f32 * PORT_HEIGHT + 8.0
    }

    fn layout_new_nodes(&mut self) {
        let mut bottoms = [24.0f32; 5];
        for node in &self.snapshot.graph.nodes {
            if let Some(position) = self.positions.get(&node.name) {
                let col = column(&node.media_class, node.ours);
                bottoms[col] = bottoms[col].max(position.y + self.node_height(node.id) + 32.0);
            }
        }
        let mut nodes: Vec<_> = self
            .snapshot
            .graph
            .nodes
            .iter()
            .filter(|node| {
                (node.ours || node.media_class.contains("Audio"))
                    && !self.positions.contains_key(&node.name)
            })
            .collect();
        nodes.sort_by(|a, b| natural_name_cmp(&a.name, &b.name).then_with(|| a.id.cmp(&b.id)));
        for node in &nodes {
            let col = column(&node.media_class, node.ours);
            self.positions.insert(
                node.name.clone(),
                Position::new(24.0 + col as f32 * 370.0, bottoms[col]),
            );
            bottoms[col] += self.node_height(node.id) + 32.0;
        }
        if let Some(mixer) = nodes.iter().find(|node| node.name == "flupmix.mixer") {
            let strips = self
                .snapshot
                .graph
                .nodes
                .iter()
                .filter(|node| column(&node.media_class, node.ours) == 1);
            let mut top = f32::INFINITY;
            let mut bottom = f32::NEG_INFINITY;
            for node in strips {
                if let Some(position) = self.positions.get(&node.name) {
                    top = top.min(position.y);
                    bottom = bottom.max(position.y + self.node_height(node.id));
                }
            }
            if top.is_finite() {
                let y = ((top + bottom - self.node_height(mixer.id)) * 0.5).max(24.0);
                if let Some(position) = self.positions.get_mut(&mixer.name) {
                    position.y = y;
                }
            }
        }
    }

    fn rebuild_geometry(&mut self) {
        let graph = &self.snapshot.graph;
        let mut scope: HashSet<_> = graph
            .nodes
            .iter()
            .filter(|node| node.ours)
            .map(|node| node.id)
            .collect();
        if self.scoped {
            let owners: HashMap<_, _> = graph.ports.iter().map(|p| (p.id, p.node_id)).collect();
            let owned = scope.clone();
            for link in &graph.links {
                if let (Some(out), Some(input)) =
                    (owners.get(&link.out_port), owners.get(&link.in_port))
                {
                    if owned.contains(out) || owned.contains(input) {
                        scope.insert(*out);
                        scope.insert(*input);
                    }
                }
            }
        }
        let query = self.query.to_lowercase();
        self.visible = graph
            .nodes
            .iter()
            .filter(|node| {
                (node.ours || node.media_class.contains("Audio"))
                    && (!self.scoped || scope.contains(&node.id))
                    && (query.is_empty()
                        || node.name.to_lowercase().contains(&query)
                        || node.description.to_lowercase().contains(&query))
            })
            .map(|node| node.id)
            .collect();
        self.ports.clear();
        for node in graph
            .nodes
            .iter()
            .filter(|node| self.visible.contains(&node.id))
        {
            let Some(position) = self.positions.get(&node.name) else {
                continue;
            };
            let mut rows = [0usize; 2];
            for port in self
                .port_rows
                .get(&node.id)
                .into_iter()
                .flatten()
                .map(|index| &graph.ports[*index])
            {
                let side = usize::from(port.dir == PortDir::Out);
                self.ports.push(PortHit {
                    id: port.id,
                    dir: port.dir,
                    position: position.plus(self.pan).plus(Position::new(
                        if side == 0 { 8.0 } else { CARD_WIDTH - 8.0 },
                        HEADER_HEIGHT + rows[side] as f32 * PORT_HEIGHT + PORT_HEIGHT / 2.0,
                    )),
                });
                rows[side] += 1;
            }
        }
        let ports: HashMap<_, _> = self
            .ports
            .iter()
            .map(|port| (port.id, port.position))
            .collect();
        self.wires = graph
            .links
            .iter()
            .filter_map(|link| {
                Some(Wire {
                    id: link.id,
                    points: curve(*ports.get(&link.out_port)?, *ports.get(&link.in_port)?),
                    ours: link.ours,
                })
            })
            .collect();
    }

    fn local_pointer(&self, pointer: Point<Pixels>) -> Position {
        let origin = self.canvas_bounds.get().origin;
        Position::new(
            f32::from(pointer.x - origin.x),
            f32::from(pointer.y - origin.y),
        )
    }

    fn port_at(&self, position: Position) -> Option<PortHit> {
        self.ports
            .iter()
            .rev()
            .find(|port| (port.position.x - position.x).hypot(port.position.y - position.y) <= 10.0)
            .copied()
    }

    fn link_at(&self, position: Position) -> Option<u32> {
        self.wires
            .iter()
            .filter_map(|wire| {
                let distance = curve_distance(position, wire.points);
                (distance <= 6.0).then_some((wire.id, distance))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(id, _)| id)
    }

    fn pointer_down(
        &mut self,
        pointer: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.canvas_focus, cx);
        let pointer = self.local_pointer(pointer);
        self.target = None;
        if let Some(port) = self.port_at(pointer) {
            self.gesture = Some(Gesture::Link { port, pointer });
        } else if let Some(node) = self.snapshot.graph.nodes.iter().rev().find(|node| {
            if !self.visible.contains(&node.id) {
                return false;
            }
            let Some(origin) = self.positions.get(&node.name) else {
                return false;
            };
            let origin = origin.plus(self.pan);
            pointer.x >= origin.x
                && pointer.x <= origin.x + CARD_WIDTH
                && pointer.y >= origin.y
                && pointer.y <= origin.y + self.node_height(node.id)
        }) {
            let origin = self.positions[&node.name];
            if pointer.y <= origin.y + self.pan.y + HEADER_HEIGHT {
                self.gesture = Some(Gesture::Node {
                    name: node.name.clone(),
                    pointer,
                    origin,
                });
            }
        } else {
            self.selected = self.link_at(pointer);
            self.gesture = self.selected.is_none().then_some(Gesture::Pan {
                pointer,
                origin: self.pan,
            });
        }
        cx.notify();
    }

    fn pointer_move(&mut self, pointer: Point<Pixels>, cx: &mut Context<Self>) {
        let pointer = self.local_pointer(pointer);
        match &mut self.gesture {
            Some(Gesture::Pan {
                pointer: start,
                origin,
            }) => self.pan = origin.plus(pointer.minus(*start)),
            Some(Gesture::Node {
                name,
                pointer: start,
                origin,
            }) => {
                self.positions
                    .insert(name.clone(), origin.plus(pointer.minus(*start)));
            }
            Some(Gesture::Link { pointer: end, port }) => {
                *end = pointer;
                let dir = port.dir;
                self.target = self
                    .port_at(pointer)
                    .filter(|target| target.dir != dir)
                    .map(|p| p.id);
                cx.notify();
                return;
            }
            None => return,
        }
        self.rebuild_geometry();
        cx.notify();
    }

    fn pointer_up(&mut self, pointer: Point<Pixels>, cx: &mut Context<Self>) {
        if let Some(Gesture::Link { port, .. }) = self.gesture.take() {
            if let Some(target) = self
                .port_at(self.local_pointer(pointer))
                .filter(|target| port.dir != target.dir)
            {
                let (out_port, in_port) = if port.dir == PortDir::Out {
                    (port.id, target.id)
                } else {
                    (target.id, port.id)
                };
                if !self
                    .snapshot
                    .graph
                    .links
                    .iter()
                    .any(|link| link.out_port == out_port && link.in_port == in_port)
                {
                    self.engine.send(Command::Link { out_port, in_port });
                }
            }
        }
        self.target = None;
        cx.notify();
    }

    fn unlink_selected(&mut self, cx: &mut Context<Self>) {
        if let Some(link_id) = self.selected.take() {
            self.engine.send(Command::Unlink { link_id });
            cx.notify();
        }
    }

    fn search_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = &event.keystroke;
        let previous = self.query[..self.query_cursor]
            .char_indices()
            .next_back()
            .map_or(0, |(i, _)| i);
        let next = self.query[self.query_cursor..]
            .chars()
            .next()
            .map_or(self.query_cursor, |c| self.query_cursor + c.len_utf8());
        match key.key.as_str() {
            "left" => self.query_cursor = previous,
            "right" => self.query_cursor = next,
            "home" => self.query_cursor = 0,
            "end" => self.query_cursor = self.query.len(),
            "backspace" => {
                self.query.replace_range(previous..self.query_cursor, "");
                self.query_cursor = previous;
            }
            "delete" => {
                self.query.replace_range(self.query_cursor..next, "");
            }
            "escape" => {
                self.query.clear();
                self.query_cursor = 0;
                self.selected = None;
                window.focus(&self.canvas_focus, cx);
            }
            "enter" => window.focus(&self.canvas_focus, cx),
            "v" if key.modifiers.control || key.modifiers.platform => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    let text: String = text.chars().filter(|ch| !ch.is_control()).collect();
                    self.query.insert_str(self.query_cursor, &text);
                    self.query_cursor += text.len();
                }
            }
            _ if !key.modifiers.control && !key.modifiers.platform => {
                if let Some(text) = &key.key_char {
                    let text: String = text.chars().filter(|ch| !ch.is_control()).collect();
                    self.query.insert_str(self.query_cursor, &text);
                    self.query_cursor += text.len();
                } else {
                    return;
                }
            }
            _ => return,
        }
        self.gesture = None;
        self.target = None;
        self.rebuild_geometry();
        cx.stop_propagation();
        cx.notify();
    }

    fn node_card(&self, node: &GraphNode) -> impl IntoElement {
        let position = self.positions[&node.name].plus(self.pan);
        let mut rows = [0usize; 2];
        let ports = self.node_ports(node.id).map(|port| {
            let side = usize::from(port.dir == PortDir::Out);
            let top = HEADER_HEIGHT + rows[side] as f32 * PORT_HEIGHT;
            rows[side] += 1;
            let compatible = self.target == Some(port.id);
            let color = if compatible {
                theme::ACCENT
            } else if port.monitor {
                theme::TEXT_FAINT
            } else {
                theme::TEXT_DIM
            };
            div()
                .absolute()
                .top(px(top))
                .left(px(if side == 0 { 3.0 } else { CARD_WIDTH / 2.0 }))
                .w(px(CARD_WIDTH / 2.0 - 3.0))
                .h(px(PORT_HEIGHT))
                .flex()
                .items_center()
                .gap_1()
                .when(side == 1, |el| el.flex_row_reverse())
                .child(
                    div()
                        .size(px(10.0))
                        .flex_shrink_0()
                        .rounded_full()
                        .bg(rgb(color))
                        .when(compatible, |el| {
                            el.border_2().border_color(rgb(theme::TEXT))
                        }),
                )
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .text_xs()
                        .text_color(rgb(color))
                        .font_family(theme::FONT_MONO)
                        .child(port.name.clone()),
                )
        });
        div()
            .id(("graph-node", node.id as usize))
            .absolute()
            .left(px(position.x))
            .top(px(position.y))
            .w(px(CARD_WIDTH))
            .h(px(self.node_height(node.id)))
            .rounded_md()
            .border_1()
            .border_color(rgb(if node.ours {
                theme::ACCENT
            } else {
                theme::BORDER_STRONG
            }))
            .bg(rgb(theme::PANEL_RAISED))
            .overflow_hidden()
            .child(
                div()
                    .h(px(HEADER_HEIGHT))
                    .px_3()
                    .py_2()
                    .cursor_move()
                    .border_b_1()
                    .border_color(rgb(theme::BORDER))
                    .bg(rgb(if node.ours {
                        theme::ACCENT_BG
                    } else {
                        theme::PANEL
                    }))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(if node.ours {
                                theme::ACCENT
                            } else {
                                theme::TEXT
                            }))
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(if node.description.is_empty() {
                                node.name.clone()
                            } else {
                                node.description.clone()
                            }),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(theme::TEXT_DIM))
                            .child(node.media_class.clone()),
                    ),
            )
            .children(ports)
    }
}

impl Render for PatchbayView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let bounds = self.canvas_bounds.clone();
        let selected = self.selected;
        let mut painted: Vec<_> = self
            .wires
            .iter()
            .map(|wire| {
                (
                    wire.points,
                    if Some(wire.id) == selected {
                        theme::MAGENTA
                    } else if wire.ours {
                        theme::ACCENT
                    } else {
                        theme::TEXT_FAINT
                    },
                    if Some(wire.id) == selected { 3.5 } else { 2.0 },
                )
            })
            .collect();
        if let Some(Gesture::Link { port, pointer }) = self.gesture {
            let target = self
                .target
                .and_then(|id| self.ports.iter().find(|p| p.id == id))
                .map_or(pointer, |p| p.position);
            let (start, end) = if port.dir == PortDir::Out {
                (port.position, target)
            } else {
                (target, port.position)
            };
            painted.push((curve(start, end), theme::ACCENT, 2.5));
        }
        let focused = self.search_focus.is_focused(window);
        let search = if focused {
            format!(
                "{}│{}",
                &self.query[..self.query_cursor],
                &self.query[self.query_cursor..]
            )
        } else if self.query.is_empty() {
            "Search nodes…".to_owned()
        } else {
            self.query.clone()
        };
        let empty = if !self.snapshot.connected {
            Some("Waiting for PipeWire…")
        } else if !self
            .snapshot
            .graph
            .nodes
            .iter()
            .any(|node| node.ours || node.media_class.contains("Audio"))
        {
            Some("No audio nodes")
        } else if self.visible.is_empty() {
            Some("No matching nodes · try All nodes or clear the search")
        } else {
            None
        };

        div().size_full().min_h_0().flex().flex_col().font_family(theme::FONT_UI)
            .text_color(rgb(theme::TEXT)).bg(rgb(theme::BG))
            .child(div().flex().items_center().gap_3().p_3().border_b_1().border_color(rgb(theme::BORDER))
                .child(div().id("patchbay-search").track_focus(&self.search_focus)
                    .w_64().h_8().px_2().flex().items_center().overflow_hidden().rounded_sm().border_1()
                    .border_color(rgb(if focused { theme::ACCENT } else { theme::BORDER_STRONG }))
                    .bg(rgb(theme::PANEL_SUNKEN)).text_sm().text_color(rgb(if focused || !self.query.is_empty() { theme::TEXT } else { theme::TEXT_DIM }))
                    .child(search).on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                        window.focus(&this.search_focus, cx);
                        cx.notify();
                    })).on_key_down(cx.listener(Self::search_key)))
                .child(div().id("patchbay-scope").px_3().py_1().rounded_sm().border_1().cursor_pointer()
                    .border_color(rgb(if self.scoped { theme::ACCENT_DIM } else { theme::BORDER_STRONG }))
                    .text_sm().text_color(rgb(if self.scoped { theme::ACCENT } else { theme::TEXT_DIM }))
                    .child(if self.scoped { "Only flupmix + connected" } else { "All nodes" })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.scoped = !this.scoped;
                        this.rebuild_port_rows();
                        this.gesture = None;
                        this.target = None;
                        this.rebuild_geometry();
                        cx.notify();
                    })))
                .child(div().id("patchbay-layout").px_3().py_1().rounded_sm().border_1().border_color(rgb(theme::BORDER_STRONG))
                    .text_sm().cursor_pointer().hover(|el| el.bg(rgb(theme::PANEL_RAISED))).child("Auto layout")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.positions.clear();
                        this.pan = Position::default();
                        this.gesture = None;
                        this.target = None;
                        this.layout_new_nodes();
                        this.rebuild_geometry();
                        cx.notify();
                    }))))
            .child(div().id("patchbay-canvas").track_focus(&self.canvas_focus)
                .relative().flex_1().min_h_0().w_full().overflow_hidden().bg(rgb(theme::PANEL_SUNKEN))
                .child(canvas(move |area, _, _| { bounds.set(area); }, move |area, _, window, _| {
                    for (points, color, width) in painted {
                        let screen = points.map(|p| p.pixels() + area.origin);
                        let mut path = PathBuilder::stroke(px(width));
                        path.move_to(screen[0]);
                        path.cubic_bezier_to(screen[3], screen[1], screen[2]);
                        if let Ok(path) = path.build() {
                            window.paint_path(path, rgb(color));
                        }
                    }
                }).absolute().size_full())
                .when(empty.is_none(), |el| el.children(self.snapshot.graph.nodes.iter()
                    .filter(|node| self.visible.contains(&node.id)).map(|node| self.node_card(node))))
                .when_some(empty, |el, message| el.child(div().absolute().inset_0().flex().items_center().justify_center()
                    .text_color(rgb(theme::TEXT_DIM)).child(message)))
                .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                    this.pointer_down(event.position, window, cx);
                }))
                .on_mouse_down(MouseButton::Right, cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                    window.focus(&this.canvas_focus, cx);
                    this.selected = this.link_at(this.local_pointer(event.position));
                    this.unlink_selected(cx);
                }))
                .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| this.pointer_move(event.position, cx)))
                .on_mouse_up(MouseButton::Left, cx.listener(|this, event: &gpui::MouseUpEvent, _, cx| this.pointer_up(event.position, cx)))
                .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _, _, cx| {
                    if this.gesture.take().is_some() {
                        this.target = None;
                        cx.notify();
                    }
                }))
                .on_scroll(cx.listener(|this, event: &gpui::ScrollEvent, _, cx| {
                    let delta = event.delta.pixel_delta(px(24.0));
                    let (x, y) = (f32::from(delta.x), f32::from(delta.y));
                    this.pan = this.pan.plus(if event.modifiers.shift { Position::new(y, x) } else { Position::new(x, y) });
                    this.gesture = None;
                    this.target = None;
                    this.rebuild_geometry();
                    cx.stop_propagation();
                    cx.notify();
                }))
                .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                    match event.keystroke.key.as_str() {
                        "delete" | "backspace" => this.unlink_selected(cx),
                        "escape" => {
                            this.selected = None;
                            this.gesture = None;
                            this.target = None;
                            cx.notify();
                        }
                        _ => return,
                    }
                    cx.stop_propagation();
                })))
            .child(div().h_7().px_3().flex().items_center().text_xs().text_color(rgb(theme::TEXT_DIM))
                .child("Drag ports to connect · select a cable and Delete to disconnect · drag background or scroll to pan"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_columns_follow_signal_flow() {
        assert_eq!(column("Audio/Source", false), 0);
        assert_eq!(column("Stream/Output/Audio", false), 0);
        assert_eq!(column("Audio/Sink", false), 4);
        assert_eq!(column("Stream/Input/Audio", false), 4);
        assert_eq!(column("Audio/Source/Virtual", false), 0);
        assert_eq!(column("Audio/Sink/Virtual", false), 4);
        assert_eq!(column("Audio/Sink", true), 1);
        assert_eq!(column("Audio/Sink/Virtual", true), 1);
        assert_eq!(column("Audio/Source/Virtual", true), 3);
        assert_eq!(column("Audio/Filter", false), 2);
        assert_eq!(column("Audio/Filter", true), 2);
        assert_eq!(column("", true), 2);
    }

    #[test]
    fn mixer_port_names_sort_numerically() {
        let mut names = [
            "out_8_FR", "in_10_FL", "in_2_FR", "out_0_FL", "in_2_FL", "in_1_FL",
        ];
        names.sort_by(|a, b| natural_name_cmp(a, b));
        assert_eq!(
            names,
            ["in_1_FL", "in_2_FL", "in_2_FR", "in_10_FL", "out_0_FL", "out_8_FR"]
        );
        assert_eq!(
            natural_name_cmp("in_99999999999999999999_FL", "in_10_FL"),
            Ordering::Greater
        );
    }

    #[test]
    fn bezier_sampling_preserves_endpoints_and_midpoint() {
        let points = curve(Position::new(0.0, 0.0), Position::new(300.0, 200.0));
        assert_eq!(sample_bezier(points, 0.0), points[0]);
        assert_eq!(sample_bezier(points, 1.0), points[3]);
        assert_eq!(sample_bezier(points, 0.5), Position::new(150.0, 100.0));
    }

    #[test]
    fn curve_hit_test_distinguishes_near_and_far_points() {
        let points = curve(Position::new(0.0, 0.0), Position::new(300.0, 200.0));
        assert!(curve_distance(Position::new(150.0, 103.0), points) < 6.0);
        assert!(curve_distance(Position::new(150.0, 140.0), points) > 6.0);
        assert!(curve_distance(points[0], points) < 0.001);
        assert!(curve_distance(points[3], points) < 0.001);
    }

    #[test]
    fn reversed_and_coincident_links_have_finite_distances() {
        let points = curve(Position::new(600.0, 400.0), Position::new(20.0, 10.0));
        assert!(curve_distance(sample_bezier(points, 0.25), points) < 1.0);
        let same = Position::new(10.0, 10.0);
        assert_eq!(segment_distance(Position::new(13.0, 14.0), same, same), 5.0);
        assert!(curve_distance(same, curve(same, same)) < 0.001);
    }
}
