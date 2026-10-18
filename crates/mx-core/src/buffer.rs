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

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn len_chars(&self) -> usize {
        self.text.len_chars()
    }

    pub fn point(&self) -> usize {
        self.point
    }

    pub fn contents(&self) -> String {
        self.text.to_string()
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
}
