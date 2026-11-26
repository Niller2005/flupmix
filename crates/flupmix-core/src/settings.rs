//! Application settings (not mixer state), stored in
//! `$XDG_CONFIG_HOME/flupmix/settings.toml`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::config_path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Palette {
    /// Cyberdream, high-contrast variant.
    #[default]
    CyberdreamHighContrast,
    /// Cyberdream, regular dark variant.
    Cyberdream,
    /// The original flup·trade mint-on-green look.
    Mint,
}

impl Palette {
    pub const ALL: [Palette; 3] = [
        Palette::CyberdreamHighContrast,
        Palette::Cyberdream,
        Palette::Mint,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Palette::CyberdreamHighContrast => "Cyberdream high contrast",
            Palette::Cyberdream => "Cyberdream",
            Palette::Mint => "Mint",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Closing the window keeps the mixer running in the tray (needs a tray host).
    pub close_to_tray: bool,
    /// Start with the window hidden (needs a tray host).
    pub start_minimised: bool,
    /// Start flupmix when you log in (XDG autostart entry).
    pub autostart: bool,
    /// Route application streams by the strip rules; off leaves apps alone.
    pub app_routing: bool,
    /// Meter refresh rate in Hz (30 or 60).
    pub meter_hz: u32,
    /// Seconds a meter peak marker is held.
    pub peak_hold_secs: f32,
    /// Meter fall-off in dB per second.
    pub meter_fall_db_per_sec: f32,
    /// Peaks above this level light the clip indicator.
    pub clip_db: f32,
    /// Fader step per mouse-wheel notch, in dB.
    pub wheel_step_db: f32,
    /// Fader step per notch while Shift is held, in dB.
    pub wheel_fine_step_db: f32,
    /// Level a double-click resets a fader or send to, in dB.
    pub reset_db: f32,
    /// Dragging a fader snaps to 0 dB when within 1 dB of it.
    pub snap_to_zero: bool,
    pub palette: Palette,
    /// Restore the last window size on launch.
    pub remember_window_size: bool,
    /// Last window size in logical pixels, written by the UI.
    pub window_size: Option<(f32, f32)>,
    /// Alt+1..9 mutes strips, Ctrl+Shift+1..9 loads presets (window focused).
    pub shortcuts: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            close_to_tray: true,
            start_minimised: false,
            autostart: false,
            app_routing: true,
            meter_hz: 30,
            peak_hold_secs: 1.5,
            meter_fall_db_per_sec: 24.0,
            clip_db: 0.0,
            wheel_step_db: 0.5,
            wheel_fine_step_db: 0.1,
            reset_db: 0.0,
            snap_to_zero: true,
            palette: Palette::default(),
            remember_window_size: true,
            window_size: None,
            shortcuts: true,
        }
    }
}

impl Settings {
    /// Clamps hand-edited values into sane ranges.
    pub fn sanitize(&mut self) {
        self.meter_hz = if self.meter_hz >= 45 { 60 } else { 30 };
        self.peak_hold_secs = self.peak_hold_secs.clamp(0.0, 10.0);
        self.meter_fall_db_per_sec = self.meter_fall_db_per_sec.clamp(6.0, 120.0);
        self.clip_db = self.clip_db.clamp(-12.0, 6.0);
        self.wheel_step_db = self.wheel_step_db.clamp(0.1, 6.0);
        self.wheel_fine_step_db = self.wheel_fine_step_db.clamp(0.05, 2.0);
        self.reset_db = self.reset_db.clamp(crate::db::MIN_DB, crate::db::MAX_DB);
        if let Some((w, h)) = self.window_size {
            if !(w.is_finite() && h.is_finite()) || w < 400.0 || h < 300.0 {
                self.window_size = None;
            }
        }
    }
}

pub fn settings_path() -> PathBuf {
    config_path().with_file_name("settings.toml")
}

