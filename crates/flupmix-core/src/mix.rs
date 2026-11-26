//! Gain matrix and the real-time mixing kernel. Nothing here allocates, so
//! [`Mixer::process`] is safe to call from an audio callback.

use crate::db::db_to_gain;
use crate::model::MixerState;
use std::sync::atomic::{AtomicU32, Ordering};

pub const MAX_STRIPS: usize = 16;
pub const MAX_BUSES: usize = 8;
/// Mixer outputs: destination 0 is the hardware bus, 1..=MAX_BUSES are virtual buses.
pub const N_DEST: usize = MAX_BUSES + 1;

/// Target gains derived from a [`MixerState`].
#[derive(Debug, Clone, PartialEq)]
pub struct Matrix {
    /// Strip fader after mute and solo.
    pub strip: [f32; MAX_STRIPS],
    /// Per strip per destination: route flag times the destination fader/mute.
    pub send: [[f32; N_DEST]; MAX_STRIPS],
}

impl Matrix {
    pub const SILENT: Matrix = Matrix {
        strip: [0.0; MAX_STRIPS],
        send: [[0.0; N_DEST]; MAX_STRIPS],
    };

    /// Builds the matrix. Solo is global: when any strip is soloed, every
    /// non-soloed strip is silenced on every bus.
    pub fn from_state(state: &MixerState) -> Matrix {
        let mut m = Matrix::SILENT;
        let any_solo = state.any_solo();
        let hw_gain = if state.hw.mute {
            0.0
        } else {
            db_to_gain(state.hw.gain_db)
        };
        for s in &state.strips {
            let slot = s.slot as usize;
            if slot >= MAX_STRIPS {
                continue;
            }
            let audible = !s.mute && (!any_solo || s.solo);
            m.strip[slot] = if audible { db_to_gain(s.gain_db) } else { 0.0 };
            if s.hw {
                m.send[slot][0] = hw_gain * db_to_gain(s.hw_db);
            }
            for bus_id in &s.buses {
                if let Some(b) = state.bus(*bus_id) {
                    let dest = b.slot as usize;
                    if (1..N_DEST).contains(&dest) {
                        m.send[slot][dest] = if b.mute {
                            0.0
                        } else {
                            db_to_gain(b.gain_db) * db_to_gain(s.bus_send_db(b.id))
                        };
                    }
                }
            }
        }
        m
    }
}

/// Lock-free peak holder shared with the UI. Peaks are non-negative, so the
/// raw f32 bit patterns order the same way as the values.
#[derive(Default)]
pub struct PeakPair {
    l: AtomicU32,
    r: AtomicU32,
}

impl PeakPair {
    pub fn push(&self, l: f32, r: f32) {
        self.l.fetch_max(l.to_bits(), Ordering::Relaxed);
        self.r.fetch_max(r.to_bits(), Ordering::Relaxed);
    }

    /// Peak (linear) since the last call.
    pub fn take(&self) -> (f32, f32) {
        (
            f32::from_bits(self.l.swap(0, Ordering::Relaxed)),
            f32::from_bits(self.r.swap(0, Ordering::Relaxed)),
        )
    }
}

/// Post-fader peak meters for every strip and destination.
#[derive(Default)]
pub struct Meters {
    pub strips: [PeakPair; MAX_STRIPS],
    pub dests: [PeakPair; N_DEST],
}

/// Mixer with per-block gain ramps to avoid zipper noise.
pub struct Mixer {
    strip: [f32; MAX_STRIPS],
    send: [[f32; N_DEST]; MAX_STRIPS],
}

impl Default for Mixer {
    fn default() -> Self {
        Self::new()
    }
}

pub type ChannelIn<'a> = Option<&'a [f32]>;
pub type ChannelOut<'a> = Option<&'a mut [f32]>;

fn peak(buf: &[f32], gain_from: f32, gain_to: f32) -> f32 {
    let n = buf.len().max(1) as f32;
    let step = (gain_to - gain_from) / n;
    let mut g = gain_from;
    let mut p = 0.0f32;
    for &x in buf {
        g += step;
        p = p.max((x * g).abs());
    }
    p
}

