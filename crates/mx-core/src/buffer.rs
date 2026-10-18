use ropey::Rope;

#[derive(Debug, Clone)]
pub struct Buffer {
    name: String,
    text: Rope,
    point: usize,
}

impl Buffer {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            text: Rope::new(),
            point: 0,
        }
    }

    pub fn with_text(name: impl Into<String>, text: &str) -> Self {
        Self {
            text: Rope::from_str(text),
            ..Self::new(name)
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn len_chars(&self) -> usize {
        self.text.len_chars()
    }

    pub fn point(&self) -> usize {
        self.point
    }

    pub fn set_point(&mut self, point: usize) {
        self.point = point.min(self.len_chars());
    }

    pub fn contents(&self) -> String {
        self.text.to_string()
    }

    pub fn insert(&mut self, text: &str) {
        self.text.insert(self.point, text);
        self.point += text.chars().count();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_buffer_is_empty_with_point_at_start() {
        let buffer = Buffer::new("*scratch*");

        assert_eq!(buffer.name(), "*scratch*");
        assert_eq!(buffer.len_chars(), 0);
        assert_eq!(buffer.point(), 0);
        assert_eq!(buffer.contents(), "");
    }

    #[test]
    fn insert_places_text_at_point_and_advances_point() {
        let mut buffer = Buffer::new("t");

        buffer.insert("hello");
        buffer.insert(", world");

        assert_eq!(buffer.contents(), "hello, world");
        assert_eq!(buffer.point(), 12);
    }

    #[test]
    fn insert_counts_point_in_chars_not_bytes() {
        let mut buffer = Buffer::new("t");

        buffer.insert("héllo→");

        assert_eq!(buffer.point(), 6);
    }

    #[test]
    fn with_text_starts_with_point_at_beginning() {
        let buffer = Buffer::with_text("t", "abc");

        assert_eq!(buffer.contents(), "abc");
        assert_eq!(buffer.point(), 0);
    }

    #[test]
    fn set_point_clamps_to_end_of_buffer() {
        let mut buffer = Buffer::with_text("t", "abc");

        buffer.set_point(2);
        assert_eq!(buffer.point(), 2);

        buffer.set_point(99);
        assert_eq!(buffer.point(), 3);
    }

    #[test]
    fn insert_in_middle_keeps_surrounding_text() {
        let mut buffer = Buffer::with_text("t", "ac");
        buffer.set_point(1);

        buffer.insert("b");

        assert_eq!(buffer.contents(), "abc");
        assert_eq!(buffer.point(), 2);
    }
}
