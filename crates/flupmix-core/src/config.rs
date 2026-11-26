//! TOML persistence in `$XDG_CONFIG_HOME/flupmix/config.toml`.

use std::fs;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::model::MixerState;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    pub mixer: MixerState,
}

pub fn config_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("flupmix").join("config.toml")
}

/// Loads the saved mixer, or the starter layout on first launch. A corrupt
/// file is moved aside to `config.toml.bad` rather than silently overwritten.
pub fn load_or_starter() -> MixerState {
    load_from(&config_path())
}

pub fn load_from(path: &std::path::Path) -> MixerState {
    match fs::read_to_string(path) {
        Ok(text) => match toml::from_str::<Config>(&text) {
            Ok(mut cfg) => {
                cfg.mixer.sanitize();
                cfg.mixer
            }
            Err(err) => {
                eprintln!(
                    "flupmix: bad config {}: {err}; starting fresh",
                    path.display()
                );
                let _ = fs::rename(path, path.with_extension("toml.bad"));
                MixerState::starter()
            }
        },
        Err(_) => MixerState::starter(),
    }
}

pub fn save(state: &MixerState) -> io::Result<()> {
    save_to(&config_path(), state)
}

pub fn save_to(path: &std::path::Path, state: &MixerState) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let text = toml::to_string_pretty(&Config {
        mixer: state.clone(),
    })
    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, text)?;
    fs::rename(tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_toml() {
        let dir = std::env::temp_dir().join(format!("flupmix-test-{}", std::process::id()));
        let path = dir.join("config.toml");
        let mut state = MixerState::starter();
        state.strips[1].gain_db = -12.5;
        state.hw.sink = Some("alsa_output.x".into());
        save_to(&path, &state).unwrap();
        assert_eq!(load_from(&path), state);
        fs::write(&path, "not [valid").unwrap();
        assert_eq!(load_from(&path), MixerState::starter());
        assert!(path.with_extension("toml.bad").exists());
        let _ = fs::remove_dir_all(dir);
    }
}
