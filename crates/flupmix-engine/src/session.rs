//! The engine thread: connects to PipeWire, mirrors the registry, and keeps
//! the audio graph reconciled with the desired [`MixerState`].

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::time::{Duration, Instant};

use flupmix_core::mix::Matrix;
use flupmix_core::model::{bus_node_name, strip_node_name};
use flupmix_core::shared::PortDir;
use flupmix_core::{Command, EngineHandle, MixerState, Shared, Snapshot, StripSource};
use pipewire as pw;
use pw::context::ContextRc;
use pw::core::CoreRc;
use pw::link::Link as LinkProxy;
use pw::main_loop::MainLoopRc;
use pw::metadata::Metadata;
use pw::node::Node as NodeProxy;
use pw::properties::properties;
use pw::registry::{GlobalObject, RegistryRc};
use pw::spa::utils::dict::DictRef;
use pw::types::ObjectType;

use crate::filter::MixerFilter;
use crate::gains::Controls;
use crate::mirror::{plan_links, plan_moves, Link, Mirror, Node, Port};

/// `EPIPE`, the errno PipeWire reports when the daemon connection is gone.
const EPIPE: i32 = 32;
const RECONNECT_TICK: Duration = Duration::from_secs(1);
/// A requested link that never shows up in the registry is retried after this.
const LINK_PENDING_TIMEOUT: Duration = Duration::from_secs(3);

