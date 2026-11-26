//! Cyberdream's high-contrast palette, with cyan routing accents and
//! semantic status colours on black surfaces.

use std::borrow::Cow;

use gpui::App;

pub const BG: u32 = 0x000000;
pub const PANEL: u32 = 0x0b0c0f;
pub const PANEL_RAISED: u32 = 0x16181d;
pub const PANEL_SUNKEN: u32 = 0x050608;
pub const BORDER: u32 = 0x3c4048;
pub const BORDER_STRONG: u32 = 0x6b7280;

pub const TEXT: u32 = 0xffffff;
pub const TEXT_DIM: u32 = 0xb4bcd0;
pub const TEXT_FAINT: u32 = 0x7b8496;

pub const ACCENT: u32 = 0x5ef1ff;
pub const ACCENT_DIM: u32 = 0x2aa5b3;
pub const ACCENT_BG: u32 = 0x0a2d33;
pub const GREEN: u32 = 0x5eff6c;
pub const YELLOW: u32 = 0xf1ff5e;
pub const ORANGE: u32 = 0xffbd5e;
pub const RED: u32 = 0xff6e5e;
pub const MAGENTA: u32 = 0xff5ea0;
pub const PINK: u32 = 0xff5ef1;
pub const PURPLE: u32 = 0xbd5eff;
pub const BLUE: u32 = 0x5ea1ff;

/// Proportional UI font (bundled, IBM Plex Sans, OFL).
pub const FONT_UI: &str = "IBM Plex Sans";
/// Monospace font for numerics and labels (bundled, Lilex, OFL).
pub const FONT_MONO: &str = "Lilex";

/// Registers the bundled fonts. Call once before opening a window.
pub fn load_fonts(cx: &App) {
    let fonts: Vec<Cow<'static, [u8]>> = vec![
        Cow::Borrowed(include_bytes!("../../../assets/fonts/IBMPlexSans-Regular.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../../../assets/fonts/IBMPlexSans-SemiBold.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../../../assets/fonts/Lilex-Regular.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../../../assets/fonts/Lilex-Bold.ttf").as_slice()),
    ];
    if let Err(err) = cx.text_system().add_fonts(fonts) {
        eprintln!("flupmix: failed to load bundled fonts: {err:#}");
    }
}
