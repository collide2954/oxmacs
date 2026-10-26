use crate::editor::Editor;
use crate::editor::windows::Parked;
use crate::editor::windows::tiling::WindowId;
use crate::text::BufferId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Side {
    pub window: WindowId,
    pub edge: Edge,
    pub columns: usize,
}

impl Editor {
    pub fn side_window(&self) -> Option<Side> {
        self.windows.side
    }

    pub(in crate::editor) fn in_side(&self) -> bool {
        self.windows
            .side
            .is_some_and(|side| side.window == self.windows.selected)
    }

    pub fn show_side(&mut self, buffer: BufferId, edge: Edge, columns: usize) -> WindowId {
        if let Some(side) = self.windows.side {
            self.windows.side = Some(Side {
                edge,
                columns,
                ..side
            });
            if self.pane_of(side.window).map(|pane| pane.buffer().id()) != Some(buffer) {
                let origin = self.windows.selected;
                self.select_window(side.window);
                self.show_buffer(buffer);
                self.select_window(origin);
            }
            return side.window;
        }
        let window = WindowId(self.windows.next);
        self.windows.next += 1;
        let Some(index) = self.buffers.position(|known| known.id() == buffer) else {
            return window;
        };
        let Some(shown) = self.buffers.get_mut(index) else {
            return window;
        };
        let point = shown.point();
        let parked = Parked {
            buffer,
            point: shown.add_marker(point),
            viewport: self.viewport,
        };
        self.windows.parked.push((window, parked));
        self.windows.side = Some(Side {
            window,
            edge,
            columns,
        });
        window
    }

    pub fn resize_side(&mut self, columns: usize) {
        if let Some(side) = &mut self.windows.side {
            side.columns = columns.max(1);
        }
    }

    pub fn close_side(&mut self) {
        let Some(side) = self.windows.side else {
            return;
        };
        if self.in_side() {
            self.select_main_window();
        }
        self.windows.side = None;
        if let Some(at) = self
            .windows
            .parked
            .iter()
            .position(|(id, _)| *id == side.window)
        {
            let (_, parked) = self.windows.parked.remove(at);
            self.release_parked(parked);
        }
    }

    pub fn select_main_window(&mut self) {
        let tiled = self.windows.tiling.windows();
        if tiled.contains(&self.windows.selected) {
            return;
        }
        let target = if tiled.contains(&self.windows.main) {
            self.windows.main
        } else {
            tiled[0]
        };
        self.select_window(target);
    }

    pub fn select_side_window(&mut self) {
        if let Some(side) = self.windows.side {
            self.select_window(side.window);
        }
    }

    pub(in crate::editor) fn keep_side_alive(&mut self) {
        let Some(side) = self.windows.side else {
            return;
        };
        let alive = self
            .windows
            .parked
            .iter()
            .find(|(id, _)| *id == side.window)
            .is_none_or(|(_, parked)| self.buffer_by_id(parked.buffer).is_some());
        if !alive {
            self.close_side();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Edge, Side};
    use crate::editor::{Arrangement, Editor, WindowId};
    use crate::testing::{press, type_text};

    fn with_tree() -> (Editor, WindowId) {
        let mut editor = Editor::new();
        press(&mut editor, "C-x b");
        type_text(&mut editor, "tree");
        press(&mut editor, "RET C-x b RET");
        let tree = editor.buffer_named("tree").unwrap();
        let side = editor.show_side(tree, Edge::Left, 30);
        (editor, side)
    }

    #[test]
    fn the_side_window_shows_its_buffer_beside_the_others_without_taking_the_focus() {
        let (editor, side) = with_tree();

        assert_eq!(
            editor.side_window(),
            Some(Side {
                window: side,
                edge: Edge::Left,
                columns: 30,
            })
        );
        assert_eq!(editor.tiling().windows().len(), 1);
        assert_eq!(editor.pane_of(side).unwrap().buffer().name(), "tree");
        assert_eq!(editor.buffer().name(), "*scratch*");
    }

    #[test]
    fn c_x_o_cycles_the_other_windows_and_leaves_the_side_one() {
        let (mut editor, side) = with_tree();
        press(&mut editor, "C-x 3");
        let [first, second] = editor.tiling().windows()[..] else {
            panic!("two windows");
        };

        press(&mut editor, "C-x o");
        assert_eq!(editor.selected_window(), second);
        press(&mut editor, "C-x o");
        assert_eq!(editor.selected_window(), first);

        press(&mut editor, "C-x o");
        editor.select_window(side);
        press(&mut editor, "C-x o");
        assert_eq!(editor.selected_window(), second);
    }

    #[test]
    fn c_x_1_keeps_the_side_window_and_c_x_0_there_closes_it() {
        let (mut editor, side) = with_tree();
        press(&mut editor, "C-x 3 C-x 1");
        assert_eq!(editor.tiling().windows().len(), 1);
        assert_eq!(editor.pane_of(side).unwrap().buffer().name(), "tree");

        editor.select_window(side);
        press(&mut editor, "C-x 1");
        assert_eq!(
            editor.message(),
            Some("The side window can't be the only window")
        );
        press(&mut editor, "C-x 0");
        assert_eq!(editor.side_window(), None);
        assert_eq!(editor.selected_window(), editor.tiling().windows()[0]);
        assert_eq!(editor.buffer().name(), "*scratch*");
    }

    #[test]
    fn the_side_window_closes_with_its_buffer() {
        let (mut editor, _) = with_tree();

        press(&mut editor, "C-x k");
        type_text(&mut editor, "tree");
        press(&mut editor, "RET");

        assert_eq!(editor.side_window(), None);
    }

    #[test]
    fn layouts_stored_recalled_and_arranged_leave_the_side_window_alone() {
        let (mut editor, side) = with_tree();
        editor.store_windows("one");
        press(&mut editor, "C-x 3");
        editor.select_window(side);

        assert!(editor.recall_windows("one"));
        assert_eq!(editor.tiling().windows().len(), 1);
        assert_eq!(editor.pane_of(side).unwrap().buffer().name(), "tree");

        editor.select_window(side);
        let scratch = editor.buffer_named("*scratch*").unwrap();
        editor.arrange_windows(
            &Arrangement::Split {
                axis: crate::editor::Axis::Columns,
                parts: vec![
                    (0.5, Arrangement::Buffer(scratch)),
                    (0.5, Arrangement::Buffer(scratch)),
                ],
            },
            None,
        );
        assert_eq!(editor.tiling().windows().len(), 2);
        assert_eq!(editor.pane_of(side).unwrap().buffer().name(), "tree");
        assert_eq!(editor.side_window().unwrap().window, side);
    }

    #[test]
    fn windmove_reaches_the_side_window_and_comes_back() {
        let (mut editor, side) = with_tree();
        let main = editor.selected_window();

        press(&mut editor, "C-c <left>");
        assert_eq!(editor.selected_window(), side);
        press(&mut editor, "C-c <right>");
        assert_eq!(editor.selected_window(), main);
    }

    #[test]
    fn showing_another_buffer_on_the_side_reuses_the_window() {
        let (mut editor, side) = with_tree();
        let scratch = editor.buffer_named("*scratch*").unwrap();

        assert_eq!(editor.show_side(scratch, Edge::Right, 20), side);

        assert_eq!(editor.pane_of(side).unwrap().buffer().name(), "*scratch*");
        assert_eq!(editor.side_window().unwrap().edge, Edge::Right);
        editor.resize_side(24);
        assert_eq!(editor.side_window().unwrap().columns, 24);
        editor.select_main_window();
        assert_ne!(editor.selected_window(), side);
    }
}
