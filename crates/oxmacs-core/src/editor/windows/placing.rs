use crate::editor::{Axis, Editor};
use crate::text::BufferId;

impl Editor {
    pub fn show_buffer_beside(&mut self, id: BufferId) {
        self.show_beside(move |editor| editor.buffers.position(|buffer| buffer.id() == id));
    }

    pub fn display_buffer(&mut self, id: BufferId, near: Option<(BufferId, Axis)>, select: bool) {
        if self.window_showing(id).is_none() {
            let place = move |editor: &mut Self| editor.show_buffer(id);
            let beside = near.and_then(|(other, axis)| Some((self.window_showing(other)?, axis)));
            match beside {
                Some((window, axis)) => self.split_off(window, axis, place),
                None => self.beside(place),
            }
        }
        if select && let Some(window) = self.window_showing(id) {
            self.select_window(window);
        }
    }
}
