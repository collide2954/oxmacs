use std::path::PathBuf;

use oxmacs_pdf::Rect;

pub const MIN_SCALE: f32 = 0.25;
pub const MAX_SCALE: f32 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Zoom {
    FitWidth,
    FitPage,
    Scale(f32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Go {
    Page { page: usize, y: Option<f32> },
    Lines(f32),
    Screens(f32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mark {
    pub page: usize,
    pub rect: Rect,
    pub current: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct View<P> {
    pub path: PathBuf,
    pub revision: u64,
    pub zoom: Zoom,
    pub continuous: bool,
    pub dark: bool,
    pub marks: Vec<Mark>,
    pub go: Option<(i64, Go)>,
    pub on_view: Option<P>,
    pub on_select: Option<P>,
    pub on_link: Option<P>,
}

impl<P> View<P> {
    pub fn map<Q>(self, press: &mut impl FnMut(P) -> Q) -> View<Q> {
        let on_view = self.on_view.map(&mut *press);
        let on_select = self.on_select.map(&mut *press);
        let on_link = self.on_link.map(&mut *press);
        View {
            path: self.path,
            revision: self.revision,
            zoom: self.zoom,
            continuous: self.continuous,
            dark: self.dark,
            marks: self.marks,
            go: self.go,
            on_view,
            on_select,
            on_link,
        }
    }
}
