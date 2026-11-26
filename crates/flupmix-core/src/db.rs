//! Decibel helpers and the fader taper.
//!
//! The fader scale matches the UI: 0 dB sits near the top, the lower half of
//! the travel covers -60..-20 dB and the upper part covers -20..+12 dB.

pub const MIN_DB: f32 = -60.0;
pub const MAX_DB: f32 = 12.0;
/// dB value at the knee of the two-segment taper.
const KNEE_DB: f32 = -20.0;
/// Fader position (0..1) at the knee.
const KNEE_POS: f32 = 0.42;

/// Linear gain for a dB value. Anything at or below [`MIN_DB`] is silence.
pub fn db_to_gain(db: f32) -> f32 {
    if db <= MIN_DB {
        0.0
    } else {
        10f32.powf(db / 20.0)
    }
}

/// dB for a linear gain/peak, floored at [`MIN_DB`].
pub fn gain_to_db(gain: f32) -> f32 {
    if gain <= 0.0 {
        MIN_DB
    } else {
        (20.0 * gain.log10()).max(MIN_DB)
    }
}

/// Fader position (0.0 bottom .. 1.0 top) for a dB value.
pub fn pos_from_db(db: f32) -> f32 {
    let db = db.clamp(MIN_DB, MAX_DB);
    if db <= KNEE_DB {
        (db - MIN_DB) / (KNEE_DB - MIN_DB) * KNEE_POS
    } else {
        KNEE_POS + (db - KNEE_DB) / (MAX_DB - KNEE_DB) * (1.0 - KNEE_POS)
    }
}

/// dB value for a fader position (0.0 bottom .. 1.0 top).
pub fn db_from_pos(pos: f32) -> f32 {
    let pos = pos.clamp(0.0, 1.0);
    if pos <= KNEE_POS {
        MIN_DB + pos / KNEE_POS * (KNEE_DB - MIN_DB)
    } else {
        KNEE_DB + (pos - KNEE_POS) / (1.0 - KNEE_POS) * (MAX_DB - KNEE_DB)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unity_is_zero_db() {
        assert!((db_to_gain(0.0) - 1.0).abs() < 1e-6);
        assert!(gain_to_db(1.0).abs() < 1e-4);
    }

    #[test]
    fn floor_is_silence() {
        assert_eq!(db_to_gain(MIN_DB), 0.0);
        assert_eq!(gain_to_db(0.0), MIN_DB);
    }

    #[test]
    fn taper_round_trips_and_hits_ends() {
        assert_eq!(pos_from_db(MIN_DB), 0.0);
        assert_eq!(pos_from_db(MAX_DB), 1.0);
        for db in [-60.0, -40.0, -20.0, -6.0, 0.0, 6.0, 12.0] {
            assert!((db_from_pos(pos_from_db(db)) - db).abs() < 1e-3, "{db}");
        }
    }

    #[test]
    fn zero_db_sits_near_the_top() {
        let pos = pos_from_db(0.0);
        assert!((0.77..0.80).contains(&pos), "{pos}");
    }
}
