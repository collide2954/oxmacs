use std::any::Any;
use std::path::{Path, PathBuf};

use crate::command::{Command, CommandId};
use crate::editor::buffers::Mode;
use crate::editor::page::Page;
use crate::editor::{Editor, Effect};
use crate::input::{KeySequence, Lookup};
use crate::plugin::PluginBuffer;
use crate::text::{Buffer, BufferId};

pub(super) type Segment = fn(&Editor) -> Option<String>;

fn downcast<T: PluginBuffer>(view: &dyn PluginBuffer) -> Option<&T> {
    let any: &dyn Any = view;
    any.downcast_ref()
}

impl Editor {
    pub fn on_display(&mut self, handler: fn(&mut Editor)) {
        self.display_handlers.push(handler);
    }

    pub(super) fn tell_display(&mut self) {
        for handler in self.display_handlers.clone() {
            handler(self);
        }
    }

    pub fn repeated(&self) -> bool {
        self.repeated
    }

    pub fn buffer_shown(&self, id: BufferId) -> bool {
        self.window_showing(id).is_some()
    }

    pub fn buffer_width(&self, id: BufferId) -> Option<usize> {
        self.width_showing(id).filter(|width| *width > 0)
    }

    pub fn move_points(&mut self, id: BufferId, to: impl Fn(usize, usize) -> Option<usize>) {
        self.move_windows(id, to);
    }

    pub fn hide_buffer(&mut self, id: BufferId) -> bool {
        let Some(window) = self.window_showing(id) else {
            return false;
        };
        let origin = self.selected_window();
        self.select_window(window);
        self.delete_window();
        if origin != window {
            self.select_window(origin);
        }
        true
    }

    pub fn open_externally(&mut self, target: impl Into<PathBuf>) {
        self.effects.push(Effect::OpenExternally(target.into()));
    }

    pub fn show_buffer_beside(&mut self, id: BufferId) {
        self.show_beside(move |editor| editor.buffers.position(|buffer| buffer.id() == id));
    }

    pub fn on_command(&mut self, handler: fn(&mut Editor)) {
        self.command_handlers.push(handler);
    }

    pub(super) fn tell_command(&mut self) {
        for handler in self.command_handlers.clone() {
            handler(self);
        }
    }

    pub fn on_disk_change(&mut self, handler: fn(&mut Editor)) {
        self.disk_handlers.push(handler);
    }

    pub(super) fn tell_disk_change(&mut self) {
        for handler in self.disk_handlers.clone() {
            handler(self);
        }
    }

