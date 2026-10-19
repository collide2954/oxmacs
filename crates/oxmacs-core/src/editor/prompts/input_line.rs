use crate::text::{Buffer, Marker};

#[derive(Debug)]
pub(in crate::editor) struct InputLine {
    mark: Marker,
    history: Vec<String>,
    recall: Option<usize>,
}

impl InputLine {
    pub(in crate::editor) fn new(buffer: &mut Buffer) -> Self {
        let mark = buffer.add_marker(buffer.len_chars());
        Self {
            mark,
            history: Vec::new(),
            recall: None,
        }
    }

    pub(in crate::editor) fn mark(&self) -> Marker {
        self.mark
    }

    pub(in crate::editor) fn start(&self, buffer: &Buffer) -> Option<usize> {
        buffer.marker(self.mark)
    }

    pub(in crate::editor) fn text(&self, buffer: &Buffer) -> Option<String> {
        let start = self.start(buffer)?;
        Some(buffer.text_in(start..buffer.len_chars()))
    }

    pub(in crate::editor) fn remember(&mut self, input: &str) {
        if !input.trim().is_empty() && self.history.last().map(String::as_str) != Some(input) {
            self.history.push(input.to_owned());
        }
        self.recall = None;
    }

    pub(in crate::editor) fn recall(&mut self, older: bool) -> Option<String> {
        if self.history.is_empty() {
            return None;
        }
        let last = self.history.len() - 1;
        let next = match (self.recall, older) {
            (None, true) => Some(last),
            (None, false) => None,
            (Some(index), true) => Some(index.saturating_sub(1)),
            (Some(index), false) => (index < last).then_some(index + 1),
        };
        self.recall = next;
        Some(
            next.and_then(|index| self.history.get(index))
                .cloned()
                .unwrap_or_default(),
        )
    }

    pub(in crate::editor) fn replace(&self, buffer: &mut Buffer, text: &str) {
        let Some(start) = self.start(buffer) else {
            return;
        };
        buffer.delete_range(start..buffer.len_chars());
        buffer.insert_at(start, text);
        buffer.set_point(buffer.len_chars());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_is_whatever_follows_the_mark() {
        let mut buffer = Buffer::with_text("t", "> ");
        let line = InputLine::new(&mut buffer);
        buffer.insert_at(2, "hello");

        assert_eq!(line.text(&buffer).as_deref(), Some("hello"));

        line.replace(&mut buffer, "bye");
        assert_eq!(buffer.contents(), "> bye");
        assert_eq!(buffer.point(), 5);
    }

    #[test]
    fn history_steps_back_and_forth() {
        let mut buffer = Buffer::with_text("t", "");
        let mut line = InputLine::new(&mut buffer);
        assert_eq!(line.recall(true), None);
        for input in ["a", "b", "b", " "] {
            line.remember(input);
        }

        assert_eq!(line.recall(true).as_deref(), Some("b"));
        assert_eq!(line.recall(true).as_deref(), Some("a"));
        assert_eq!(line.recall(true).as_deref(), Some("a"));
        assert_eq!(line.recall(false).as_deref(), Some("b"));
        assert_eq!(line.recall(false).as_deref(), Some(""));
    }
}