/// The running engine thread. Dropping it stops the thread, returns routed
/// applications to the system default and removes flupmix's nodes.
pub struct RunningEngine {
    handle: EngineHandle,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl RunningEngine {
    pub fn handle(&self) -> EngineHandle {
        self.handle.clone()
    }
}

impl Drop for RunningEngine {
    fn drop(&mut self) {
        self.handle.send(Command::Quit);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Starts the engine thread with `initial` as the desired mixer.
pub fn spawn(initial: MixerState) -> RunningEngine {
    let shared = Arc::new(Shared::default());
    let (tx, rx) = pw::channel::channel::<Command>();
    let handle = EngineHandle::new(shared.clone(), move |cmd| {
        let _ = tx.send(cmd);
    });
    handle.set_state(&initial);
    let thread = std::thread::Builder::new()
        .name("flupmix-engine".into())
        .spawn(move || run(shared, rx))
        .expect("spawn engine thread");
    RunningEngine {
        handle,
        thread: Some(thread),
    }
}

struct VirtualNode {
    proxy: NodeProxy,
    description: String,
}

struct OurLink {
    proxy: LinkProxy,
    created: Instant,
    seen: bool,
}

/// Field order matters: Rust drops fields top to bottom, and PipeWire objects
/// must go before the listeners, registry and core they were created from
/// (`pw_filter_destroy` touches the core).
struct Conn {
    _filter: MixerFilter,
    our_links: HashMap<(u32, u32), OurLink>,
    nodes: HashMap<String, VirtualNode>,
    metadata: Option<(Metadata, pw::metadata::MetadataListener)>,
    _registry_listener: pw::registry::Listener,
    _core_listener: pw::core::Listener,
    registry: RegistryRc,
    core: CoreRc,
    mirror: Mirror,
    /// Application stream node id -> `target.object` we set for it.
    moved: HashMap<u32, String>,
    default_sink: Option<String>,
    default_source: Option<String>,
    sync_seq: Option<pw::spa::utils::result::AsyncSeq>,
    dirty: bool,
    lost: bool,
}

struct Engine {
    shared: Arc<Shared>,
    controls: Arc<Controls>,
    context: ContextRc,
    mainloop: MainLoopRc,
    state: MixerState,
    app_routing: bool,
    conn: Option<Conn>,
    last_published: Option<Snapshot>,
}

type EngineRef = Rc<RefCell<Engine>>;

fn run(shared: Arc<Shared>, commands: pw::channel::Receiver<Command>) {
    pw::init();
    let mainloop = MainLoopRc::new(None).expect("create PipeWire main loop");
    let context = ContextRc::new(&mainloop, None).expect("create PipeWire context");
    let engine: EngineRef = Rc::new(RefCell::new(Engine {
        shared,
        controls: Arc::new(Controls::new()),
        context,
        mainloop: mainloop.clone(),
        state: MixerState::empty(),
        app_routing: true,
        conn: None,
        last_published: None,
    }));

    let _commands = commands.attach(mainloop.loop_(), {
        let engine = engine.clone();
        move |cmd| handle_command(&engine, cmd)
    });

    let timer = mainloop.loop_().add_timer({
        let engine = Rc::downgrade(&engine);
        move |_| {
            if let Some(engine) = engine.upgrade() {
                tick(&engine);
            }
        }
    });
    timer.update_timer(Some(Duration::from_millis(1)), Some(RECONNECT_TICK));

    mainloop.run();

    engine.borrow_mut().disconnect();
}

fn tick(engine: &EngineRef) {
    let mut e = engine.borrow_mut();
    if e.conn.as_ref().is_some_and(|c| c.lost) {
        e.disconnect();
    }
    if e.conn.is_none() {
        e.connect(Rc::downgrade(engine));
    }
}

fn handle_command(engine: &EngineRef, cmd: Command) {
    let mut e = engine.borrow_mut();
    match cmd {
        Command::SetState(state) => {
            e.controls.store(&Matrix::from_state(&state));
            e.state = *state;
            e.request_reconcile();
        }
        Command::SetAppRouting(enabled) => {
            e.app_routing = enabled;
            e.request_reconcile();
        }
        Command::Link { out_port, in_port } => e.link_ports(out_port, in_port),
        Command::Unlink { link_id } => {
            if let Some(conn) = &e.conn {
                conn.registry.destroy_global(link_id);
            }
        }
        Command::Quit => e.mainloop.quit(),
    }
}

fn str_prop(props: Option<&DictRef>, key: &str) -> String {
    props
        .and_then(|p| p.get(key))
        .unwrap_or_default()
        .to_string()
}

fn parse_default_name(value: &str) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(value).ok()?;
    json.get("name")?.as_str().map(str::to_string)
}

impl Engine {
    fn connect(&mut self, weak: Weak<RefCell<Engine>>) {
        let Ok(core) = self.context.connect_rc(None) else {
            return;
        };
        let Ok(registry) = core.get_registry_rc() else {
            return;
        };
        let filter = match MixerFilter::new(&core, self.controls.clone(), self.shared.clone()) {
            Ok(f) => f,
            Err(err) => {
                eprintln!("flupmix: {err}");
                return;
            }
        };

        let core_listener = core
            .add_listener_local()
            .done({
                let weak = weak.clone();
                move |id, seq| {
                    if id != pw::core::PW_ID_CORE {
                        return;
                    }
                    if let Some(engine) = weak.upgrade() {
                        engine.borrow_mut().on_sync_done(seq);
                    }
                }
            })
            .error({
                let weak = weak.clone();
                move |id, _seq, res, message| {
                    if id == pw::core::PW_ID_CORE && res == -EPIPE {
                        eprintln!("flupmix: PipeWire connection lost: {message}");
                        if let Some(engine) = weak.upgrade() {
                            if let Some(conn) = engine.borrow_mut().conn.as_mut() {
                                conn.lost = true;
                            }
                        }
                    }
                }
            })
            .register();

        let registry_listener = registry
            .add_listener_local()
            .global({
                let weak = weak.clone();
                move |global| {
                    if let Some(engine) = weak.upgrade() {
                        engine.borrow_mut().on_global(global, &weak);
                    }
                }
            })
            .global_remove({
                let weak = weak.clone();
                move |id| {
                    if let Some(engine) = weak.upgrade() {
                        engine.borrow_mut().on_global_remove(id);
                    }
                }
            })
            .register();

        self.conn = Some(Conn {
            core,
            registry,
            _core_listener: core_listener,
            _registry_listener: registry_listener,
            _filter: filter,
            mirror: Mirror::default(),
            nodes: HashMap::new(),
            our_links: HashMap::new(),
            moved: HashMap::new(),
            metadata: None,
            default_sink: None,
            default_source: None,
            sync_seq: None,
            dirty: false,
            lost: false,
        });
        self.request_reconcile();
    }

