//! Semantic color roles (UX-STANDARDS §9). Panel code asks for a role such as
//! `surface` or `accent`, never a hex value.
use eframe::egui::Color32;

/// Chrome colors for one theme. `accent` is also used as text, so it must
/// remain readable on every surface; `on_accent` is the primary button label.
#[derive(Debug, PartialEq)]
pub struct Palette {
    pub background: Color32,
    pub surface: Color32,
    pub raised: Color32,
    pub hover: Color32,
    pub field: Color32,
    pub divider: Color32,
    pub control_border: Color32,
    pub text: Color32,
    pub secondary: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub accent_pressed: Color32,
    pub on_accent: Color32,
    pub selection: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub error: Color32,
    pub axis: [Color32; 3],
}

pub(crate) const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

pub const DARK: Palette = Palette {
    background: rgb(0x151314),
    surface: rgb(0x201d1e),
    raised: rgb(0x292527),
    hover: rgb(0x382e30),
    field: rgb(0x191617),
    divider: rgb(0x383032),
    control_border: rgb(0x827574),
    text: rgb(0xf1e8e3),
    secondary: rgb(0xb9adaa),
    accent: rgb(0xef9aa2),
    accent_hover: rgb(0xf9b2b7),
    accent_pressed: rgb(0xdd838c),
    on_accent: rgb(0x151314),
    selection: rgb(0x583439),
    success: rgb(0x9bddab),
    warning: rgb(0xffc078),
    error: rgb(0xff8291),
    axis: [rgb(0xffa2a7), rgb(0x9bddab), rgb(0xa7caff)],
};

/// Dark text and accents on pure-black panels; raised controls stay lifted.
pub const OLED_BLACK: Palette = Palette {
    background: rgb(0x000000),
    surface: rgb(0x000000),
    raised: rgb(0x121820),
    hover: rgb(0x1c242d),
    field: rgb(0x000000),
    divider: rgb(0x222b34),
    control_border: rgb(0x5f7384),
    selection: rgb(0x362127),
    ..DARK
};

/// Light chrome uses graphite text and deep oxide accents.
pub const LIGHT: Palette = Palette {
    background: rgb(0xeee6e1),
    surface: rgb(0xf7efeb),
    raised: rgb(0xffffff),
    hover: rgb(0xece1de),
    field: rgb(0xffffff),
    divider: rgb(0xd9c8c4),
    control_border: rgb(0x7e6967),
    text: rgb(0x151314),
    secondary: rgb(0x634f4d),
    accent: rgb(0x96373e),
    accent_hover: rgb(0xa34249),
    accent_pressed: rgb(0x7d2d34),
    on_accent: rgb(0xffffff),
    selection: rgb(0xf0d9dc),
    success: rgb(0x2f6b3f),
    warning: rgb(0x8a5300),
    error: rgb(0xb3261e),
    axis: [rgb(0xb42318), rgb(0x2b7a0b), rgb(0x1f5fbf)],
};
