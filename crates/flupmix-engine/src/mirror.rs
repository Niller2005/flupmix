//! A plain-data mirror of the PipeWire registry (nodes, ports, links) and the
//! pure planning logic on top of it. No PipeWire calls live here, so all of it
//! is unit-testable.

use std::collections::BTreeMap;

use flupmix_core::model::{bus_node_name, strip_node_name};
use flupmix_core::rules::{resolve_target, AppInfo};
use flupmix_core::shared::{
    AppStream, Device, Graph, GraphLink, GraphNode, GraphPort, PortDir, Snapshot,
};
use flupmix_core::{MixerState, StripSource};

pub const MIXER_NODE: &str = "flupmix.mixer";
const OUR_PREFIX: &str = "flupmix.";

#[derive(Debug, Clone)]
pub struct Node {
    pub id: u32,
    pub name: String,
    pub description: String,
    pub media_class: String,
    pub app_name: String,
    pub binary: String,
}

impl Node {
    pub fn ours(&self) -> bool {
        self.name.starts_with(OUR_PREFIX)
    }

    fn label(&self) -> String {
        if self.description.is_empty() {
            self.name.clone()
        } else {
            self.description.clone()
        }
    }
}

#[derive(Debug, Clone)]
pub struct Port {
    pub id: u32,
    pub node_id: u32,
    pub name: String,
    pub dir: PortDir,
    pub monitor: bool,
    pub channel: String,
}

#[derive(Debug, Clone)]
pub struct Link {
    pub id: u32,
    pub out_port: u32,
    pub in_port: u32,
    pub out_node: u32,
    pub in_node: u32,
}

#[derive(Debug, Default)]
pub struct Mirror {
    pub nodes: BTreeMap<u32, Node>,
    pub ports: BTreeMap<u32, Port>,
    pub links: BTreeMap<u32, Link>,
}

pub fn mixer_in_port(slot: usize, ch: usize) -> String {
    format!("in_{slot}_{}", if ch == 0 { "FL" } else { "FR" })
}

pub fn mixer_out_port(dest: usize, ch: usize) -> String {
    format!("out_{dest}_{}", if ch == 0 { "FL" } else { "FR" })
}

fn channel_rank(channel: &str) -> u8 {
    match channel {
        "FL" | "MONO" => 0,
        "FR" => 1,
        _ => 2,
    }
}

impl Mirror {
    pub fn node_by_name(&self, name: &str) -> Option<&Node> {
        self.nodes.values().find(|n| n.name == name)
    }

    /// Ports of a node in channel order (left first).
    pub fn node_ports(&self, node_id: u32, dir: PortDir, monitor: bool) -> Vec<&Port> {
        let mut ports: Vec<&Port> = self
            .ports
            .values()
            .filter(|p| p.node_id == node_id && p.dir == dir && p.monitor == monitor)
            .collect();
        ports.sort_by(|a, b| {
            (channel_rank(&a.channel), &a.name).cmp(&(channel_rank(&b.channel), &b.name))
        });
        ports
    }

    fn mixer_port(&self, mixer: u32, dir: PortDir, name: &str) -> Option<u32> {
        self.ports
            .values()
            .find(|p| p.node_id == mixer && p.dir == dir && p.name == name)
            .map(|p| p.id)
    }

    pub fn has_link(&self, out_port: u32, in_port: u32) -> bool {
        self.links
            .values()
            .any(|l| l.out_port == out_port && l.in_port == in_port)
    }

