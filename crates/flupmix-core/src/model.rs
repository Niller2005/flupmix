//! The persisted mixer description. The engine reconciles the audio graph to
//! match whatever [`MixerState`] it is given.

use serde::{Deserialize, Serialize};

use crate::db::{MAX_DB, MIN_DB};
use crate::mix::{MAX_BUSES, MAX_STRIPS};

pub type StripId = u32;
pub type BusId = u32;

/// Case-insensitive app pattern: equals the process binary or is contained in
/// the application name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppMatch(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StripSource {
    /// A virtual sink apps can play into.
    Virtual,
    /// A hardware (or any other) capture source, remembered by `node.name`.
    Device { node_name: Option<String> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Strip {
    pub id: StripId,
    /// Mixer input slot, `0..MAX_STRIPS`.
    pub slot: u8,
    pub name: String,
    pub source: StripSource,
    pub gain_db: f32,
    pub mute: bool,
    pub solo: bool,
    /// Send to the hardware bus.
    pub hw: bool,
    /// Virtual buses this strip feeds.
    pub buses: Vec<BusId>,
    /// Level of the send to the hardware bus.
    #[serde(default)]
    pub hw_db: f32,
    /// Levels of the sends to virtual buses; a bus without an entry is at 0 dB.
    #[serde(default)]
    pub send_db: Vec<BusSend>,
    /// Virtual strips only: takes every app that no strip claims explicitly.
    #[serde(default)]
    pub catch_all: bool,
    /// Virtual strips only: apps pinned to this strip.
    #[serde(default)]
    pub apps: Vec<AppMatch>,
    /// Virtual strips only: apps that may never be routed here.
    #[serde(default)]
    pub blocked: Vec<AppMatch>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BusSend {
    pub bus: BusId,
    pub db: f32,
}

impl Strip {
    pub fn bus_send_db(&self, bus: BusId) -> f32 {
        self.send_db
            .iter()
            .find(|s| s.bus == bus)
            .map_or(0.0, |s| s.db)
    }

    pub fn set_bus_send_db(&mut self, bus: BusId, db: f32) {
        let db = db.clamp(MIN_DB, MAX_DB);
        match self.send_db.iter_mut().find(|s| s.bus == bus) {
            Some(s) => s.db = db,
            None => self.send_db.push(BusSend { bus, db }),
        }
    }
}

/// A virtual bus, exposed to other apps as a source (a "virtual mic").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bus {
    pub id: BusId,
    /// Mixer output slot, `1..=MAX_BUSES` (0 is the hardware bus).
    pub slot: u8,
    pub name: String,
    pub gain_db: f32,
    pub mute: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HwBus {
    pub name: String,
    /// `node.name` of the sink to play to; `None` follows the system default sink.
    pub sink: Option<String>,
    pub gain_db: f32,
    pub mute: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MixerState {
    pub strips: Vec<Strip>,
    pub buses: Vec<Bus>,
    pub hw: HwBus,
    next_id: u32,
}

impl Default for MixerState {
    fn default() -> Self {
        Self::empty()
    }
}

impl MixerState {
    pub fn empty() -> Self {
        Self {
            strips: Vec::new(),
            buses: Vec::new(),
            hw: HwBus {
                name: "Headphones".into(),
                sink: None,
                gain_db: 0.0,
                mute: false,
            },
            next_id: 1,
        }
    }

    /// First-launch layout: a mic, five virtual sinks and two virtual buses.
    pub fn starter() -> Self {
        let mut s = Self::empty();
        s.add_strip("Mic", StripSource::Device { node_name: None });
        let game = s
            .add_strip("Game", StripSource::Virtual)
            .expect("free slot");
        s.add_strip("Music", StripSource::Virtual);
        s.add_strip("Browser", StripSource::Virtual);
        s.add_strip("Desktop", StripSource::Virtual);
        s.add_strip("Comms", StripSource::Virtual);
        s.strip_mut(game).expect("just added").catch_all = true;
        let mic = s.strips[0].id;
        s.strip_mut(mic).expect("mic").hw = false;
        let b1 = s.add_bus("Bus 1").expect("free slot");
        s.add_bus("Bus 2");
        s.strip_mut(mic).expect("mic").buses.push(b1);
        s
    }

    fn alloc_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn add_strip(&mut self, name: &str, source: StripSource) -> Option<StripId> {
        let slot = (0..MAX_STRIPS as u8).find(|s| self.strips.iter().all(|x| x.slot != *s))?;
        let id = self.alloc_id();
        self.strips.push(Strip {
            id,
            slot,
            name: name.to_string(),
            source,
            gain_db: 0.0,
            mute: false,
            solo: false,
            hw: true,
            buses: Vec::new(),
            hw_db: 0.0,
            send_db: Vec::new(),
            catch_all: false,
            apps: Vec::new(),
            blocked: Vec::new(),
        });
        Some(id)
    }

    pub fn remove_strip(&mut self, id: StripId) {
        self.strips.retain(|s| s.id != id);
    }

    pub fn add_bus(&mut self, name: &str) -> Option<BusId> {
        let slot = (1..=MAX_BUSES as u8).find(|s| self.buses.iter().all(|x| x.slot != *s))?;
        let id = self.alloc_id();
        self.buses.push(Bus {
            id,
            slot,
            name: name.to_string(),
            gain_db: 0.0,
            mute: false,
        });
        Some(id)
    }

    pub fn remove_bus(&mut self, id: BusId) {
        self.buses.retain(|b| b.id != id);
        for s in &mut self.strips {
            s.buses.retain(|b| *b != id);
            s.send_db.retain(|x| x.bus != id);
        }
    }

    pub fn strip(&self, id: StripId) -> Option<&Strip> {
        self.strips.iter().find(|s| s.id == id)
    }

    pub fn strip_mut(&mut self, id: StripId) -> Option<&mut Strip> {
        self.strips.iter_mut().find(|s| s.id == id)
    }

    pub fn bus(&self, id: BusId) -> Option<&Bus> {
        self.buses.iter().find(|b| b.id == id)
    }

    pub fn bus_mut(&mut self, id: BusId) -> Option<&mut Bus> {
        self.buses.iter_mut().find(|b| b.id == id)
    }

    pub fn any_solo(&self) -> bool {
        self.strips.iter().any(|s| s.solo)
    }

    /// Repairs a hand-edited or old config: unique in-range slots, sane gains,
    /// no dangling bus references.
    pub fn sanitize(&mut self) {
        let mut used = [false; MAX_STRIPS];
        self.strips.retain_mut(|s| {
            let slot = s.slot as usize;
            if slot < MAX_STRIPS && !used[slot] {
                used[slot] = true;
                true
            } else {
                false
            }
        });
        let mut used = [false; MAX_BUSES + 1];
        self.buses.retain_mut(|b| {
            let slot = b.slot as usize;
            if (1..=MAX_BUSES).contains(&slot) && !used[slot] {
                used[slot] = true;
                true
            } else {
                false
            }
        });
        let bus_ids: Vec<BusId> = self.buses.iter().map(|b| b.id).collect();
        for s in &mut self.strips {
            s.gain_db = s.gain_db.clamp(MIN_DB, MAX_DB);
            s.hw_db = s.hw_db.clamp(MIN_DB, MAX_DB);
            s.send_db.retain(|x| bus_ids.contains(&x.bus));
            for x in &mut s.send_db {
                x.db = x.db.clamp(MIN_DB, MAX_DB);
            }
            s.buses.retain(|b| bus_ids.contains(b));
            if matches!(s.source, StripSource::Device { .. }) {
                s.catch_all = false;
                s.apps.clear();
                s.blocked.clear();
            }
        }
        for b in &mut self.buses {
            b.gain_db = b.gain_db.clamp(MIN_DB, MAX_DB);
        }
        self.hw.gain_db = self.hw.gain_db.clamp(MIN_DB, MAX_DB);
        let max_id = self
            .strips
            .iter()
            .map(|s| s.id)
            .chain(self.buses.iter().map(|b| b.id))
            .max()
            .unwrap_or(0);
        self.next_id = self.next_id.max(max_id + 1);
        // At most one catch-all strip.
        let mut seen = false;
        for s in &mut self.strips {
            if s.catch_all {
                if seen {
                    s.catch_all = false;
                }
                seen = true;
            }
        }
    }
}

/// `node.name` of the virtual sink backing a strip.
pub fn strip_node_name(id: StripId) -> String {
    format!("flupmix.strip.{id}")
}

/// `node.name` of the virtual source backing a bus.
pub fn bus_node_name(id: BusId) -> String {
    format!("flupmix.bus.{id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starter_layout_is_valid() {
        let mut s = MixerState::starter();
        let before = s.clone();
        s.sanitize();
        assert_eq!(s, before);
        assert_eq!(s.strips.len(), 6);
        assert_eq!(s.buses.len(), 2);
        assert_eq!(s.strips.iter().filter(|s| s.catch_all).count(), 1);
    }

    #[test]
    fn slots_are_reused_and_capped() {
        let mut s = MixerState::empty();
        let ids: Vec<_> = (0..MAX_STRIPS)
            .map(|i| s.add_strip(&format!("s{i}"), StripSource::Virtual).unwrap())
            .collect();
        assert!(s.add_strip("overflow", StripSource::Virtual).is_none());
        s.remove_strip(ids[3]);
        let id = s.add_strip("again", StripSource::Virtual).unwrap();
        assert_eq!(s.strip(id).unwrap().slot, 3);
    }

    #[test]
    fn removing_a_bus_clears_sends() {
        let mut s = MixerState::starter();
        let bus = s.buses[0].id;
        assert!(s.strips.iter().any(|x| x.buses.contains(&bus)));
        s.remove_bus(bus);
        assert!(s.strips.iter().all(|x| !x.buses.contains(&bus)));
    }

    #[test]
    fn sanitize_drops_duplicate_slots_and_extra_catch_alls() {
        let mut s = MixerState::starter();
        s.strips[2].slot = s.strips[1].slot;
        s.strips[3].catch_all = true;
        s.sanitize();
        assert_eq!(s.strips.len(), 5);
        assert_eq!(s.strips.iter().filter(|s| s.catch_all).count(), 1);
    }
}
