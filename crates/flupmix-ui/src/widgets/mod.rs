pub(crate) mod button;
pub(crate) mod fader;
pub(crate) mod send_slider;
pub(crate) mod text_field;

pub(crate) fn drag_gain(gain: f32, snap_to_zero: bool) -> f32 {
    let gain = gain.clamp(flupmix_core::db::MIN_DB, flupmix_core::db::MAX_DB);
    if snap_to_zero && gain.abs() <= 1. {
        0.
    } else {
        gain
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drag_snaps_only_inside_the_enabled_unity_band() {
        for gain in [-1., -0.5, 0., 0.5, 1.] {
            assert_eq!(drag_gain(gain, true), 0.);
        }
        assert_eq!(drag_gain(1.1, true), 1.1);
        assert_eq!(drag_gain(-1.1, true), -1.1);
        assert_eq!(drag_gain(0.5, false), 0.5);
    }

    #[test]
    fn dragging_clamps_at_the_fader_limits() {
        assert_eq!(drag_gain(-100., true), flupmix_core::db::MIN_DB);
        assert_eq!(drag_gain(100., true), flupmix_core::db::MAX_DB);
    }
}
