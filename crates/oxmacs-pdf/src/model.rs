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
