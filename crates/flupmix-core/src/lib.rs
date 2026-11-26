//! Pure, audio-server-agnostic core of flupmix: the mixer model, gain
//! taper, the real-time mixing kernel, app routing rules, persistence and the
//! types shared between the engine thread and the UI.

pub mod config;
pub mod db;
pub mod mix;
pub mod model;
pub mod presets;
pub mod rules;
pub mod settings;
pub mod shared;

pub use model::{AppMatch, Bus, HwBus, MixerState, Strip, StripSource};
pub use shared::{Command, EngineHandle, Shared, Snapshot};
