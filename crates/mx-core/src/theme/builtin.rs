use super::{Appearance, Color, Face, Theme};

pub static ADWAITA: Theme = Theme {
    name: "adwaita",
    appearance: Appearance::Light,
    default: Face {
        foreground: Color::hex(0x2E_34_36),
        background: Color::hex(0xED_ED_ED),
    },
    cursor: Color::hex(0x00_BB_FF),
    region: Color::hex(0xC2_D5_E9),
    mode_line: Face {
        foreground: Color::hex(0x2E_34_36),
        background: Color::hex(0xFF_FF_FF),
    },
    mode_line_inactive: Face {
        foreground: Color::hex(0xC6_C6_C6),
        background: Color::hex(0xFF_FF_FF),
    },
    mode_line_border: Color::hex(0xC6_C6_C6),
    minibuffer_prompt: Color::hex(0x00_84_C8),
};

pub static ADWAITA_DARK: Theme = Theme {
    name: "adwaita-dark",
    appearance: Appearance::Dark,
    default: Face {
        foreground: Color::hex(0xDE_DD_DA),
        background: Color::hex(0x1C_1C_1C),
    },
    cursor: Color::hex(0xDE_DD_DA),
    region: Color::hex(0x45_45_45),
    mode_line: Face {
        foreground: Color::hex(0xDE_DD_DA),
        background: Color::hex(0x30_30_30),
    },
    mode_line_inactive: Face {
        foreground: Color::hex(0x65_65_65),
        background: Color::hex(0x24_24_24),
    },
    mode_line_border: Color::hex(0x45_45_45),
    minibuffer_prompt: Color::hex(0xA5_A5_A5),
};