    /// Tears the connection down, giving applications back to the system default.
    fn disconnect(&mut self) {
        if let Some(conn) = self.conn.take() {
            if let Some((meta, _)) = &conn.metadata {
                for node in conn.moved.keys() {
                    meta.set_property(*node, "target.object", None, None);
                }
            }
            // Flush the queued requests before the connection goes away.
            if !conn.lost {
                conn.core.sync(0).ok();
                self.mainloop
                    .loop_()
                    .iterate(pw::loop_::Timeout::Finite(Duration::from_millis(50)));
            }
        }
        self.last_published = None;
        self.shared.publish(Snapshot::default());
    }

    fn request_reconcile(&mut self) {
        let Some(conn) = self.conn.as_mut() else {
            return;
        };
        conn.dirty = true;
        if conn.sync_seq.is_none() {
            conn.sync_seq = conn.core.sync(0).ok();
        }
    }

    fn on_sync_done(&mut self, seq: pw::spa::utils::result::AsyncSeq) {
        let Some(conn) = self.conn.as_mut() else {
            return;
        };
        if conn.sync_seq != Some(seq) {
            return;
        }
        conn.sync_seq = None;
        if conn.dirty {
            self.reconcile();
        }
    }

    fn on_global(&mut self, global: &GlobalObject<&DictRef>, weak: &Weak<RefCell<Engine>>) {
        let Some(conn) = self.conn.as_mut() else {
            return;
        };
        let props = global.props;
        match global.type_ {
            ObjectType::Node => {
                let name = str_prop(props, "node.name");
                let app_name = str_prop(props, "application.name");
                let description = ["node.description", "node.nick"]
                    .iter()
                    .map(|k| str_prop(props, k))
                    .find(|s| !s.is_empty())
                    .unwrap_or_default();
                conn.mirror.nodes.insert(
                    global.id,
                    Node {
                        id: global.id,
                        name,
                        description,
                        media_class: str_prop(props, "media.class"),
                        app_name,
                        binary: str_prop(props, "application.process.binary"),
                    },
                );
            }
            ObjectType::Port => {
                let Some(node_id) = str_prop(props, "node.id").parse().ok() else {
                    return;
                };
                let dir = if str_prop(props, "port.direction") == "in" {
                    PortDir::In
                } else {
                    PortDir::Out
                };
                conn.mirror.ports.insert(
                    global.id,
                    Port {
                        id: global.id,
                        node_id,
                        name: str_prop(props, "port.name"),
                        dir,
                        monitor: str_prop(props, "port.monitor") == "true",
                        channel: str_prop(props, "audio.channel"),
                    },
                );
            }
            ObjectType::Link => {
                let id = |k| str_prop(props, k).parse::<u32>().ok();
                let (Some(out_port), Some(in_port), Some(out_node), Some(in_node)) = (
                    id("link.output.port"),
                    id("link.input.port"),
                    id("link.output.node"),
                    id("link.input.node"),
                ) else {
                    return;
                };
                conn.mirror.links.insert(
                    global.id,
                    Link {
                        id: global.id,
                        out_port,
                        in_port,
                        out_node,
                        in_node,
                    },
                );
            }
            ObjectType::Metadata => {
                if str_prop(props, "metadata.name") != "default" || conn.metadata.is_some() {
                    return;
                }
                let Ok(meta) = conn.registry.bind::<Metadata, _>(global) else {
                    return;
                };
                let weak = weak.clone();
                let listener = meta
                    .add_listener_local()
                    .property(move |subject, key, _type, value| {
                        let Some(engine) = weak.upgrade() else {
                            return 0;
                        };
                        // Try-borrow: the callback can fire while we are setting properties.
                        let Ok(mut e) = engine.try_borrow_mut() else {
                            return 0;
                        };
                        match key {
                            Some("default.audio.sink") => {
                                e.set_default_device(true, value.and_then(parse_default_name))
                            }
                            Some("default.audio.source") => {
                                e.set_default_device(false, value.and_then(parse_default_name))
                            }
                            Some("target.object") => e.adopt_target(subject, value),
                            _ => {}
                        }
                        0
                    })
                    .register();
                conn.metadata = Some((meta, listener));
            }
            _ => return,
        }
        self.request_reconcile();
    }

