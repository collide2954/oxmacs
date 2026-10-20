use crate::layout::Size;

#[derive(Debug, Clone, PartialEq)]
pub struct Heading {
    pub title: String,
    pub page: usize,
    pub depth: usize,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Document {
    pub pages: Vec<Size>,
    pub outline: Vec<Heading>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Rect {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        (self.left..=self.right).contains(&x) && (self.top..=self.bottom).contains(&y)
    }

    pub(crate) fn distance(&self, x: f32, y: f32) -> f32 {
        let across = (self.left - x).max(x - self.right).max(0.0);
        let down = (self.top - y).max(y - self.bottom).max(0.0);
        across * across + down * down
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.right <= self.left || self.bottom <= self.top
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    Page(usize),
    Uri(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub rect: Rect,
    pub target: Target,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PageText {
    pub text: String,
    pub boxes: Vec<Rect>,
    pub links: Vec<Link>,
}
