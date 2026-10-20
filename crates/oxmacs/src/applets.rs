use oxmacs_gui::applet::GuiApplet;

pub fn all() -> Vec<Box<dyn GuiApplet>> {
    vec![
        #[cfg(feature = "demo")]
        Box::new(oxmacs_demo::Demo),
    ]
}
