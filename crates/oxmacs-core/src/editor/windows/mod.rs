pub(super) mod numbers;
pub(super) mod pane;
pub(super) mod status;
pub(super) mod tiling;

use crate::display::{Layout, Viewport};
use crate::editor::Editor;
use crate::editor::windows::pane::Pane;
use crate::editor::windows::tiling::{Axis, Tiling, WindowId};
use crate::text::{Buffer, BufferId, Marker};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Direction {
    Left,
    Right,
    Up,
    Down,
}

impl Direction {
    fn name(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Up => "above",
            Self::Down => "below",
        }
    }
}

#[expect(
    clippy::cast_precision_loss,
    reason = "line counts are far below the f32 precision limit"
)]
fn signed_as_f32(value: isize) -> f32 {
    value as f32
}

#[expect(
    clippy::cast_precision_loss,
    reason = "window sizes are far below the f32 precision limit"
)]
fn unsigned_as_f32(value: usize) -> f32 {
    value as f32
}

#[derive(Debug, Clone, Copy)]
struct Parked {
    buffer: BufferId,
    point: Marker,
    viewport: Viewport,
}

#[derive(Debug)]
pub(super) struct Windows {
    tiling: Tiling,
    selected: WindowId,
    parked: Vec<(WindowId, Parked)>,
    next: u64,
}

impl Windows {
    pub(super) fn new() -> Self {
        Self {
            tiling: Tiling::Window(WindowId(0)),
            selected: WindowId(0),
            parked: Vec::new(),
            next: 1,
        }
    }
}

impl Editor {
    pub fn tiling(&self) -> &Tiling {
        &self.windows.tiling
    }

    pub fn selected_window(&self) -> WindowId {
        self.windows.selected
    }