impl Mixer {
    pub fn new() -> Self {
        Self {
            strip: [0.0; MAX_STRIPS],
            send: [[0.0; N_DEST]; MAX_STRIPS],
        }
    }

    /// Mixes one block. `ins[strip][channel]` is `None` when the port has no
    /// buffer this cycle; `outs[dest][channel]` likewise. All present slices
    /// must be at least `n` long. Outputs are overwritten.
    pub fn process(
        &mut self,
        target: &Matrix,
        n: usize,
        ins: &[[ChannelIn; 2]; MAX_STRIPS],
        outs: &mut [[ChannelOut; 2]; N_DEST],
        meters: &Meters,
    ) {
        for dest in outs.iter_mut() {
            for ch in dest.iter_mut().flatten() {
                ch[..n].fill(0.0);
            }
        }
        for s in 0..MAX_STRIPS {
            let (sg0, sg1) = (self.strip[s], target.strip[s]);
            let both_idle = sg0 == 0.0 && sg1 == 0.0;
            if !both_idle {
                let l = ins[s][0].map_or(0.0, |b| peak(&b[..n], sg0, sg1));
                let r = ins[s][1].map_or(l, |b| peak(&b[..n], sg0, sg1));
                meters.strips[s].push(l, r);
            }
            for d in 0..N_DEST {
                let g0 = sg0 * self.send[s][d];
                let g1 = sg1 * target.send[s][d];
                if g0 == 0.0 && g1 == 0.0 {
                    continue;
                }
                let step = (g1 - g0) / n.max(1) as f32;
                for c in 0..2 {
                    // A mono (single connected) strip feeds both channels.
                    let Some(src) = ins[s][c].or(ins[s][0]) else {
                        continue;
                    };
                    let Some(dst) = outs[d][c].as_deref_mut() else {
                        continue;
                    };
                    let mut g = g0;
                    for (o, &x) in dst[..n].iter_mut().zip(&src[..n]) {
                        g += step;
                        *o += x * g;
                    }
                }
            }
            self.strip[s] = sg1;
            self.send[s] = target.send[s];
        }
        for d in 0..N_DEST {
            let l = outs[d][0]
                .as_deref()
                .map_or(0.0, |b| peak(&b[..n], 1.0, 1.0));
            let r = outs[d][1]
                .as_deref()
                .map_or(0.0, |b| peak(&b[..n], 1.0, 1.0));
            meters.dests[d].push(l, r);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::StripSource;

    fn state() -> MixerState {
        let mut s = MixerState::empty();
        s.add_strip("a", StripSource::Virtual);
        s.add_strip("b", StripSource::Virtual);
        let bus = s.add_bus("bus").unwrap();
        s.strips[0].buses.push(bus);
        s
    }

    #[test]
    fn solo_is_global() {
        let mut s = state();
        s.strips[1].solo = true;
        let m = Matrix::from_state(&s);
        assert_eq!(m.strip[0], 0.0);
        assert!(m.strip[1] > 0.0);
        assert!(
            m.send[0][1] > 0.0,
            "route flags stay; the strip gain silences it"
        );
    }

    #[test]
    fn mute_and_bus_fader_fold_into_matrix() {
        let mut s = state();
        s.buses[0].gain_db = -6.0;
        let m = Matrix::from_state(&s);
        assert!((m.send[0][1] - db_to_gain(-6.0)).abs() < 1e-6);
        assert_eq!(m.send[1][1], 0.0, "strip b is not routed to the bus");
        s.strips[0].mute = true;
        assert_eq!(Matrix::from_state(&s).strip[0], 0.0);
        s.buses[0].mute = true;
        assert_eq!(Matrix::from_state(&s).send[0][1], 0.0);
    }

    #[test]
    fn per_strip_send_levels_scale_the_sends() {
        let mut s = state();
        let bus = s.buses[0].id;
        s.strips[0].hw_db = -6.0;
        s.strips[0].set_bus_send_db(bus, -12.0);
        let m = Matrix::from_state(&s);
        assert!((m.send[0][0] - db_to_gain(-6.0)).abs() < 1e-6);
        assert!((m.send[0][1] - db_to_gain(-12.0)).abs() < 1e-6);
        assert!((m.send[1][0] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn hw_send_follows_hw_bus() {
        let mut s = state();
        s.hw.mute = true;
        assert_eq!(Matrix::from_state(&s).send[0][0], 0.0);
        s.hw.mute = false;
        s.strips[0].hw = false;
        assert_eq!(Matrix::from_state(&s).send[0][0], 0.0);
        assert!(Matrix::from_state(&s).send[1][0] > 0.0);
    }

    fn run(
        m: &mut Mixer,
        target: &Matrix,
        a: &[f32],
        b: &[f32],
        meters: &Meters,
    ) -> ([f32; 4], [f32; 4]) {
        let mut hw_l = [9.0f32; 4];
        let mut bus_l = [9.0f32; 4];
        {
            let mut ins: [[ChannelIn; 2]; MAX_STRIPS] = Default::default();
            ins[0][0] = Some(a);
            ins[1][0] = Some(b);
            let mut outs: [[ChannelOut; 2]; N_DEST] = Default::default();
            outs[0][0] = Some(&mut hw_l);
            outs[1][0] = Some(&mut bus_l);
            m.process(target, 4, &ins, &mut outs, meters);
        }
        (hw_l, bus_l)
    }

    #[test]
    fn sums_strips_per_destination() {
        let s = state();
        let target = Matrix::from_state(&s);
        let meters = Meters::default();
        let mut m = Mixer::new();
        // First block ramps up from silence; the second is steady-state.
        run(&mut m, &target, &[0.5; 4], &[0.25; 4], &meters);
        let (hw, bus) = run(&mut m, &target, &[0.5; 4], &[0.25; 4], &meters);
        assert!(hw.iter().all(|x| (x - 0.75).abs() < 1e-6), "{hw:?}");
        assert!(bus.iter().all(|x| (x - 0.5).abs() < 1e-6), "{bus:?}");
    }

    #[test]
    fn gain_changes_ramp_instead_of_jumping() {
        let s = state();
        let meters = Meters::default();
        let mut m = Mixer::new();
        let target = Matrix::from_state(&s);
        run(&mut m, &target, &[1.0; 4], &[0.0; 4], &meters);
        let mut muted = s.clone();
        muted.strips[0].mute = true;
        let (hw, _) = run(
            &mut m,
            &Matrix::from_state(&muted),
            &[1.0; 4],
            &[0.0; 4],
            &meters,
        );
        assert!(hw[0] < 1.0 && hw[0] > hw[1] && hw[3].abs() < 1e-6, "{hw:?}");
    }

    #[test]
    fn meters_report_post_fader_peak_and_reset_on_take() {
        let mut s = state();
        s.strips[0].gain_db = -6.0;
        let target = Matrix::from_state(&s);
        let meters = Meters::default();
        let mut m = Mixer::new();
        run(&mut m, &target, &[1.0; 4], &[0.0; 4], &meters);
        meters.strips[0].take();
        run(&mut m, &target, &[1.0; 4], &[0.0; 4], &meters);
        let (l, _) = meters.strips[0].take();
        assert!((l - db_to_gain(-6.0)).abs() < 1e-4, "{l}");
        assert_eq!(meters.strips[0].take(), (0.0, 0.0));
    }

    #[test]
    fn mono_strip_feeds_both_channels() {
        let mut s = MixerState::empty();
        s.add_strip("mic", StripSource::Virtual);
        let target = Matrix::from_state(&s);
        let meters = Meters::default();
        let mut m = Mixer::new();
        let (a, mut l, mut r) = ([0.5f32; 2], [0.0f32; 2], [0.0f32; 2]);
        for _ in 0..2 {
            let mut ins: [[ChannelIn; 2]; MAX_STRIPS] = Default::default();
            ins[0][0] = Some(&a);
            let mut outs: [[ChannelOut; 2]; N_DEST] = Default::default();
            outs[0] = [Some(&mut l), Some(&mut r)];
            m.process(&target, 2, &ins, &mut outs, &meters);
        }
        assert_eq!(l, r);
        assert!((l[0] - 0.5).abs() < 1e-6);
    }
}
