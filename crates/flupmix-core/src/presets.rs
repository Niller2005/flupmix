//! Named mixer presets: whole [`MixerState`] snapshots stored one per file in
//! `$XDG_CONFIG_HOME/flupmix/presets/<name>.toml`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::config::{config_path, Config};
use crate::model::MixerState;

const MAX_NAME_CHARS: usize = 64;

pub fn presets_dir() -> PathBuf {
    config_path()
        .parent()
        .map_or_else(|| PathBuf::from("presets"), |p| p.join("presets"))
}

/// Trims a user-entered name and rejects ones that cannot be a file name.
pub fn clean_name(name: &str) -> Option<String> {
    let name = name.trim();
    let bad = |c: char| {
        c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
    };
    if name.is_empty()
        || name.chars().count() > MAX_NAME_CHARS
        || name.starts_with('.')
        || name.chars().any(bad)
    {
        None
    } else {
        Some(name.to_string())
    }
}

fn file_for(dir: &Path, name: &str) -> io::Result<PathBuf> {
    let name = clean_name(name)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid preset name"))?;
    Ok(dir.join(format!("{name}.toml")))
}

/// Preset names in `dir`, sorted case-insensitively. A missing directory is empty.
pub fn list_in(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let path = e.path();
            (path.extension()? == "toml").then(|| path.file_stem()?.to_str().map(str::to_string))?
        })
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names
}

pub fn save_in(dir: &Path, name: &str, state: &MixerState) -> io::Result<()> {
    let path = file_for(dir, name)?;
    fs::create_dir_all(dir)?;
    let text = toml::to_string_pretty(&Config {
        mixer: state.clone(),
    })
    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, text)?;
    fs::rename(tmp, path)
}

pub fn load_from(dir: &Path, name: &str) -> io::Result<MixerState> {
    let text = fs::read_to_string(file_for(dir, name)?)?;
    let mut cfg: Config =
        toml::from_str(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    cfg.mixer.sanitize();
    Ok(cfg.mixer)
}

pub fn delete_in(dir: &Path, name: &str) -> io::Result<()> {
    fs::remove_file(file_for(dir, name)?)
}

pub fn list() -> Vec<String> {
    list_in(&presets_dir())
}

pub fn save(name: &str, state: &MixerState) -> io::Result<()> {
    save_in(&presets_dir(), name, state)
}

pub fn load(name: &str) -> io::Result<MixerState> {
    load_from(&presets_dir(), name)
}

pub fn delete(name: &str) -> io::Result<()> {
    delete_in(&presets_dir(), name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("flupmix-presets-{tag}-{}", std::process::id()))
    }

    #[test]
    fn save_list_load_delete_round_trip() {
        let dir = temp_dir("rt");
        let mut state = MixerState::starter();
        state.strips[1].hw_db = -9.0;
        save_in(&dir, "Streaming", &state).unwrap();
        save_in(&dir, "anime night", &MixerState::empty()).unwrap();
        assert_eq!(list_in(&dir), vec!["anime night", "Streaming"]);
        assert_eq!(load_from(&dir, "Streaming").unwrap(), state);
        delete_in(&dir, "Streaming").unwrap();
        assert_eq!(list_in(&dir), vec!["anime night"]);
        assert!(load_from(&dir, "Streaming").is_err());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn names_cannot_escape_the_directory() {
        let dir = temp_dir("names");
        for bad in ["", "  ", "../x", "a/b", ".hidden", "a\\b", "x:y"] {
            assert!(save_in(&dir, bad, &MixerState::empty()).is_err(), "{bad:?}");
        }
        assert_eq!(clean_name("  My mix  ").as_deref(), Some("My mix"));
        assert!(list_in(&dir).is_empty());
    }

    #[test]
    fn corrupt_preset_is_an_error_not_a_panic() {
        let dir = temp_dir("bad");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("broken.toml"), "not [valid").unwrap();
        assert!(load_from(&dir, "broken").is_err());
        let _ = fs::remove_dir_all(dir);
    }
}