    pub fn snapshot(&self, state: &MixerState, connected: bool) -> Snapshot {
        let mut sources = Vec::new();
        let mut sinks = Vec::new();
        let mut apps = Vec::new();
        for n in self.nodes.values().filter(|n| !n.ours()) {
            match n.media_class.as_str() {
                "Audio/Source" | "Audio/Source/Virtual" => sources.push(Device {
                    node_name: n.name.clone(),
                    description: n.label(),
                }),
                "Audio/Sink" => sinks.push(Device {
                    node_name: n.name.clone(),
                    description: n.label(),
                }),
                "Stream/Output/Audio" => {
                    let info = app_info(n);
                    let assigned = resolve_target(state, &info);
                    apps.push(AppStream { info, assigned });
                }
                _ => {}
            }
        }
        sources.sort_by(|a, b| a.description.cmp(&b.description));
        sinks.sort_by(|a, b| a.description.cmp(&b.description));
        apps.sort_by_key(|a| a.info.name.to_lowercase());
        let graph = Graph {
            nodes: self
                .nodes
                .values()
                .map(|n| GraphNode {
                    id: n.id,
                    name: n.name.clone(),
                    description: n.label(),
                    media_class: n.media_class.clone(),
                    ours: n.ours(),
                })
                .collect(),
            ports: self
                .ports
                .values()
                .map(|p| GraphPort {
                    id: p.id,
                    node_id: p.node_id,
                    name: p.name.clone(),
                    dir: p.dir,
                    monitor: p.monitor,
                })
                .collect(),
            links: self
                .links
                .values()
                .map(|l| GraphLink {
                    id: l.id,
                    out_port: l.out_port,
                    in_port: l.in_port,
                    ours: [l.out_node, l.in_node]
                        .iter()
                        .any(|id| self.nodes.get(id).is_some_and(Node::ours)),
                })
                .collect(),
        };
        Snapshot {
            connected,
            sources,
            sinks,
            apps,
            graph,
        }
    }
}

fn app_info(n: &Node) -> AppInfo {
    AppInfo {
        node_id: n.id,
        name: if n.app_name.is_empty() {
            n.name.clone()
        } else {
            n.app_name.clone()
        },
        binary: n.binary.clone(),
    }
}

/// Links the mixer needs, as `(output_port, input_port)` pairs.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct LinkPlan {
    pub links: Vec<(u32, u32)>,
    /// Bit per mixer input slot whose source exposes a single channel.
    pub mono_mask: u32,
}

/// What the mixer graph should look like for `state`. `default_sink` is the
/// system default sink's `node.name`, used while `hw.sink` is `None`;
/// `default_source` likewise serves device strips with no source chosen.
pub fn plan_links(
    state: &MixerState,
    mirror: &Mirror,
    default_sink: Option<&str>,
    default_source: Option<&str>,
) -> LinkPlan {
    let mut plan = LinkPlan::default();
    let Some(mixer) = mirror.node_by_name(MIXER_NODE).map(|n| n.id) else {
        return plan;
    };

    for strip in &state.strips {
        let slot = strip.slot as usize;
        let (name, monitor) = match &strip.source {
            StripSource::Virtual => (Some(strip_node_name(strip.id)), true),
            StripSource::Device { node_name } => {
                let name = node_name.as_deref().or(default_source);
                // A device strip must never capture flupmix's own nodes.
                (
                    name.filter(|n| !n.starts_with(OUR_PREFIX))
                        .map(str::to_string),
                    false,
                )
            }
        };
        let Some(node) = name.as_deref().and_then(|n| mirror.node_by_name(n)) else {
            continue;
        };
        let ports = mirror.node_ports(node.id, PortDir::Out, monitor);
        if ports.len() == 1 {
            plan.mono_mask |= 1 << slot;
        }
        for (ch, port) in ports.iter().take(2).enumerate() {
            if let Some(target) = mirror.mixer_port(mixer, PortDir::In, &mixer_in_port(slot, ch)) {
                plan.links.push((port.id, target));
            }
        }
    }

    let hw_sink = state
        .hw
        .sink
        .as_deref()
        .or(default_sink)
        .filter(|n| !n.starts_with(OUR_PREFIX));
    if let Some(node) = hw_sink.and_then(|n| mirror.node_by_name(n)) {
        link_outputs(mirror, mixer, 0, node.id, &mut plan);
    }
    for bus in &state.buses {
        if let Some(node) = mirror.node_by_name(&bus_node_name(bus.id)) {
            link_outputs(mirror, mixer, bus.slot as usize, node.id, &mut plan);
        }
    }
    plan
}

fn link_outputs(mirror: &Mirror, mixer: u32, dest: usize, sink_node: u32, plan: &mut LinkPlan) {
    for (ch, port) in mirror
        .node_ports(sink_node, PortDir::In, false)
        .iter()
        .take(2)
        .enumerate()
    {
        if let Some(src) = mirror.mixer_port(mixer, PortDir::Out, &mixer_out_port(dest, ch)) {
            plan.links.push((src, port.id));
        }
    }
}

