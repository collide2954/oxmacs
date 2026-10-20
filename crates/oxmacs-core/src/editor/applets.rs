use std::any::Any;

use crate::applet::AppletBuffer;
use crate::editor::Editor;
use crate::editor::buffers::Mode;
use crate::editor::page::Page;
use crate::text::{Buffer, BufferId};

fn downcast<T: AppletBuffer>(view: &dyn AppletBuffer) -> Option<&T> {
    let any: &dyn Any = view;
    any.downcast_ref()
}

impl Editor {
    pub fn open_applet_buffer(&mut self, name: &str, view: impl AppletBuffer) -> BufferId {
        let mut buffer = Buffer::with_text(name, "");
        buffer.set_read_only(true);
        let id = buffer.id();
        self.add_buffer_with(buffer, Mode::Applet(Box::new(view)));
        id
    }

    pub fn find_applet_buffer<T: AppletBuffer>(
        &self,
        matches: impl Fn(&T) -> bool,
    ) -> Option<BufferId> {
        let index = self.buffers.find_mode(|mode| match mode {
            Mode::Applet(view) => downcast::<T>(view.as_ref()).is_some_and(&matches),
            _ => false,
        })?;
        self.buffers.get(index).map(Buffer::id)
    }

    pub fn applet_buffer<T: AppletBuffer>(&self, id: BufferId) -> Option<(&Buffer, &T)> {
        let index = self.buffers.position(|buffer| buffer.id() == id)?;
        let Some(Mode::Applet(view)) = self.buffers.mode(index) else {
            return None;
        };
        Some((self.buffers.get(index)?, downcast(view.as_ref())?))
    }

    pub fn applet_buffer_mut<T: AppletBuffer>(
        &mut self,
        id: BufferId,
    ) -> Option<(&mut Buffer, &mut T)> {
        let index = self.buffers.position(|buffer| buffer.id() == id)?;
        let (buffer, Mode::Applet(view)) = self.buffers.entry_mut(index)? else {
            return None;
        };
        let any: &mut dyn Any = view.as_mut();
        Some((buffer, any.downcast_mut()?))
    }

    pub fn show_buffer(&mut self, id: BufferId) {
        if let Some(index) = self.buffers.position(|buffer| buffer.id() == id) {
            self.select_buffer(index);
        }
    }

    pub fn fill_buffer(&mut self, id: BufferId, text: &str) {
        let Some(index) = self.buffers.position(|buffer| buffer.id() == id) else {
            return;
        };
        if let Some(buffer) = self.buffers.get_mut(index) {
            Page {
                text: text.to_owned(),
                styles: Vec::new(),
            }
            .fill(buffer);
        }
    }
}
