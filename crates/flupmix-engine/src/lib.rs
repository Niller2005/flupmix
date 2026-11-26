//! PipeWire engine: owns the mixer filter node, the virtual sinks/sources, the
//! links between them and hardware, and the routing of application streams.

mod filter;
mod gains;
mod mirror;
mod session;

pub use session::{spawn, RunningEngine};
