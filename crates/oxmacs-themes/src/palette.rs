use oxmacs_core::theme::{Appearance, Color, Face, SyntaxColors, Theme};

const READABLE: f64 = 4.6;
const STEP: f32 = 0.05;

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub name: &'static str,
    pub background: u32,
    pub panel: u32,
    pub inactive: u32,
    pub selection: u32,
    pub foreground: u32,
    pub comment: u32,
    pub red: u32,
    pub orange: u32,
    pub yellow: u32,
    pub green: u32,
    pub cyan: u32,
    pub blue: u32,
    pub purple: u32,
    pub keyword: u32,
    pub string: u32,
    pub function: u32,
    pub type_name: u32,
    pub constant: u32,
}

fn readable(color: Color, against: Color, toward: Color) -> Color {
    let mut amount = 0.0;
    let mut adjusted = color;
    while adjusted.contrast_ratio(against) < READABLE && amount < 1.0 {
        amount += STEP;
        adjusted = color.mix(toward, amount);
    }
    adjusted
}

fn tint(color: Color, text: Color, background: Color) -> Color {
    let mut adjusted = color;
    for step in 1..=20_u8 {
        if text.contrast_ratio(adjusted) >= READABLE {
            break;
        }
        adjusted = color.mix(background, f32::from(step) * STEP);
    }
    adjusted
}

fn pole(background: Color) -> Color {
    if background.relative_luminance() < 0.18 {
        Color::hex(0xFF_FF_FF)
    } else {
        Color::hex(0x00_00_00)
    }
}

impl Palette {
    pub fn theme(&self) -> Theme {
        let background = Color::hex(self.background);
        let panel = Color::hex(self.panel);
        let foreground = readable(Color::hex(self.foreground), background, pole(background));
        let text = |color: u32| readable(Color::hex(color), background, foreground);
        let comment = text(self.comment);
        let appearance = if background.relative_luminance() < foreground.relative_luminance() {
            Appearance::Dark
        } else {
            Appearance::Light
        };
        let shade = |color: u32, amount: f32| {
            tint(
                background.mix(Color::hex(color), amount),
                foreground,
                background,
            )
        };
        let region = tint(Color::hex(self.selection), foreground, background);
        let search_match = shade(self.yellow, 0.4);
        let mut search_current = shade(self.orange, 0.6);
        if search_current == search_match {
            search_current = shade(self.red, 0.5);
        }
        Theme {
            name: self.name,
            appearance,
            default: Face {
                foreground,
                background,
            },
            cursor: match appearance {
                Appearance::Dark => foreground,
                Appearance::Light => text(self.blue),
            },
            region,
            mode_line: Face {
                foreground: readable(foreground, panel, pole(panel)),
                background: panel,
            },
            mode_line_inactive: Face {
                foreground: Color::hex(self.comment),
                background: Color::hex(self.inactive),
            },
            mode_line_border: panel.mix(foreground, 0.15),
            minibuffer_prompt: text(self.blue),
            completion_current: Face {
                foreground,
                background: region,
            },
            shadow: comment,
            heading: text(self.purple),
            link: text(self.blue),
            symlink: text(self.cyan),
            marked: shade(self.yellow, 0.22),
            flagged: shade(self.red, 0.22),
            search_match,
            search_current,
            paren_match: shade(self.blue, 0.35),
            success: text(self.green),
            warning: text(self.yellow),
            error: text(self.red),
            syntax: SyntaxColors {
                keyword: text(self.keyword),
                string: text(self.string),
                comment,
                doc_comment: readable(
                    comment.mix(Color::hex(self.string), 0.35),
                    background,
                    foreground,
                ),
                function: text(self.function),
                type_name: text(self.type_name),
                constant: text(self.constant),
                macro_name: text(self.cyan),
                attribute: text(self.orange),
                lifetime: text(self.red),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn faint_colors_are_pulled_toward_the_text_until_readable() {
        let background = Color::hex(0x28_28_28);
        let faint = Color::hex(0x50_50_50);

        let fixed = readable(faint, background, Color::hex(0xEB_DB_B2));

        assert!(fixed.contrast_ratio(background) >= READABLE);
        assert_ne!(fixed, faint);
        assert_eq!(
            readable(Color::hex(0xEB_DB_B2), background, faint),
            Color::hex(0xEB_DB_B2)
        );
    }

    #[test]
    fn strong_tints_fade_toward_the_background_until_text_reads_on_them() {
        let background = Color::hex(0xFB_F1_C7);
        let text = Color::hex(0x3C_38_36);
        let strong = Color::hex(0x5C_30_10);

        let fixed = tint(strong, text, background);

        assert!(text.contrast_ratio(fixed) >= READABLE);
        assert_ne!(fixed, background);
    }
}
