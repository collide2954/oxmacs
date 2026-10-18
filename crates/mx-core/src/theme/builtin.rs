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
    completion_current: Face {
        foreground: Color::hex(0xFF_FF_FF),
        background: Color::hex(0x4A_90_D9),
    },
    shadow: Color::hex(0x7F_7F_7F),
    dired_header: Color::hex(0x61_35_83),
    dired_directory: Color::hex(0x1A_5F_B4),
    dired_symlink: Color::hex(0x0B_6E_6E),
    dired_marked: Color::hex(0xF6_E5_A8),
    dired_flagged: Color::hex(0xF6_C9_CC),
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
    completion_current: Face {
        foreground: Color::hex(0xDE_DD_DA),
        background: Color::hex(0x30_30_30),
    },
    shadow: Color::hex(0x65_65_65),
    dired_header: Color::hex(0xDC_8A_DD),
    dired_directory: Color::hex(0x99_C1_F1),
    dired_symlink: Color::hex(0x93_DD_C2),
    dired_marked: Color::hex(0x4D_44_22),
    dired_flagged: Color::hex(0x5C_2A_2E),
};
