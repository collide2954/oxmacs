use oxmacs_gui::applet::GuiApplet;

pub fn all() -> Vec<Box<dyn GuiApplet>> {
    vec![
        #[cfg(feature = "dired")]
        Box::new(oxmacs_dired::Dired),
        #[cfg(feature = "markdown")]
        Box::new(oxmacs_markdown::Markdown::default()),
        #[cfg(feature = "weather")]
        Box::new(oxmacs_weather::Weather),
        #[cfg(feature = "animated-splash")]
        Box::new(oxmacs_splash::Splash::default()),
        #[cfg(feature = "demo")]
        Box::new(oxmacs_demo::Demo),
        #[cfg(feature = "extra-themes")]
        Box::new(oxmacs_themes::Themes),
        #[cfg(feature = "glow")]
        Box::new(oxmacs_glow::Glow::default()),
        #[cfg(feature = "irc")]
        Box::new(oxmacs_irc::Irc),
        #[cfg(feature = "atlas")]
        Box::new(oxmacs_atlas::Atlas),
        #[cfg(feature = "pdf")]
        Box::new(oxmacs_pdf::Pdf::default()),
        #[cfg(feature = "git")]
        Box::new(oxmacs_git::Git),
        #[cfg(feature = "database")]
        Box::new(oxmacs_database::Database::default()),
    ]
}
