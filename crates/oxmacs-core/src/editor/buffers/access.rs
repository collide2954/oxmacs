use std::path::{Path, PathBuf};

use crate::command::Command;
use crate::display::LineStyle;
use crate::editor::Editor;
use crate::editor::buffers::Mode;
use crate::editor::page::Page;
use crate::files;
use crate::special_buffer::SpecialBuffer;
use crate::text::{Buffer, BufferId};

impl Editor {
    pub fn buffer_shown(&self, id: BufferId) -> bool {
        self.window_showing(id).is_some()
    }

    pub fn buffer_width(&self, id: BufferId) -> Option<usize> {
        self.width_showing(id).filter(|width| *width > 0)
    }

    pub fn move_points(&mut self, id: BufferId, to: impl Fn(usize, usize) -> Option<usize>) {
        self.move_windows(id, to);
    }

    pub fn set_header_lines(&mut self, id: BufferId, lines: Vec<(String, LineStyle)>) {
        let alive: Vec<BufferId> = self.buffers.iter().map(Buffer::id).collect();
        self.headers
            .retain(|(known, _)| *known != id && alive.contains(known));
        if !lines.is_empty() {
            self.headers.push((id, lines));
        }
    }

    pub(in crate::editor) fn header_lines_of(&self, id: BufferId) -> Vec<(String, LineStyle)> {
        self.headers
            .iter()
            .find(|(known, _)| *known == id)
            .map(|(_, lines)| lines.clone())
            .unwrap_or_default()
    }

    pub fn show_text(
        &mut self,
        name: &str,
        text: &str,
        language: Option<crate::syntax::Language>,
        read_only: bool,
    ) -> Result<BufferId, String> {
        match self.buffers.position(|buffer| buffer.name() == name) {
            Some(index) if self.buffers.get(index).and_then(Buffer::file).is_some() => {
                return Err(format!("{name} visits a file"));
            }
            Some(index) => self.select_buffer(index),
            None => self.add_buffer_with(Buffer::with_text(name, ""), Mode::Text(None)),
        }
        if language.is_some() {
            self.set_major_mode(language);
        }
        let buffer = self.buffers.current_mut();
        buffer.set_read_only(false);
        buffer.set_text(text);
        buffer.mark_saved();
        buffer.set_read_only(read_only);
        buffer.set_point(0);
        Ok(buffer.id())
    }

    pub fn buffer_by_id(&self, id: BufferId) -> Option<&Buffer> {
        let index = self.buffers.position(|buffer| buffer.id() == id)?;
        self.buffers.get(index)
    }

    pub fn buffer_by_id_mut(&mut self, id: BufferId) -> Option<&mut Buffer> {
        let index = self.buffers.position(|buffer| buffer.id() == id)?;
        self.buffers.get_mut(index)
    }

    pub fn file_moved(&mut self, from: &Path, to: &Path) {
        let from = files::identity(from);
        let moves: Vec<(usize, PathBuf)> = (0..self.buffers.len())
            .filter_map(|index| {
                let file = self.buffers.get(index)?.file()?;
                let rest = file.identity().strip_prefix(&from).ok()?;
                let moved = if rest.as_os_str().is_empty() {
                    to.to_owned()
                } else {
                    to.join(rest)
                };
                Some((index, moved))
            })
            .collect();
        for (index, moved) in moves {
            self.visit_path(index, moved);
        }
        self.name_file_buffers();
    }

    pub fn buffer_ids(&self) -> Vec<BufferId> {
        self.buffers.iter().map(Buffer::id).collect()
    }

    pub fn buffer_visiting(&self, path: &Path) -> Option<BufferId> {
        let index = self.index_visiting(path)?;
        self.buffers.get(index).map(Buffer::id)
    }

    pub fn buffer_showing(&self, path: &Path) -> Option<BufferId> {
        let index = self.index_showing(path)?;
        self.buffers.get(index).map(Buffer::id)
    }

    pub fn save_buffer(&mut self, id: BufferId) -> bool {
        self.buffers
            .position(|buffer| buffer.id() == id)
            .is_some_and(|index| self.save_buffer_at(index))
    }

    #[cfg(test)]
    pub fn open_special_buffer(&mut self, name: &str, view: impl SpecialBuffer) -> BufferId {
        let mut buffer = Buffer::with_text(name, "");
        buffer.set_read_only(true);
        let id = buffer.id();
        self.add_buffer_with(buffer, Mode::Special(Box::new(view)));
        id
    }

    pub fn add_special_buffer_with<T: SpecialBuffer>(
        &mut self,
        name: &str,
        make: impl FnOnce(&mut Buffer) -> T,
    ) -> BufferId {
        let mut buffer = Buffer::with_text(name, "");
        let view = make(&mut buffer);
        let id = buffer.id();
        self.buffers
            .push_back_with(buffer, Mode::Special(Box::new(view)));
        self.name_file_buffers();
        id
    }

    pub(in crate::editor) fn guard_input(&mut self, command: Command) -> bool {
        if !command.modifies_text() {
            return false;
        }
        let Some((buffer, Mode::Special(view))) = self.buffers.entry_mut(0) else {
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

    pub fn special_buffers<T: SpecialBuffer>(&self) -> Vec<BufferId> {
        (0..self.buffers.len())
            .filter(|index| self.buffers.special::<T>(*index).is_some())
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

    pub fn find_special_buffer<T: SpecialBuffer>(
        &self,
        matches: impl Fn(&T) -> bool,
    ) -> Option<BufferId> {
        let index = self.buffers.find_special(matches)?;
        self.buffers.get(index).map(Buffer::id)
    }

    pub fn special_buffer<T: SpecialBuffer>(&self, id: BufferId) -> Option<(&Buffer, &T)> {
        let index = self.buffers.position(|buffer| buffer.id() == id)?;
        Some((self.buffers.get(index)?, self.buffers.special(index)?))
    }

    pub fn special_buffer_mut<T: SpecialBuffer>(
        &mut self,
        id: BufferId,
    ) -> Option<(&mut Buffer, &mut T)> {
        let index = self.buffers.position(|buffer| buffer.id() == id)?;
        self.buffers.entry_special_mut(index)
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