/// Loads settings; a missing or corrupt file yields the defaults (a corrupt
/// one is moved aside to `settings.toml.bad`).
pub fn load() -> Settings {
    load_from(&settings_path())
}

pub fn load_from(path: &Path) -> Settings {
    let Ok(text) = fs::read_to_string(path) else {
        return Settings::default();
    };
    match toml::from_str::<Settings>(&text) {
        Ok(mut s) => {
            s.sanitize();
            s
        }
        Err(err) => {
            eprintln!(
                "flupmix: bad settings {}: {err}; using defaults",
                path.display()
            );
            let _ = fs::rename(path, path.with_extension("toml.bad"));
            Settings::default()
        }
    }
}

pub fn save(settings: &Settings) -> io::Result<()> {
    save_to(&settings_path(), settings)
}

pub fn save_to(path: &Path, settings: &Settings) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let text = toml::to_string_pretty(settings)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, text)?;
    fs::rename(tmp, path)
}

/// `$XDG_CONFIG_HOME/autostart/flupmix.desktop`.
pub fn autostart_path() -> PathBuf {
    config_path()
        .parent()
        .and_then(Path::parent)
        .map_or_else(|| PathBuf::from("autostart"), |p| p.join("autostart"))
        .join("flupmix.desktop")
}

pub fn autostart_entry(exe: &Path) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=flupmix\nComment=Voicemeeter-style mixer for PipeWire\n\
         Exec=\"{}\"\nIcon=flupmix\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
        exe.display()
    )
}

/// Creates or removes the autostart entry at `path`.
pub fn set_autostart_at(path: &Path, enabled: bool, exe: &Path) -> io::Result<()> {
    if enabled {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(path, autostart_entry(exe))
    } else {
        match fs::remove_file(path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}

pub fn set_autostart(enabled: bool) -> io::Result<()> {
    let exe = std::env::current_exe()?;
    set_autostart_at(&autostart_path(), enabled, &exe)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("flupmix-settings-{tag}-{}", std::process::id()))
    }

    #[test]
    fn round_trips_and_fills_missing_fields() {
        let dir = temp("rt");
        let path = dir.join("settings.toml");
        let mut s = Settings {
            palette: Palette::Mint,
            wheel_step_db: 1.0,
            ..Settings::default()
        };
        s.window_size = Some((1200.0, 700.0));
        save_to(&path, &s).unwrap();
        assert_eq!(load_from(&path), s);
        fs::write(&path, "close_to_tray = false\n").unwrap();
        let partial = load_from(&path);
        assert!(!partial.close_to_tray);
        assert_eq!(partial.meter_hz, 30);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn corrupt_file_falls_back_and_is_kept_aside() {
        let dir = temp("bad");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.toml");
        fs::write(&path, "meter_hz = [").unwrap();
        assert_eq!(load_from(&path), Settings::default());
        assert!(path.with_extension("toml.bad").exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn sanitize_clamps_nonsense() {
        let mut s = Settings {
            meter_hz: 144,
            peak_hold_secs: -3.0,
            wheel_step_db: 99.0,
            window_size: Some((10.0, f32::NAN)),
            ..Settings::default()
        };
        s.sanitize();
        assert_eq!(s.meter_hz, 60);
        assert_eq!(s.peak_hold_secs, 0.0);
        assert_eq!(s.wheel_step_db, 6.0);
        assert_eq!(s.window_size, None);
    }

    #[test]
    fn autostart_entry_is_created_and_removed() {
        let dir = temp("auto");
        let path = dir.join("autostart").join("flupmix.desktop");
        set_autostart_at(&path, true, Path::new("/opt/my apps/flupmix")).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("Exec=\"/opt/my apps/flupmix\""));
        set_autostart_at(&path, false, Path::new("/x")).unwrap();
        assert!(!path.exists());
        set_autostart_at(&path, false, Path::new("/x")).unwrap();
        let _ = fs::remove_dir_all(dir);
    }
}