    fn on_global_remove(&mut self, id: u32) {
        let Some(conn) = self.conn.as_mut() else {
            return;
        };
        let m = &mut conn.mirror;
        let removed = m.nodes.remove(&id).is_some()
            | m.ports.remove(&id).is_some()
            | m.links.remove(&id).is_some();
        if removed {
            self.request_reconcile();
        }
    }

    fn set_default_device(&mut self, sink: bool, name: Option<String>) {
        let Some(conn) = self.conn.as_mut() else {
            return;
        };
        let slot = if sink {
            &mut conn.default_sink
        } else {
            &mut conn.default_source
        };
        if *slot != name {
            *slot = name;
            self.request_reconcile();
        }
    }

    /// Tracks `target.object` entries pointing at our sinks, including ones left
    /// behind by a previous run, so they are cleared when no rule claims the stream.
    fn adopt_target(&mut self, node: u32, value: Option<&str>) {
        let Some(conn) = self.conn.as_mut() else {
            return;
        };
        match value.filter(|v| v.starts_with("flupmix.strip.")) {
            Some(v) => {
                conn.moved.insert(node, v.to_string());
                self.request_reconcile();
            }
            None => {
                conn.moved.remove(&node);
            }
        }
    }

    fn link_ports(&mut self, out_port: u32, in_port: u32) {
        let Some(conn) = self.conn.as_ref() else {
            return;
        };
        let (Some(out), Some(inp)) = (
            conn.mirror.ports.get(&out_port),
            conn.mirror.ports.get(&in_port),
        ) else {
            return;
        };
        if out.dir != PortDir::Out
            || inp.dir != PortDir::In
            || conn.mirror.has_link(out_port, in_port)
        {
            return;
        }
        let props = properties! {
            "link.output.node" => out.node_id.to_string(),
            "link.output.port" => out_port.to_string(),
            "link.input.node" => inp.node_id.to_string(),
            "link.input.port" => in_port.to_string(),
            // User-made links outlive this process and our proxy.
            "object.linger" => "true",
        };
        if let Err(err) = conn.core.create_object::<LinkProxy>("link-factory", &props) {
            eprintln!("flupmix: could not create link: {err}");
        }
    }

