//! Runtime-selectable application palettes with semantic status colours.

use std::{
    borrow::Cow,
    sync::atomic::{AtomicU8, Ordering},
};

use flupmix_core::settings::Palette;
use gpui::App;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Colors {
    pub(crate) bg: u32,
    pub(crate) panel: u32,
    pub(crate) panel_raised: u32,
    pub(crate) panel_sunken: u32,
    pub(crate) border: u32,
    pub(crate) border_strong: u32,
    pub(crate) text: u32,
    pub(crate) text_dim: u32,
    pub(crate) text_faint: u32,
    pub(crate) accent: u32,
    pub(crate) accent_dim: u32,
    pub(crate) accent_bg: u32,
    pub(crate) green: u32,
    pub(crate) yellow: u32,
    pub(crate) orange: u32,
    pub(crate) red: u32,
    pub(crate) magenta: u32,
    pub(crate) pink: u32,
    pub(crate) purple: u32,
    pub(crate) blue: u32,
}

const CYBERDREAM_HIGH_CONTRAST: Colors = Colors {
    bg: 0x000000,
    panel: 0x0b0c0f,
    panel_raised: 0x16181d,
    panel_sunken: 0x050608,
    border: 0x3c4048,
    border_strong: 0x6b7280,
    text: 0xffffff,
    text_dim: 0xb4bcd0,
    text_faint: 0x7b8496,
    accent: 0x5ef1ff,
    accent_dim: 0x2aa5b3,
    accent_bg: 0x0a2d33,
    green: 0x5eff6c,
    yellow: 0xf1ff5e,
    orange: 0xffbd5e,
    red: 0xff6e5e,
    magenta: 0xff5ea0,
    pink: 0xff5ef1,
    purple: 0xbd5eff,
    blue: 0x5ea1ff,
};

static PALETTES: [Colors; 3] = [
    CYBERDREAM_HIGH_CONTRAST,
    Colors {
        bg: 0x16181a,
        panel: 0x1e2124,
        panel_raised: 0x2a2e33,
        panel_sunken: 0x101214,
        border: 0x3c4048,
        border_strong: 0x5a606c,
        ..CYBERDREAM_HIGH_CONTRAST
    },
    Colors {
        bg: 0x060a08,
        panel: 0x0d1411,
        panel_raised: 0x131d19,
        panel_sunken: 0x08100d,
        border: 0x2b3d35,
        border_strong: 0x45604f,
        text: 0xf4faf6,
        text_dim: 0xa4b8ad,
        text_faint: 0x6f8579,
        accent: 0x3ceba9,
        accent_dim: 0x1f9a6e,
        accent_bg: 0x0f3a2a,
        orange: 0xf5b53a,
        red: 0xff4f60,
        green: 0x3ceba9,
        ..CYBERDREAM_HIGH_CONTRAST
    },
];

static ACTIVE_PALETTE: AtomicU8 = AtomicU8::new(0);

const fn palette_index(palette: Palette) -> usize {
    match palette {
        Palette::CyberdreamHighContrast => 0,
        Palette::Cyberdream => 1,
        Palette::Mint => 2,
    }
}

/// Reads the current palette without allocating or locking.
pub(crate) fn colors() -> &'static Colors {
    &PALETTES[ACTIVE_PALETTE.load(Ordering::Relaxed) as usize]
}

/// Selects a palette; the caller refreshes windows after a live change.
pub(crate) fn set_palette(palette: Palette) {
    // Only the index is shared; every palette is immutable and already initialized.
    ACTIVE_PALETTE.store(palette_index(palette) as u8, Ordering::Relaxed);
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cyberdream_only_overrides_surfaces_and_borders() {
        let colors = PALETTES[palette_index(Palette::Cyberdream)];
        assert_eq!(
            [
                colors.bg,
                colors.panel,
                colors.panel_raised,
                colors.panel_sunken,
                colors.border,
                colors.border_strong,
            ],
            [0x16181a, 0x1e2124, 0x2a2e33, 0x101214, 0x3c4048, 0x5a606c]
        );
        assert_eq!(
            Colors {
                bg: CYBERDREAM_HIGH_CONTRAST.bg,
                panel: CYBERDREAM_HIGH_CONTRAST.panel,
                panel_raised: CYBERDREAM_HIGH_CONTRAST.panel_raised,
                panel_sunken: CYBERDREAM_HIGH_CONTRAST.panel_sunken,
                border: CYBERDREAM_HIGH_CONTRAST.border,
                border_strong: CYBERDREAM_HIGH_CONTRAST.border_strong,
                ..colors
            },
            CYBERDREAM_HIGH_CONTRAST
        );
    }

    #[test]
    fn mint_overrides_surfaces_text_and_semantic_accents() {
        let colors = PALETTES[palette_index(Palette::Mint)];
        assert_eq!(
            [
                colors.bg,
                colors.panel,
                colors.panel_raised,
                colors.panel_sunken,
                colors.border,
                colors.border_strong,
                colors.text,
                colors.text_dim,
                colors.text_faint,
                colors.accent,
                colors.accent_dim,
                colors.accent_bg,
                colors.orange,
                colors.red,
                colors.green,
            ],
            [
                0x060a08, 0x0d1411, 0x131d19, 0x08100d, 0x2b3d35, 0x45604f, 0xf4faf6, 0xa4b8ad,
                0x6f8579, 0x3ceba9, 0x1f9a6e, 0x0f3a2a, 0xf5b53a, 0xff4f60, 0x3ceba9,
            ]
        );
        assert_eq!(
            [
                colors.yellow,
                colors.magenta,
                colors.pink,
                colors.purple,
                colors.blue
            ],
            [
                CYBERDREAM_HIGH_CONTRAST.yellow,
                CYBERDREAM_HIGH_CONTRAST.magenta,
                CYBERDREAM_HIGH_CONTRAST.pink,
                CYBERDREAM_HIGH_CONTRAST.purple,
                CYBERDREAM_HIGH_CONTRAST.blue,
            ]
        );
    }
}
