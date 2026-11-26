//! Which virtual strip an application stream belongs on.

use crate::model::{AppMatch, MixerState, Strip, StripId, StripSource};

/// A running application playback stream, as seen in the audio graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppInfo {
    pub node_id: u32,
    /// `application.name`, falling back to `node.name`.
    pub name: String,
    /// `application.process.binary`, possibly empty.
    pub binary: String,
}

impl AppMatch {
    pub fn matches(&self, app: &AppInfo) -> bool {
        let pat = self.0.trim().to_lowercase();
        if pat.is_empty() {
            return false;
        }
        app.binary.to_lowercase() == pat || app.name.to_lowercase().contains(&pat)
    }
}

fn is_virtual(s: &Strip) -> bool {
    matches!(s.source, StripSource::Virtual)
}

fn blocked(s: &Strip, app: &AppInfo) -> bool {
    s.blocked.iter().any(|m| m.matches(app))
}

/// Explicit assignment wins; otherwise the catch-all strip takes the app unless
/// it blocks it. `None` leaves the app on the system default.
pub fn resolve_target(state: &MixerState, app: &AppInfo) -> Option<StripId> {
    let explicit = state
        .strips
        .iter()
        .filter(|s| is_virtual(s))
        .find(|s| !blocked(s, app) && s.apps.iter().any(|m| m.matches(app)));
    if let Some(s) = explicit {
        return Some(s.id);
    }
    state
        .strips
        .iter()
        .find(|s| is_virtual(s) && s.catch_all && !blocked(s, app))
        .map(|s| s.id)
}

/// Adds `pattern` to a list unless an equal (case-insensitive) entry exists.
pub fn add_pattern(list: &mut Vec<AppMatch>, pattern: &str) {
    let pattern = pattern.trim();
    if pattern.is_empty() || list.iter().any(|m| m.0.eq_ignore_ascii_case(pattern)) {
        return;
    }
    list.push(AppMatch(pattern.to_string()));
}

/// The pattern used when the UI assigns or blocks a running app: the binary
/// when known (stable across title changes), otherwise the name.
pub fn pattern_for(app: &AppInfo) -> String {
    if app.binary.is_empty() {
        app.name.clone()
    } else {
        app.binary.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(name: &str, binary: &str) -> AppInfo {
        AppInfo {
            node_id: 1,
            name: name.into(),
            binary: binary.into(),
        }
    }

    fn state() -> (MixerState, StripId, StripId) {
        let mut s = MixerState::empty();
        let game = s.add_strip("Game", StripSource::Virtual).unwrap();
        let music = s.add_strip("Music", StripSource::Virtual).unwrap();
        s.strip_mut(game).unwrap().catch_all = true;
        add_pattern(&mut s.strip_mut(music).unwrap().apps, "spotify");
        (s, game, music)
    }

    #[test]
    fn explicit_beats_catch_all() {
        let (s, game, music) = state();
        assert_eq!(resolve_target(&s, &app("Spotify", "spotify")), Some(music));
        assert_eq!(
            resolve_target(&s, &app("Some Game", "game.exe")),
            Some(game)
        );
    }

    #[test]
    fn blocked_apps_skip_the_strip() {
        let (mut s, game, music) = state();
        add_pattern(&mut s.strip_mut(game).unwrap().blocked, "firefox");
        assert_eq!(resolve_target(&s, &app("Firefox", "firefox")), None);
        add_pattern(&mut s.strip_mut(music).unwrap().blocked, "spotify");
        assert_eq!(resolve_target(&s, &app("Spotify", "spotify")), Some(game));
    }

    #[test]
    fn device_strips_never_receive_apps() {
        let mut s = MixerState::empty();
        let mic = s
            .add_strip("Mic", StripSource::Device { node_name: None })
            .unwrap();
        let st = s.strip_mut(mic).unwrap();
        st.catch_all = true;
        add_pattern(&mut st.apps, "spotify");
        assert_eq!(resolve_target(&s, &app("Spotify", "spotify")), None);
    }

    #[test]
    fn matching_is_case_insensitive_and_name_is_substring() {
        let m = AppMatch("spot".into());
        assert!(m.matches(&app("Spotify", "")));
        assert!(
            !m.matches(&app("x", "spotify")),
            "binary must match exactly"
        );
        assert!(!AppMatch("  ".into()).matches(&app("anything", "")));
    }

    #[test]
    fn add_pattern_dedupes() {
        let mut l = Vec::new();
        add_pattern(&mut l, "Spotify");
        add_pattern(&mut l, "spotify");
        add_pattern(&mut l, " ");
        assert_eq!(l.len(), 1);
    }
}