    pub fn add_file_opener(
        &mut self,
        extensions: &'static [&'static str],
        open: fn(&mut Editor, &Path) -> bool,
    ) {
        self.file_openers.push((extensions, open));
    }

    pub(super) fn file_opener(&self, path: &Path) -> Option<fn(&mut Editor, &Path) -> bool> {
        let extension = path.extension()?.to_str()?;
        self.file_openers
            .iter()
            .find(|(extensions, _)| {
                extensions
                    .iter()
                    .any(|known| known.eq_ignore_ascii_case(extension))
            })
            .map(|(_, open)| *open)
    }

    pub fn on_open_directory(&mut self, handler: fn(&mut Editor, &Path) -> bool) {
        self.directory_handler = Some(handler);
    }

    pub fn on_save(&mut self, handler: fn(&mut Editor, BufferId)) {
        self.save_handlers.push(handler);
    }

    pub(super) fn tell_save(&mut self, id: BufferId) {
        for handler in self.save_handlers.clone() {
            handler(self, id);
        }
    }

    pub fn on_saved(&mut self, handler: fn(&mut Editor, BufferId)) {
        self.saved_handlers.push(handler);
    }

    pub(super) fn tell_saved(&mut self, id: BufferId) {
        for handler in self.saved_handlers.clone() {
            handler(self, id);
        }
    }

    pub fn buffer_by_id(&self, id: BufferId) -> Option<&Buffer> {
        let index = self.buffers.position(|buffer| buffer.id() == id)?;
        self.buffers.get(index)
    }

    pub fn buffer_by_id_mut(&mut self, id: BufferId) -> Option<&mut Buffer> {
        let index = self.buffers.position(|buffer| buffer.id() == id)?;
        self.buffers.get_mut(index)
    }

    pub fn home(&self) -> Option<&Path> {
        self.home.as_deref()
    }

    pub fn set_home(&mut self, home: Option<PathBuf>) {
        self.home = home;
    }

    pub fn file_moved(&mut self, from: &Path, to: &Path) {
        for index in 0..self.buffers.len() {
            if let Some(file) = self.buffers.get_mut(index).and_then(Buffer::file_mut)
                && let Ok(rest) = file.path.strip_prefix(from)
            {
                file.path = if rest.as_os_str().is_empty() {
                    to.to_owned()
                } else {
                    to.join(rest)
                };
            }
        }
        self.name_file_buffers();
    }

    pub(super) fn bound_command(&self, keys: &str) -> Option<CommandId> {
        let keys: KeySequence = keys.parse().ok()?;
        match self.keymaps.get(self.buffer_scope()).lookup(keys.keys()) {
            Lookup::Command(command) => Some(command),
            Lookup::Prefix | Lookup::Unbound => None,
        }
    }

    pub fn buffer_ids(&self) -> Vec<BufferId> {
        self.buffers.iter().map(Buffer::id).collect()
    }

    pub fn buffer_visiting(&self, path: &Path) -> Option<BufferId> {
        let index = self
            .buffers
            .position(|buffer| buffer.file().is_some_and(|file| file.path == path))?;
        self.buffers.get(index).map(Buffer::id)
    }

    pub fn save_buffer(&mut self, id: BufferId) -> bool {
        self.buffers
            .position(|buffer| buffer.id() == id)
            .is_some_and(|index| self.save_buffer_at(index))
    }

    pub fn on_quit(&mut self, handler: fn(&mut Editor)) {
        self.quit_handlers.push(handler);
    }

    pub fn quitting(&mut self) {
        for handler in self.quit_handlers.clone() {
            handler(self);
        }
    }

    pub fn on_configured(&mut self, handler: fn(&mut Editor)) {
        self.configured_handlers.push(handler);
    }

    pub(super) fn tell_configured(&mut self) {
        for handler in self.configured_handlers.clone() {
            handler(self);
        }
    }

    pub fn add_mode_line_segment(&mut self, segment: Segment) {
        self.mode_line_segments.push(segment);
    }

    pub fn mode_line_segments(&self) -> Vec<String> {
        self.macros
            .recording()
            .then(|| "Recording macro".to_owned())
            .into_iter()
            .chain(
                self.mode_line_segments
                    .iter()
                    .filter_map(|segment| segment(self)),
            )
            .collect()
    }

    pub fn open_plugin_buffer(&mut self, name: &str, view: impl PluginBuffer) -> BufferId {
        let mut buffer = Buffer::with_text(name, "");
        buffer.set_read_only(true);
        let id = buffer.id();
        self.add_buffer_with(buffer, Mode::Plugin(Box::new(view)));
        id
    }

    pub fn add_plugin_buffer_with<T: PluginBuffer>(
        &mut self,
        name: &str,
        make: impl FnOnce(&mut Buffer) -> T,
    ) -> BufferId {
        let mut buffer = Buffer::with_text(name, "");
        let view = make(&mut buffer);
        let id = buffer.id();
        self.buffers
            .push_back_with(buffer, Mode::Plugin(Box::new(view)));
        self.name_file_buffers();
        id
    }

    pub(super) fn guard_input(&mut self, command: Command) -> bool {
        if !command.modifies_text() {
            return false;
        }
        let Some((buffer, Mode::Plugin(view))) = self.buffers.entry_mut(0) else {
            return false;
        };
        let Some(start) = view.input_line().and_then(|input| input.start(buffer)) else {
            return false;
        };
        if buffer.point() < start {
            buffer.set_point(buffer.len_chars());
        }
        buffer.point() <= start
            && matches!(
                command,
                Command::DeleteBackwardChar | Command::BackwardDeleteWord
            )
    }

    pub fn with_plugin_state<T: Any + Send, R>(
        &mut self,
        work: impl FnOnce(&mut Editor, &mut T) -> R,
    ) -> Option<R> {
        let mut state = self.plugin_states.take::<T>()?;
        let result = work(self, &mut state);
        self.plugin_states.set(state);
        Some(result)
    }

    pub fn plugin_buffers<T: PluginBuffer>(&self) -> Vec<BufferId> {
        (0..self.buffers.len())
            .filter(|index| {
                matches!(self.buffers.mode(*index), Some(Mode::Plugin(view)) if downcast::<T>(view.as_ref()).is_some())
            })
            .filter_map(|index| self.buffers.get(index).map(Buffer::id))
            .collect()
    }

    pub fn buffer_named(&self, name: &str) -> Option<BufferId> {
        let index = self.buffers.position(|buffer| buffer.name() == name)?;
        self.buffers.get(index).map(Buffer::id)
    }

    pub fn buffer_mut(&mut self) -> &mut Buffer {
        self.buffers.current_mut()
    }

    pub fn close_buffer(&mut self, id: BufferId) {
        if let Some(index) = self.buffers.position(|buffer| buffer.id() == id) {
            self.finish_kill(index);
        }
    }

    pub fn find_plugin_buffer<T: PluginBuffer>(
        &self,
        matches: impl Fn(&T) -> bool,
    ) -> Option<BufferId> {
        let index = self.buffers.find_mode(|mode| match mode {
            Mode::Plugin(view) => downcast::<T>(view.as_ref()).is_some_and(&matches),
            _ => false,
        })?;
        self.buffers.get(index).map(Buffer::id)
    }

    pub fn plugin_buffer<T: PluginBuffer>(&self, id: BufferId) -> Option<(&Buffer, &T)> {
        let index = self.buffers.position(|buffer| buffer.id() == id)?;
        let Some(Mode::Plugin(view)) = self.buffers.mode(index) else {
            return None;
        };
        Some((self.buffers.get(index)?, downcast(view.as_ref())?))
    }

    pub fn plugin_buffer_mut<T: PluginBuffer>(
        &mut self,
        id: BufferId,
    ) -> Option<(&mut Buffer, &mut T)> {
        let index = self.buffers.position(|buffer| buffer.id() == id)?;
        let (buffer, Mode::Plugin(view)) = self.buffers.entry_mut(index)? else {
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
