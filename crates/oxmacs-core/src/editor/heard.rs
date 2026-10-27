use crate::editor::buffers::Mode;
use crate::editor::{Cell, Clicks, Editor};
use crate::input::{Key, KeyCode};
use crate::special_buffer::SpecialBuffer;
use crate::text::BufferId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Heard {
    Clicked {
        buffer: BufferId,
        line: usize,
        clicks: usize,
    },
    Dropped {
        buffer: BufferId,
        from: usize,
        to: usize,
    },
    Typed {
        buffer: BufferId,
        text: String,
    },
}

impl Editor {
    pub fn take_heard(&mut self) -> Vec<Heard> {
        std::mem::take(&mut self.heard)
    }

    fn hears(&self, wants: fn(&dyn SpecialBuffer) -> bool) -> bool {
        matches!(self.buffers.mode(0), Some(Mode::Special(view)) if wants(view.as_ref()))
    }

    pub(super) fn hear_click(&mut self, position: usize, clicks: Clicks, extend: bool) -> bool {
        if extend || !self.hears(SpecialBuffer::hears_clicks) {
            return false;
        }
        let buffer = self.buffers.current_mut();
        buffer.deactivate_mark();
        buffer.set_point(position);
        let line = buffer.line_of(position);
        let id = buffer.id();
        self.drag_anchor = None;
        self.line_drag = (clicks == Clicks::Single).then_some((id, line));
        self.heard.push(Heard::Clicked {
            buffer: id,
            line,
            clicks: match clicks {
                Clicks::Single => 1,
                Clicks::Double => 2,
                Clicks::Triple => 3,
            },
        });
        true
    }

    pub(super) fn hear_drag(&mut self, position: usize) -> bool {
        let current = self.buffers.current().id();
        if self.line_drag.is_some_and(|(id, _)| id == current) {
            self.buffers.current_mut().set_point(position);
            return true;
        }
        false
    }

    pub fn release(&mut self, cell: Cell) {
        let Some((id, from)) = self.line_drag.take() else {
            return;
        };
        if id != self.buffers.current().id() {
            return;
        }
        let to = self.buffers.current().line_of(self.position_at(cell));
        if to != from {
            self.heard.push(Heard::Dropped {
                buffer: id,
                from,
                to,
            });
        }
    }

    pub(super) fn hear_typing(&mut self) -> bool {
        let Some(KeyCode::Char(typed)) = self.last_key.map(Key::code) else {
            return false;
        };
        if self.minibuffer.is_some() || !self.hears(SpecialBuffer::hears_typing) {
            return false;
        }
        let buffer = self.buffers.current().id();
        self.heard.push(Heard::Typed {
            buffer,
            text: typed.to_string(),
        });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::Heard;
    use crate::command::ScopeId;
    use crate::editor::{Cell, Clicks, Editor};
    use crate::special_buffer::SpecialBuffer;
    use crate::testing::press;
    use crate::text::BufferId;

    #[derive(Debug)]
    struct Listening(ScopeId);

    impl SpecialBuffer for Listening {
        fn scope(&self) -> ScopeId {
            self.0
        }

        fn hears_clicks(&self) -> bool {
            true
        }

        fn hears_typing(&self) -> bool {
            true
        }
    }

    fn listening() -> (Editor, BufferId) {
        let mut editor = Editor::new();
        let scope = editor.add_scope("listening");
        let id = editor.add_special_buffer_with("*list*", |buffer| {
            buffer.set_read_only(true);
            Listening(scope)
        });
        editor.fill_buffer(id, "a\nb\nc");
        editor.show_buffer(id);
        editor.resize_window(10, 20);
        (editor, id)
    }

    fn at(row: usize) -> Cell {
        Cell { row, column: 0 }
    }

    #[test]
    fn clicks_in_buffers_that_hear_them_come_back_by_line() {
        let (mut editor, id) = listening();

        editor.click(at(1), Clicks::Single, false);
        editor.click(at(1), Clicks::Double, false);

        assert_eq!(
            editor.take_heard(),
            [
                Heard::Clicked {
                    buffer: id,
                    line: 1,
                    clicks: 1
                },
                Heard::Clicked {
                    buffer: id,
                    line: 1,
                    clicks: 2
                },
            ]
        );
        assert_eq!(editor.buffer().line_of(editor.buffer().point()), 1);
        assert_eq!(editor.buffer().selection(), None);
    }

    #[test]
    fn dragging_from_line_to_line_comes_back_once_released() {
        let (mut editor, id) = listening();

        editor.click(at(0), Clicks::Single, false);
        editor.drag(at(2));
        assert_eq!(editor.buffer().selection(), None);
        editor.release(at(2));
        editor.click(at(1), Clicks::Single, false);
        editor.release(at(1));

        let heard = editor.take_heard();
        assert_eq!(
            heard[1],
            Heard::Dropped {
                buffer: id,
                from: 0,
                to: 2
            }
        );
        assert_eq!(heard.len(), 3);
    }

    #[test]
    fn typing_in_buffers_that_hear_it_comes_back_instead_of_being_refused() {
        let (mut editor, id) = listening();

        press(&mut editor, "a SPC");

        assert_eq!(
            editor.take_heard(),
            [
                Heard::Typed {
                    buffer: id,
                    text: "a".to_owned()
                },
                Heard::Typed {
                    buffer: id,
                    text: " ".to_owned()
                },
            ]
        );
        assert_eq!(editor.buffer().contents(), "a\nb\nc");
        assert_eq!(editor.message(), None);
    }

    #[test]
    fn other_buffers_select_and_type_as_before() {
        let mut editor = Editor::new();
        press(&mut editor, "a b");
        editor.resize_window(10, 20);

        editor.click(at(0), Clicks::Single, false);
        editor.drag(Cell { row: 0, column: 1 });
        editor.release(Cell { row: 0, column: 1 });

        assert_eq!(editor.buffer().selection(), Some(0..1));
        assert_eq!(editor.take_heard(), []);
    }
}
