//! flup·trade-derived palette: near-black green surfaces, mint accent,
//! monospace numerics, hairline borders. Contrast is deliberately higher than
//! the reference mock-up.

use std::borrow::Cow;

use gpui::App;

pub const BG: u32 = 0x060a08;
pub const PANEL: u32 = 0x0d1411;
pub const PANEL_RAISED: u32 = 0x131d19;
pub const PANEL_SUNKEN: u32 = 0x08100d;
pub const BORDER: u32 = 0x2b3d35;
pub const BORDER_STRONG: u32 = 0x45604f;

pub const TEXT: u32 = 0xf4faf6;
pub const TEXT_DIM: u32 = 0xa4b8ad;
pub const TEXT_FAINT: u32 = 0x6f8579;

pub const MINT: u32 = 0x3ceba9;
pub const MINT_DIM: u32 = 0x1f9a6e;
/// Tinted fill behind mint outlines and active chips.
pub const MINT_BG: u32 = 0x0f3a2a;
pub const AMBER: u32 = 0xf5b53a;
pub const RED: u32 = 0xff4f60;

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
