//! Runs the engine with the starter layout and prints meters, for smoke tests
//! without a UI: `cargo run -p flupmix-engine --example headless`.

use std::time::Duration;

use flupmix_core::db::gain_to_db;
use flupmix_core::MixerState;

fn main() {
    // Silent smoke-test routing: only an app named "testapp" is captured, by
    // the Game strip, and it feeds Bus 1 instead of the hardware output.
    let mut state = MixerState::starter();
    let bus = state.buses[0].id;
    for strip in &mut state.strips {
        strip.catch_all = false;
        if strip.name == "Game" {
            strip.apps.push(flupmix_core::AppMatch("testapp".into()));
            strip.hw = false;
            strip.buses.push(bus);
        }
    }
    let engine = flupmix_engine::spawn(state.clone());
    let handle = engine.handle();
    loop {
        std::thread::sleep(Duration::from_millis(500));
        let snap = handle.shared.snapshot();
        let strips: Vec<String> = state
            .strips
            .iter()
            .map(|s| {
                let (l, r) = handle.shared.meters.strips[s.slot as usize].take();
                format!("{}:{:.0}/{:.0}", s.name, gain_to_db(l), gain_to_db(r))
            })
            .collect();
        let (hl, hr) = handle.shared.meters.dests[1].take();
        println!(
            "connected={} nodes={} links={} apps={} bus1={:.0}/{:.0} | {}",
            snap.connected,
            snap.graph.nodes.len(),
            snap.graph.links.len(),
            snap.apps.len(),
            gain_to_db(hl),
            gain_to_db(hr),
            strips.join(" ")
        );
    }
}