    fn reconcile(&mut self) {
        let Some(conn) = self.conn.as_mut() else {
            return;
        };
        conn.dirty = false;
        let state = &self.state;

        ensure_virtual_nodes(conn, state);

        let plan = plan_links(
            state,
            &conn.mirror,
            conn.default_sink.as_deref(),
            conn.default_source.as_deref(),
        );
        self.controls.set_mono_mask(plan.mono_mask);
        conn.our_links.retain(|pair, link| {
            if conn.mirror.has_link(pair.0, pair.1) {
                link.seen = true;
                true
            } else {
                !(link.seen || link.created.elapsed() > LINK_PENDING_TIMEOUT)
            }
        });
        for &(out, inp) in &plan.links {
            if conn.mirror.has_link(out, inp) || conn.our_links.contains_key(&(out, inp)) {
                continue;
            }
            let node = |port: u32| conn.mirror.ports.get(&port).map(|p| p.node_id.to_string());
            let (Some(out_node), Some(in_node)) = (node(out), node(inp)) else {
                continue;
            };
            let props = properties! {
                "link.output.node" => out_node,
                "link.output.port" => out.to_string(),
                "link.input.node" => in_node,
                "link.input.port" => inp.to_string(),
                "object.linger" => "false",
            };
            match conn.core.create_object::<LinkProxy>("link-factory", &props) {
                Ok(proxy) => {
                    conn.our_links.insert(
                        (out, inp),
                        OurLink {
                            proxy,
                            created: Instant::now(),
                            seen: false,
                        },
                    );
                }
                Err(err) => eprintln!("flupmix: could not link {out}->{inp}: {err}"),
            }
        }
        let stale: Vec<(u32, u32)> = conn
            .our_links
            .keys()
            .filter(|pair| !plan.links.contains(pair))
            .copied()
            .collect();
        for pair in stale {
            if let Some(link) = conn.our_links.remove(&pair) {
                let _ = conn.core.destroy_object(link.proxy);
            }
        }

        if let Some((meta, _)) = &conn.metadata {
            for (node, target) in plan_moves(state, &conn.mirror, self.app_routing) {
                match (target, conn.moved.get(&node)) {
                    (Some(t), Some(cur)) if *cur == t => {}
                    (Some(t), _) => {
                        meta.set_property(node, "target.object", Some("Spa:String"), Some(&t));
                        conn.moved.insert(node, t);
                    }
                    (None, Some(_)) => {
                        meta.set_property(node, "target.object", None, None);
                        conn.moved.remove(&node);
                    }
                    (None, None) => {}
                }
            }
            conn.moved
                .retain(|id, _| conn.mirror.nodes.contains_key(id));
        }

        let snapshot = conn.mirror.snapshot(state, true);
        if self.last_published.as_ref() != Some(&snapshot) {
            self.shared.publish(snapshot.clone());
            self.last_published = Some(snapshot);
        }
    }
}

fn ensure_virtual_nodes(conn: &mut Conn, state: &MixerState) {
    // node.name -> (description, is_source)
    let mut wanted: HashMap<String, (String, bool)> = HashMap::new();
    for s in state
        .strips
        .iter()
        .filter(|s| matches!(s.source, StripSource::Virtual))
    {
        wanted.insert(
            strip_node_name(s.id),
            (format!("Flupmix {}", s.name), false),
        );
    }
    for b in &state.buses {
        wanted.insert(bus_node_name(b.id), (format!("Flupmix {}", b.name), true));
    }

    let stale: Vec<String> = conn
        .nodes
        .iter()
        .filter(|(name, node)| {
            wanted
                .get(*name)
                .is_none_or(|(desc, _)| *desc != node.description)
        })
        .map(|(name, _)| name.clone())
        .collect();
    for name in stale {
        if let Some(node) = conn.nodes.remove(&name) {
            let _ = conn.core.destroy_object(node.proxy);
        }
    }

    for (name, (description, is_source)) in wanted {
        if conn.nodes.contains_key(&name) {
            continue;
        }
        let class = if is_source {
            "Audio/Source/Virtual"
        } else {
            "Audio/Sink"
        };
        let props = properties! {
            "factory.name" => "support.null-audio-sink",
            "node.name" => name.as_str(),
            "node.description" => description.as_str(),
            "media.class" => class,
            "audio.position" => "[ FL FR ]",
            "monitor.channel-volumes" => "true",
            // Never let the session manager pick one of these as the default device.
            "priority.session" => "10",
            "priority.driver" => "10",
            "object.linger" => "false",
        };
        match conn.core.create_object::<NodeProxy>("adapter", &props) {
            Ok(proxy) => {
                conn.nodes.insert(name, VirtualNode { proxy, description });
            }
            Err(err) => eprintln!("flupmix: could not create {name}: {err}"),
        }
    }
}
