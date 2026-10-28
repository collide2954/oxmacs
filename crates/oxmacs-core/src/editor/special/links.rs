use std::ops::Range;

use crate::editor::buffers::Mode;
use crate::editor::{Editor, Event};

impl Editor {
    fn links_on(&self, line: usize) -> Vec<Range<usize>> {
        match self.buffers.mode(0) {
            Some(Mode::Special(view)) => view.links(line),
            _ => Vec::new(),
        }
    }

    fn link_at(&self, position: usize, whole_line: bool) -> Option<(usize, usize)> {
        let buffer = self.buffers.current();
        let line = buffer.line_of(position);
        let column = position - buffer.line_start(line);
        let links = self.links_on(line);
        links
            .iter()
            .position(|link| link.contains(&column))
            .or_else(|| (whole_line && !links.is_empty()).then_some(0))
            .map(|index| (line, index))
    }

    pub(in crate::editor) fn follow_link(&mut self) {
        match self.link_at(self.buffers.current().point(), true) {
            Some((line, index)) => self.record(Event::Link {
                buffer: self.buffers.current().id(),
                line,
                link: index,
            }),
            None => self.show_message("No link here"),
        }
    }

    pub(in crate::editor) fn follow_link_at(&mut self, position: usize) -> bool {
        let Some((line, index)) = self.link_at(position, false) else {
            return false;
        };
        let buffer = self.buffers.current_mut();
        buffer.deactivate_mark();
        buffer.set_point(position);
        let id = buffer.id();
        self.record(Event::Link {
            buffer: id,
            line,
            link: index,
        });
        true
    }

    pub(in crate::editor) fn step_link(&mut self, forward: bool) {
        let buffer = self.buffers.current();
        let starts: Vec<usize> = (0..buffer.len_lines())
            .flat_map(|line| {
                let start = buffer.line_start(line);
                self.links_on(line)
                    .into_iter()
                    .map(move |link| start + link.start)
            })
            .collect();
        let point = buffer.point();
        let target = if forward {
            starts
                .iter()
                .find(|start| **start > point)
                .or(starts.first())
        } else {
            starts
                .iter()
                .rev()
                .find(|start| **start < point)
                .or(starts.last())
        };
        match target.copied() {
            Some(start) => {
                self.buffers.current_mut().set_point(start);
                self.reveal_point();
            }
            None => self.show_message("No links here"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Range;

    use crate::command::{Command, ScopeId};
    use crate::editor::{Cell, Clicks, Editor, Event, EventKind};
    use crate::special_buffer::SpecialBuffer;
    use crate::text::BufferId;

    #[derive(Debug)]
    struct Linked(ScopeId);

    impl SpecialBuffer for Linked {
        fn scope(&self) -> ScopeId {
            self.0
        }

        fn links(&self, line: usize) -> Vec<Range<usize>> {
            match line {
                0 => vec![4..5, 10..11],
                2 => std::iter::once(0..1).collect(),
                _ => Vec::new(),
            }
        }
    }

    fn linked() -> (Editor, BufferId) {
        let mut editor = Editor::new();
        editor.subscribe(EventKind::Link);
        let scope = editor.add_scope("linked");
        let id = editor.add_special_buffer_with("*links*", |buffer| {
            buffer.set_read_only(true);
            Linked(scope)
        });
        editor.fill_buffer(id, "see a and b\nnone here\nc");
        editor.show_buffer(id);
        editor.resize_window(10, 20);
        (editor, id)
    }

    fn point(editor: &Editor) -> usize {
        editor.buffer().point()
    }

    #[test]
    fn links_are_stepped_through_around_the_buffer() {
        let (mut editor, _) = linked();

        let mut visited = Vec::new();
        for _ in 0..4 {
            editor.execute(Command::NextLink);
            visited.push(point(&editor));
        }
        assert_eq!(visited, [4, 10, 22, 4]);

        editor.execute(Command::PreviousLink);
        assert_eq!(point(&editor), 22);
        editor.execute(Command::PreviousLink);
        assert_eq!(point(&editor), 10);
    }

    #[test]
    fn following_a_link_asks_its_buffer_for_the_one_at_point_or_first_on_the_line() {
        let (mut editor, id) = linked();

        editor.buffer_mut().set_point(10);
        editor.execute(Command::FollowLink);
        editor.buffer_mut().set_point(7);
        editor.execute(Command::FollowLink);
        editor.buffer_mut().set_point(13);
        editor.execute(Command::FollowLink);

        assert_eq!(
            editor.take_events(),
            [
                Event::Link {
                    buffer: id,
                    line: 0,
                    link: 1
                },
                Event::Link {
                    buffer: id,
                    line: 0,
                    link: 0
                },
            ]
        );
        assert_eq!(editor.message(), Some("No link here"));
    }

    #[test]
    fn clicking_a_link_follows_it_and_elsewhere_moves_point() {
        let (mut editor, id) = linked();

        editor.click(Cell { row: 0, column: 4 }, Clicks::Single, false);
        assert_eq!(
            editor.take_events(),
            [Event::Link {
                buffer: id,
                line: 0,
                link: 0
            }]
        );
        assert_eq!(point(&editor), 4);

        editor.click(Cell { row: 0, column: 7 }, Clicks::Single, false);
        assert_eq!(editor.take_events(), []);
        assert_eq!(point(&editor), 7);
    }

    #[test]
    fn buffers_without_links_say_so() {
        let mut editor = Editor::new();

        editor.execute(Command::NextLink);

        assert_eq!(editor.message(), Some("No links here"));
    }
}
