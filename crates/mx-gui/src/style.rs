use iced::theme::Palette;
use iced::widget::container;
use iced::{Border, Theme};
use mx_core::theme::{self, Appearance};

pub fn color(color: theme::Color) -> iced::Color {
    iced::Color::from_rgb8(color.r, color.g, color.b)
}

pub fn iced_theme(theme: &theme::Theme) -> Theme {
    let base = match theme.appearance {
        Appearance::Light => Palette::LIGHT,
        Appearance::Dark => Palette::DARK,
    };
    Theme::custom(
        theme.name,
        Palette {
            background: color(theme.default.background),
            text: color(theme.default.foreground),
            primary: color(theme.minibuffer_prompt),
            ..base
        },
    )
}

pub fn mode_line(theme: &theme::Theme) -> container::Style {
    container::Style {
        background: Some(color(theme.mode_line.background).into()),
        text_color: Some(color(theme.mode_line.foreground)),
        border: Border {
            color: color(theme.mode_line_border),
            width: 1.0,
            radius: 0.0.into(),
        },
        ..container::Style::default()
    }
}