    pub fn pane_of(&self, window: WindowId) -> Option<Pane<'_>> {
        if window == self.windows.selected {
            return Some(self.pane());
        }
        let (_, parked) = self.windows.parked.iter().find(|(id, _)| *id == window)?;
        let index = self
            .buffers
            .position(|buffer| buffer.id() == parked.buffer)?;
        let point = self.buffers.get(index)?.marker(parked.point).unwrap_or(0);
        Some(Pane::new(self, index, parked.viewport, point, false))
    }

    pub(super) fn beside(&mut self, place: impl FnOnce(&mut Self)) {
        let origin = self.windows.selected;
        if self.windows.tiling.windows().len() == 1 {
            self.split_window(Axis::Columns);
        }
        self.other_window();
        place(self);
        self.select_window(origin);
    }

    pub(super) fn show_beside(&mut self, find: impl Fn(&Self) -> Option<usize>) {
        let Some(id) = find(self)
            .and_then(|index| self.buffers.get(index))
            .map(Buffer::id)
        else {
            return;
        };
        if self.window_showing(id).is_none() {
            self.beside(|editor| {
                if let Some(index) = find(editor) {
                    editor.select_buffer(index);
                }
            });
        }
        if let Some(window) = self.window_showing(id) {
            self.select_window(window);
        }
    }

    pub(super) fn follow_end(&mut self, buffer: BufferId, old_end: usize) {
        self.move_windows(buffer, |point, end| (point == old_end).then_some(end));
    }

    pub(super) fn point_windows_at(&mut self, buffer: BufferId, position: usize) {
        self.move_windows(buffer, |_, _| Some(position));
    }

    pub(super) fn move_windows(
        &mut self,
        buffer: BufferId,
        to: impl Fn(usize, usize) -> Option<usize>,
    ) {
        let current = self.buffers.current_mut();
        if current.id() == buffer
            && let Some(point) = to(current.point(), current.len_chars())
        {
            current.set_point(point);
            self.reveal_point();
        }
        let tab_width = self.tab_width;
        let Some(index) = self.buffers.position(|text| text.id() == buffer) else {
            return;
        };
        for (_, parked) in &mut self.windows.parked {
            let Some(text) = self
                .buffers
                .get_mut(index)
                .filter(|_| parked.buffer == buffer)
            else {
                continue;
            };
            let Some(point) = text
                .marker(parked.point)
                .and_then(|point| to(point, text.len_chars()))
            else {
                continue;
            };
            text.set_marker(parked.point, point);
            let layout = Layout::new(text, parked.viewport.wrap_width(), tab_width);
            parked.viewport.reveal(layout.screen_line(point), &layout);
        }
    }

    pub(super) fn window_showing(&self, buffer: BufferId) -> Option<WindowId> {
        if self.buffers.current().id() == buffer {
            return Some(self.windows.selected);
        }
        self.windows
            .parked
            .iter()
            .find(|(_, parked)| parked.buffer == buffer)
            .map(|(id, _)| *id)
    }

    pub(super) fn visible_buffers(&self) -> Vec<usize> {
        self.windows
            .parked
            .iter()
            .filter_map(|(_, parked)| self.buffers.position(|buffer| buffer.id() == parked.buffer))
            .collect()
    }

    pub(super) fn split_window(&mut self, axis: Axis) {
        let id = WindowId(self.windows.next);
        if !self.windows.tiling.split(self.windows.selected, axis, id) {
            return;
        }
        self.windows.next += 1;
        let parked = self.park();
        self.windows.parked.push((id, parked));
    }

    fn park(&mut self) -> Parked {
        let buffer = self.buffers.current_mut();
        let point = buffer.point();
        Parked {
            buffer: buffer.id(),
            point: buffer.add_marker(point),
            viewport: self.viewport,
        }
    }

    pub fn select_window(&mut self, window: WindowId) {
        let Some(position) = self.windows.parked.iter().position(|(id, _)| *id == window) else {
            return;
        };
        let (_, target) = self.windows.parked.remove(position);
        let current = self.park();
        self.windows.parked.push((self.windows.selected, current));
        self.windows.selected = window;
        self.show(target);
    }

    fn show(&mut self, parked: Parked) {
        if let Some(index) = self.buffers.position(|buffer| buffer.id() == parked.buffer) {
            self.buffers.set_top(self.viewport.top());
            self.buffers.select(index);
        }
        let buffer = self.buffers.current_mut();
        if let Some(point) = buffer.marker(parked.point) {
            buffer.set_point(point);
        }
        buffer.remove_marker(parked.point);
        self.viewport = parked.viewport;
        self.goal_column = None;
        self.shift_selection = false;
        self.last_command = None;
        self.drag_anchor = None;
        self.reveal_point();
    }

    pub(super) fn other_window(&mut self) {
        let windows = self.windows.tiling.windows();
        let Some(index) = windows.iter().position(|id| *id == self.windows.selected) else {
            return;
        };
        let next = windows[(index + 1) % windows.len()];
        self.select_window(next);
    }

    pub(super) fn delete_window(&mut self) {
        let Some(next) = self.windows.tiling.remove(self.windows.selected) else {
            self.show_message("Attempt to delete minibuffer or sole ordinary window");
            return;
        };
        let Some(position) = self.windows.parked.iter().position(|(id, _)| *id == next) else {
            return;
        };
        let (_, target) = self.windows.parked.remove(position);
        self.windows.selected = next;
        self.show(target);
    }

    pub(super) fn delete_other_windows(&mut self) {
        for (_, parked) in self.windows.parked.drain(..) {
            if let Some(index) = self.buffers.position(|buffer| buffer.id() == parked.buffer)
                && let Some(buffer) = self.buffers.get_mut(index)
            {
                buffer.remove_marker(parked.point);
            }
        }
        self.windows.tiling = Tiling::Window(self.windows.selected);
    }

    pub(super) fn windmove(&mut self, direction: Direction) {
        let rects = self.windows.tiling.rects();
        let Some((_, from)) = rects.iter().find(|(id, _)| *id == self.windows.selected) else {
            return;
        };
        let overlap = |start: f32, length: f32, other_start: f32, other_length: f32| {
            (start + length).min(other_start + other_length) - start.max(other_start)
        };
        let touches = |a: f32, b: f32| (a - b).abs() < 1e-4;
        let target = rects
            .iter()
            .filter_map(|(id, to)| {
                let (adjacent, shared) = match direction {
                    Direction::Left => (
                        touches(to.x + to.width, from.x),
                        overlap(from.y, from.height, to.y, to.height),
                    ),
                    Direction::Right => (
                        touches(from.x + from.width, to.x),
                        overlap(from.y, from.height, to.y, to.height),
                    ),
                    Direction::Up => (
                        touches(to.y + to.height, from.y),
                        overlap(from.x, from.width, to.x, to.width),
                    ),
                    Direction::Down => (
                        touches(from.y + from.height, to.y),
                        overlap(from.x, from.width, to.x, to.width),
                    ),
                };
                (adjacent && shared > 0.0).then_some((*id, shared))
            })
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(id, _)| id);
        match target {
            Some(id) => self.select_window(id),
            None => self.show_message(format!(
                "No window {} from the selected window",
                direction.name()
            )),
        }
    }

    pub(super) fn balance_windows(&mut self) {
        self.windows.tiling.balance();
    }

    pub(super) fn enlarge_window(&mut self, axis: Axis, lines: isize) {
        let size = match axis {
            Axis::Rows => self.viewport.rows(),
            Axis::Columns => self.viewport.columns(),
        };
        let resized = self
            .windows
            .tiling
            .share(self.windows.selected, axis)
            .filter(|_| size > 0)
            .is_some_and(|share| {
                let delta = signed_as_f32(lines) * share / unsigned_as_f32(size);
                self.windows
                    .tiling
                    .resize(self.windows.selected, axis, delta)
            });
        if !resized {
            self.show_message("Cannot resize the selected window");
        }
    }

    pub fn scroll_window(&mut self, window: WindowId, delta: isize) {
        if window == self.windows.selected {
            self.scroll_lines(delta);
            return;
        }
        let tab_width = self.tab_width;
        let Some((_, parked)) = self.windows.parked.iter_mut().find(|(id, _)| *id == window) else {
            return;
        };
        let Some(index) = self.buffers.position(|buffer| buffer.id() == parked.buffer) else {
            return;
        };
        let Some(buffer) = self.buffers.get(index) else {
            return;
        };
        let layout = Layout::new(buffer, parked.viewport.wrap_width(), tab_width);
        parked.viewport.scroll_by(delta, &layout);
        let point = buffer.marker(parked.point).unwrap_or(0);
        let spot = layout.screen_line(point);
        let target = if spot < parked.viewport.top() {
            parked.viewport.top()
        } else if parked.viewport.shows(spot, &layout) {
            return;
        } else {
            parked.viewport.bottom(&layout)
        };
        let position = layout.position_at(target, layout.column(point));
        let marker = parked.point;
        if let Some(buffer) = self.buffers.get_mut(index) {
            buffer.set_marker(marker, position);
        }
    }

    pub fn drag_divider(&mut self, path: &[usize], index: usize, fraction: f32) {
        self.windows.tiling.set_divider(path, index, fraction);
    }

    pub fn resize_pane(&mut self, window: WindowId, rows: usize, columns: usize) {
        if window == self.windows.selected {
            self.resize_window(rows, columns);
        } else {
            self.resize_parked(window, rows, columns);
        }
        self.render_help();
        self.refill_chats();
    }

    pub(super) fn width_showing(&self, buffer: BufferId) -> Option<usize> {
        if self.buffers.current().id() == buffer {
            return Some(self.viewport.wrap_width());
        }
        self.windows
            .parked
            .iter()
            .find(|(_, parked)| parked.buffer == buffer)
            .map(|(_, parked)| parked.viewport.wrap_width())
    }

    fn resize_parked(&mut self, window: WindowId, rows: usize, columns: usize) {
        let tab_width = self.tab_width;
        let Some((_, parked)) = self.windows.parked.iter_mut().find(|(id, _)| *id == window) else {
            return;
        };
        parked.viewport.resize(rows, columns);
        let Some(buffer) = self
            .buffers
            .position(|buffer| buffer.id() == parked.buffer)
            .and_then(|index| self.buffers.get(index))
        else {
            return;
        };
        let layout = Layout::new(buffer, parked.viewport.wrap_width(), tab_width);
        let point = buffer.marker(parked.point).unwrap_or(0);
        parked.viewport.reveal(layout.screen_line(point), &layout);
    }

    pub(super) fn rehome_windows(&mut self) {
        let current = self.buffers.current().id();
        let missing: Vec<usize> = self
            .windows
            .parked
            .iter()
            .enumerate()
            .filter(|(_, (_, parked))| {
                self.buffers
                    .position(|buffer| buffer.id() == parked.buffer)
                    .is_none()
            })
            .map(|(index, _)| index)
            .collect();
        for index in missing {
            let buffer = self.buffers.current_mut();
            let point = buffer.point();
            let marker = buffer.add_marker(point);
            let parked = &mut self.windows.parked[index].1;
            parked.buffer = current;
            parked.point = marker;
            parked
                .viewport
                .set_top(crate::display::ScreenLine::default());
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::test_support::{editor_with, press, type_text};
    use crate::editor::{Editor, WindowId};

    fn second(editor: &Editor) -> WindowId {
        editor.tiling().windows()[1]
    }

    #[test]
    fn c_x_2_shows_the_buffer_in_a_new_window() {
        let mut editor = Editor::new();
        let first = editor.selected_window();

        press(&mut editor, "C-x 2");

        assert_eq!(editor.tiling().windows().len(), 2);
        assert_eq!(editor.selected_window(), first);
        let pane = editor.pane_of(second(&editor)).unwrap();
        assert_eq!(pane.buffer().name(), "*scratch*");
        assert!(!pane.is_selected());
    }

    #[test]
    fn each_window_keeps_its_own_point() {
        let mut editor = editor_with("abc\ndef", 0);
        press(&mut editor, "C-x 3 C-n");
        assert_eq!(editor.buffer().point(), 4);

        press(&mut editor, "C-x o");
        assert_eq!(editor.buffer().point(), 0);

        press(&mut editor, "C-x o");
        assert_eq!(editor.buffer().point(), 4);
    }

    #[test]
    fn other_windows_follow_edits_made_elsewhere() {
        let mut editor = editor_with("abc", 0);
        press(&mut editor, "C-x 2 C-x o M-> C-x o");

        type_text(&mut editor, "xx");

        assert_eq!(editor.pane_of(second(&editor)).unwrap().point(), 5);
    }

    #[test]
    fn windows_can_show_different_buffers() {
        let mut editor = Editor::new();
        press(&mut editor, "C-x 3 C-x b");
        type_text(&mut editor, "notes");
        press(&mut editor, "RET");

        assert_eq!(editor.buffer().name(), "notes");
        let other = editor.pane_of(second(&editor)).unwrap();
        assert_eq!(other.buffer().name(), "*scratch*");
        assert_eq!(other.mode_line(), "-- *scratch*   L1 C0");
    }

    #[test]
    fn c_x_0_deletes_the_selected_window() {
        let mut editor = Editor::new();
        press(&mut editor, "C-x 2 C-x o C-x 0");
        assert_eq!(editor.tiling().windows().len(), 1);

        press(&mut editor, "C-x 0");
        assert_eq!(
            editor.message(),
            Some("Attempt to delete minibuffer or sole ordinary window")
        );
    }

    #[test]
    fn c_x_1_keeps_only_the_selected_window() {
        let mut editor = Editor::new();
        let first = editor.selected_window();

        press(&mut editor, "C-x 2 C-x 3 C-x 1");

        assert_eq!(editor.tiling().windows(), [first]);
    }

    fn width_of(editor: &Editor, window: WindowId) -> f32 {
        editor
            .tiling()
            .rects()
            .into_iter()
            .find(|(id, _)| *id == window)
            .unwrap()
            .1
            .width
    }

    #[test]
    fn windmove_goes_to_the_neighbor_in_a_direction() {
        let mut editor = Editor::new();
        let first = editor.selected_window();
        press(&mut editor, "C-x 3");

        press(&mut editor, "C-c <right>");
        assert_eq!(editor.selected_window(), second(&editor));

        press(&mut editor, "C-c <left>");
        assert_eq!(editor.selected_window(), first);

        press(&mut editor, "C-c <up>");
        assert_eq!(
            editor.message(),
            Some("No window above from the selected window")
        );
    }

    #[test]
    fn c_x_plus_balances_the_windows() {
        let mut editor = Editor::new();
        press(&mut editor, "C-x 3 C-x 3");
        assert!((width_of(&editor, second(&editor)) - 0.25).abs() < 1e-4);

        press(&mut editor, "C-x +");

        assert!((width_of(&editor, second(&editor)) - 1.0 / 3.0).abs() < 1e-4);
    }

    #[test]
    fn resize_keys_grow_and_shrink_by_lines_and_columns() {
        let mut editor = Editor::new();
        editor.resize_window(10, 40);
        let first = editor.selected_window();
        press(&mut editor, "C-x 3");

        press(&mut editor, "C-x }");
        assert!((width_of(&editor, first) - 0.5125).abs() < 1e-4);

        press(&mut editor, "C-x { C-x {");
        assert!((width_of(&editor, first) - 0.4875).abs() < 1e-3);

        press(&mut editor, "C-x ^");
        assert_eq!(editor.message(), Some("Cannot resize the selected window"));
    }

    #[test]
    fn the_wheel_scrolls_an_unselected_window_without_selecting_it() {
        let text = (0..30)
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let mut editor = editor_with(&text, 0);
        editor.resize_window(10, 0);
        press(&mut editor, "C-x 2");
        let other = second(&editor);
        editor.resize_pane(other, 10, 0);

        editor.scroll_window(other, 5);

        let pane = editor.pane_of(other).unwrap();
        assert_eq!(pane.viewport().top().line, 5);
        assert_eq!(pane.point(), 10);
        assert_ne!(editor.selected_window(), other);
        assert_eq!(editor.viewport().top().line, 0);
    }

    #[test]
    fn dividers_can_be_dragged() {
        let mut editor = Editor::new();
        let first = editor.selected_window();
        press(&mut editor, "C-x 3");

        editor.drag_divider(&[], 0, 0.3);

        assert!((width_of(&editor, first) - 0.3).abs() < 1e-4);
    }

    #[test]
    fn killing_a_buffer_moves_its_windows_elsewhere() {
        let mut editor = Editor::new();
        press(&mut editor, "C-x 2 C-x b");
        type_text(&mut editor, "doomed");
        press(&mut editor, "RET C-x o C-x k");
        type_text(&mut editor, "doomed");
        press(&mut editor, "RET");

        let first = editor.tiling().windows()[0];
        let other = editor.pane_of(first).unwrap();
        assert_eq!(other.buffer().name(), "*scratch*");
    }
}
