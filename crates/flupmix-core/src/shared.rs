//! Types shared between the engine thread and the UI.

use std::sync::{Arc, Mutex};

use crate::mix::Meters;
use crate::model::{MixerState, StripId};
use crate::rules::AppInfo;

/// A capture source or playback sink the user can pick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub node_name: String,
    pub description: String,
}

/// A running application stream and the strip the rules route it to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppStream {
    pub info: AppInfo,
    pub assigned: Option<StripId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PortDir {
    In,
    Out,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphNode {
    pub id: u32,
    pub name: String,
    pub description: String,
    pub media_class: String,
    /// One of flupmix's own nodes (virtual sinks/sources and the mixer).
    pub ours: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphPort {
    pub id: u32,
    pub node_id: u32,
    pub name: String,
    pub dir: PortDir,
    pub monitor: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphLink {
    pub id: u32,
    pub out_port: u32,
    pub in_port: u32,
    pub ours: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Graph {
    pub nodes: Vec<GraphNode>,
    pub ports: Vec<GraphPort>,
    pub links: Vec<GraphLink>,
}

/// Immutable view of the audio server published by the engine.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    /// False until the engine is connected to the audio server.
    pub connected: bool,
    pub sources: Vec<Device>,
    pub sinks: Vec<Device>,
    pub apps: Vec<AppStream>,
    pub graph: Graph,
}

/// UI to engine requests.
#[derive(Debug, Clone)]
pub enum Command {
    /// Make the audio graph match this mixer.
    SetState(Box<MixerState>),
    /// Patchbay: link two ports.
    Link { out_port: u32, in_port: u32 },
    /// Patchbay: remove a link.
    Unlink { link_id: u32 },
    /// Stop the engine thread.
    Quit,
}

/// State the engine publishes and the UI reads.
#[derive(Default)]
pub struct Shared {
    pub meters: Meters,
    snapshot: Mutex<Arc<Snapshot>>,
}

impl Shared {
    pub fn snapshot(&self) -> Arc<Snapshot> {
        self.snapshot.lock().expect("snapshot lock").clone()
    }

    pub fn publish(&self, snapshot: Snapshot) {
        *self.snapshot.lock().expect("snapshot lock") = Arc::new(snapshot);
    }
}

/// Cloneable handle the UI uses to talk to the engine.
#[derive(Clone)]
pub struct EngineHandle {
    pub shared: Arc<Shared>,
    send: Arc<dyn Fn(Command) + Send + Sync>,
}

impl EngineHandle {
    pub fn new(shared: Arc<Shared>, send: impl Fn(Command) + Send + Sync + 'static) -> Self {
        Self {
            shared,
            send: Arc::new(send),
        }
    }

    /// A handle that goes nowhere; for UI development without an audio server.
    pub fn detached() -> Self {
        Self::new(Arc::new(Shared::default()), |_| {})
    }

    pub fn send(&self, cmd: Command) {
        (self.send)(cmd);
    }

    pub fn set_state(&self, state: &MixerState) {
        self.send(Command::SetState(Box::new(state.clone())));
    }
}
