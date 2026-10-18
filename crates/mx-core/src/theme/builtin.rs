use super::{Appearance, Color, Face, SyntaxColors, Theme};

#[expect(
    clippy::mistyped_literal_suffixes,
    reason = "0x5E_5C_64 is a color, not a typed literal"
)]
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
    heading: Color::hex(0x61_35_83),
    link: Color::hex(0x1A_5F_B4),
    symlink: Color::hex(0x0B_6E_6E),
    marked: Color::hex(0xF6_E5_A8),
    flagged: Color::hex(0xF6_C9_CC),
    search_match: Color::hex(0xF9_F0_6B),
    search_current: Color::hex(0xFF_A3_48),
    paren_match: Color::hex(0x99_C1_F1),
    success: Color::hex(0x1C_68_49),
    warning: Color::hex(0x9C_5A_00),
    error: Color::hex(0xC0_1C_28),
    syntax: SyntaxColors {
        keyword: Color::hex(0xB3_3F_00),
        string: Color::hex(0x1A_6B_6B),
        comment: Color::hex(0x5E_5C_64),
        doc_comment: Color::hex(0x1C_68_49),
        function: Color::hex(0x1A_5F_B4),
        type_name: Color::hex(0x81_3D_9C),
        constant: Color::hex(0x4E_57_BA),
        macro_name: Color::hex(0xA5_1D_2D),
        attribute: Color::hex(0x86_5E_3C),
        lifetime: Color::hex(0x3B_75_04),
    },
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
    heading: Color::hex(0xDC_8A_DD),
    link: Color::hex(0x99_C1_F1),
    symlink: Color::hex(0x93_DD_C2),
    marked: Color::hex(0x4D_44_22),
    flagged: Color::hex(0x5C_2A_2E),
    search_match: Color::hex(0x5C_54_20),
    search_current: Color::hex(0x9C_3C_00),
    paren_match: Color::hex(0x1A_5F_B4),
    success: Color::hex(0x2E_C2_7E),
    warning: Color::hex(0xF5_C2_11),
    error: Color::hex(0xF6_61_51),
    syntax: SyntaxColors {
        keyword: Color::hex(0xFF_A3_48),
        string: Color::hex(0x5B_C8_AF),
        comment: Color::hex(0x9A_99_96),
        doc_comment: Color::hex(0x26_A2_69),
        function: Color::hex(0x62_A0_EA),
        type_name: Color::hex(0xDC_8A_DD),
        constant: Color::hex(0x7D_8A_C7),
        macro_name: Color::hex(0xF6_61_51),
        attribute: Color::hex(0xB5_83_5A),
        lifetime: Color::hex(0x4E_9A_06),
    },
};
