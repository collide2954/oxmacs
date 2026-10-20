use oxmacs_gui::applet::GuiApplet;

pub fn all() -> Vec<Box<dyn GuiApplet>> {
    vec![
        #[cfg(feature = "animated-splash")]
        Box::new(oxmacs_splash::Splash::default()),
        #[cfg(feature = "demo")]
        Box::new(oxmacs_demo::Demo),
        #[cfg(feature = "extra-themes")]
        Box::new(oxmacs_themes::Themes),
    ]
}