/// Desired `target.object` per application stream node: the strip's sink
/// `node.name`, or `None` to leave the stream on the system default.
pub fn plan_moves(
    state: &MixerState,
    mirror: &Mirror,
    routing_enabled: bool,
) -> Vec<(u32, Option<String>)> {
    mirror
        .nodes
        .values()
        .filter(|n| n.media_class == "Stream/Output/Audio" && !n.ours())
        .map(|n| {
            let target = resolve_target(state, &app_info(n))
                .filter(|_| routing_enabled)
                .map(strip_node_name)
                .filter(|name| mirror.node_by_name(name).is_some());
            (n.id, target)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(m: &mut Mirror, id: u32, name: &str, class: &str) {
        m.nodes.insert(
            id,
            Node {
                id,
                name: name.into(),
                description: String::new(),
                media_class: class.into(),
                app_name: String::new(),
                binary: String::new(),
            },
        );
    }

    fn port(
        m: &mut Mirror,
        id: u32,
        node_id: u32,
        name: &str,
        dir: PortDir,
        monitor: bool,
        channel: &str,
    ) {
        m.ports.insert(
            id,
            Port {
                id,
                node_id,
                name: name.into(),
                dir,
                monitor,
                channel: channel.into(),
            },
        );
    }

    /// Mixer (node 1) with all ports, one virtual strip sink, one hw sink.
    fn fixture() -> (MixerState, Mirror) {
        let mut state = MixerState::empty();
        let strip = state.add_strip("Game", StripSource::Virtual).unwrap();
        state.hw.sink = Some("hw".into());
        let mut m = Mirror::default();
        node(&mut m, 1, MIXER_NODE, "");
        let mut id = 100;
        for slot in 0..16 {
            for ch in 0..2 {
                port(
                    &mut m,
                    id,
                    1,
                    &mixer_in_port(slot, ch),
                    PortDir::In,
                    false,
                    "",
                );
                id += 1;
            }
        }
        for dest in 0..9 {
            for ch in 0..2 {
                port(
                    &mut m,
                    id,
                    1,
                    &mixer_out_port(dest, ch),
                    PortDir::Out,
                    false,
                    "",
                );
                id += 1;
            }
        }
        node(&mut m, 2, &strip_node_name(strip), "Audio/Sink");
        port(&mut m, 10, 2, "monitor_FR", PortDir::Out, true, "FR");
        port(&mut m, 11, 2, "monitor_FL", PortDir::Out, true, "FL");
        port(&mut m, 12, 2, "playback_FL", PortDir::In, false, "FL");
        node(&mut m, 3, "hw", "Audio/Sink");
        port(&mut m, 20, 3, "playback_FR", PortDir::In, false, "FR");
        port(&mut m, 21, 3, "playback_FL", PortDir::In, false, "FL");
        (state, m)
    }

    fn port_id(m: &Mirror, node: u32, name: &str) -> u32 {
        m.ports
            .values()
            .find(|p| p.node_id == node && p.name == name)
            .unwrap()
            .id
    }

    #[test]
    fn plans_strip_and_hw_links_in_channel_order() {
        let (state, m) = fixture();
        let plan = plan_links(&state, &m, None, None);
        let mixer_in = |ch| port_id(&m, 1, &mixer_in_port(0, ch));
        let mixer_out = |ch| port_id(&m, 1, &mixer_out_port(0, ch));
        assert!(
            plan.links.contains(&(11, mixer_in(0))),
            "FL monitor to in_0_FL"
        );
        assert!(plan.links.contains(&(10, mixer_in(1))));
        assert!(plan.links.contains(&(mixer_out(0), 21)));
        assert!(plan.links.contains(&(mixer_out(1), 20)));
        assert_eq!(plan.links.len(), 4);
        assert_eq!(plan.mono_mask, 0);
    }

    #[test]
    fn mono_sources_set_the_mask_and_link_left_only() {
        let (mut state, mut m) = fixture();
        let mic = state
            .add_strip(
                "Mic",
                StripSource::Device {
                    node_name: Some("mic".into()),
                },
            )
            .unwrap();
        let slot = state.strip(mic).unwrap().slot as usize;
        node(&mut m, 4, "mic", "Audio/Source");
        port(&mut m, 30, 4, "capture_MONO", PortDir::Out, false, "MONO");
        let plan = plan_links(&state, &m, None, None);
        assert_eq!(plan.mono_mask, 1 << slot);
        assert!(plan
            .links
            .contains(&(30, port_id(&m, 1, &mixer_in_port(slot, 0)))));
        assert_eq!(plan.links.iter().filter(|(o, _)| *o == 30).count(), 1);
    }

    #[test]
    fn device_strip_without_a_choice_follows_the_default_source() {
        let (mut state, mut m) = fixture();
        let mic = state
            .add_strip("Mic", StripSource::Device { node_name: None })
            .unwrap();
        let slot = state.strip(mic).unwrap().slot as usize;
        node(&mut m, 4, "mic", "Audio/Source");
        port(&mut m, 30, 4, "capture_FL", PortDir::Out, false, "FL");
        port(&mut m, 31, 4, "capture_FR", PortDir::Out, false, "FR");
        let has_mic_links = |source| {
            let plan = plan_links(&state, &m, None, source);
            plan.links
                .contains(&(30, port_id(&m, 1, &mixer_in_port(slot, 0))))
        };
        assert!(!has_mic_links(None));
        assert!(has_mic_links(Some("mic")));
        assert!(!has_mic_links(Some("flupmix.strip.1")));
    }

    #[test]
    fn follows_default_sink_but_never_a_flupmix_node() {
        let (mut state, m) = fixture();
        state.hw.sink = None;
        assert_eq!(plan_links(&state, &m, Some("hw"), None).links.len(), 4);
        assert_eq!(plan_links(&state, &m, None, None).links.len(), 2);
        assert_eq!(
            plan_links(&state, &m, Some("flupmix.strip.1"), None)
                .links
                .len(),
            2
        );
    }

    #[test]
    fn missing_devices_and_mixer_yield_no_links() {
        let (mut state, m) = fixture();
        state.hw.sink = Some("gone".into());
        assert_eq!(plan_links(&state, &m, None, None).links.len(), 2);
        assert!(plan_links(&state, &Mirror::default(), None, None)
            .links
            .is_empty());
    }

    #[test]
    fn virtual_bus_links_to_its_source_node() {
        let (mut state, mut m) = fixture();
        let bus = state.add_bus("Bus 1").unwrap();
        node(&mut m, 5, &bus_node_name(bus), "Audio/Source/Virtual");
        port(&mut m, 40, 5, "input_FL", PortDir::In, false, "FL");
        port(&mut m, 41, 5, "input_FR", PortDir::In, false, "FR");
        let plan = plan_links(&state, &m, None, None);
        assert!(plan
            .links
            .contains(&(port_id(&m, 1, &mixer_out_port(1, 0)), 40)));
        assert!(plan
            .links
            .contains(&(port_id(&m, 1, &mixer_out_port(1, 1)), 41)));
    }

    #[test]
    fn moves_follow_catch_all_only_when_the_sink_exists() {
        let (mut state, mut m) = fixture();
        let strip = state.strips[0].id;
        state.strip_mut(strip).unwrap().catch_all = true;
        node(&mut m, 50, "firefox-stream", "Stream/Output/Audio");
        m.nodes.get_mut(&50).unwrap().app_name = "Firefox".into();
        assert_eq!(
            plan_moves(&state, &m, true),
            vec![(50, Some(strip_node_name(strip)))]
        );
        m.nodes.retain(|id, _| *id != 2);
        assert_eq!(plan_moves(&state, &m, true), vec![(50, None)]);
    }

    #[test]
    fn moves_are_all_none_when_routing_is_off() {
        let (mut state, mut m) = fixture();
        let strip = state.strips[0].id;
        state.strip_mut(strip).unwrap().catch_all = true;
        node(&mut m, 50, "s", "Stream/Output/Audio");
        assert_eq!(plan_moves(&state, &m, false), vec![(50, None)]);
    }

    #[test]
    fn snapshot_lists_devices_and_hides_ours() {
        let (state, m) = fixture();
        let snap = m.snapshot(&state, true);
        assert_eq!(snap.sinks.len(), 1);
        assert_eq!(snap.sinks[0].node_name, "hw");
        assert!(snap.graph.nodes.iter().any(|n| n.ours));
        assert!(snap.connected);
    }
}
