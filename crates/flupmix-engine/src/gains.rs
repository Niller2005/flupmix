//! Lock-free hand-off of the gain matrix from the control thread to the
//! real-time process callback.

use std::sync::atomic::{AtomicU32, Ordering};

use flupmix_core::mix::{Matrix, MAX_STRIPS, N_DEST};

pub struct Controls {
    strip: [AtomicU32; MAX_STRIPS],
    send: [AtomicU32; MAX_STRIPS * N_DEST],
    mono_mask: AtomicU32,
}

impl Controls {
    pub fn new() -> Self {
        Self {
            strip: std::array::from_fn(|_| AtomicU32::new(0)),
            send: std::array::from_fn(|_| AtomicU32::new(0)),
            mono_mask: AtomicU32::new(0),
        }
    }

    pub fn store(&self, m: &Matrix) {
        for (cell, v) in self.strip.iter().zip(&m.strip) {
            cell.store(v.to_bits(), Ordering::Relaxed);
        }
        for (s, row) in m.send.iter().enumerate() {
            for (d, v) in row.iter().enumerate() {
                self.send[s * N_DEST + d].store(v.to_bits(), Ordering::Relaxed);
            }
        }
    }

    pub fn load(&self, m: &mut Matrix) {
        for (cell, v) in self.strip.iter().zip(m.strip.iter_mut()) {
            *v = f32::from_bits(cell.load(Ordering::Relaxed));
        }
        for (s, row) in m.send.iter_mut().enumerate() {
            for (d, v) in row.iter_mut().enumerate() {
                *v = f32::from_bits(self.send[s * N_DEST + d].load(Ordering::Relaxed));
            }
        }
    }

    pub fn set_mono_mask(&self, mask: u32) {
        self.mono_mask.store(mask, Ordering::Relaxed);
    }

    pub fn mono_mask(&self) -> u32 {
        self.mono_mask.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flupmix_core::MixerState;

    #[test]
    fn matrix_round_trips() {
        let state = MixerState::starter();
        let m = Matrix::from_state(&state);
        let c = Controls::new();
        c.store(&m);
        let mut out = Matrix::SILENT;
        c.load(&mut out);
        assert_eq!(out, m);
    }
}
