use std::any::Any;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::applet::{AppletBuffer, DoctorLine};
use crate::command::Command;
use crate::config::Config;
use crate::config::schema::Setting;
use crate::editor::Editor;
use crate::editor::buffers::Mode;
use crate::editor::page::Page;
use crate::text::{Buffer, BufferId};

#[derive(Debug, Clone, Copy)]
pub(super) struct ConfigTable {
    name: &'static str,
    check: fn(&Config, &str) -> Result<(), String>,
    defaults: fn() -> Option<toml::Table>,
}

pub(super) type DoctorSection = fn(&Editor) -> Vec<DoctorLine>;

pub(super) type Segment = fn(&Editor) -> Option<String>;

fn check<T: DeserializeOwned + Default>(config: &Config, name: &str) -> Result<(), String> {
    config.applet::<T>(name).map(|_| ())
}

fn defaults<T: Serialize + Default>() -> Option<toml::Table> {
    match toml::Value::try_from(T::default()).ok()? {
        toml::Value::Table(table) => Some(table),
        _ => None,
    }
}

fn downcast<T: AppletBuffer>(view: &dyn AppletBuffer) -> Option<&T> {
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
        self.mode_line_segments
            .iter()
            .filter_map(|segment| segment(self))
            .collect()
    }

    pub fn add_doctor_section(&mut self, title: &'static str, lines: DoctorSection) {
        self.doctor_sections.push((title, lines));
    }

    pub fn add_config_table<T: Serialize + DeserializeOwned + Default>(
        &mut self,
        name: &'static str,
    ) {
        self.config_tables.push(ConfigTable {
            name,
            check: check::<T>,
            defaults: defaults::<T>,
        });
        self.check_config_tables();
    }

    pub fn add_settings(&mut self, table: &'static str, settings: &'static [Setting]) {
        self.applet_settings
            .extend(settings.iter().map(|setting| (table, setting)));
    }

    pub(super) fn check_config_table(&self, config: &Config, name: &str) -> Result<(), String> {
        self.config_tables
            .iter()
            .filter(|table| table.name == name)
            .try_for_each(|table| (table.check)(config, name))
    }

    pub(super) fn config_default(&self, name: &str, key: &str) -> Option<toml::Value> {
        let table = self.config_tables.iter().find(|table| table.name == name)?;
        (table.defaults)()?.remove(key)
    }

    pub fn applet_config<T: DeserializeOwned + Default>(&self, name: &str) -> T {
        self.config.applet(name).unwrap_or_default()
    }

    pub fn unclaimed_config_tables(&self) -> Vec<&str> {
        self.config
            .applet_tables()
            .into_iter()
            .filter(|table| self.config_tables.iter().all(|known| known.name != *table))
            .collect()
    }

    pub(super) fn check_config_tables(&mut self) {
        let problems: Vec<String> = self
            .config_tables
            .iter()
            .filter_map(|table| (table.check)(&self.config, table.name).err())
            .collect();
        if !problems.is_empty() {
            self.show_message(problems.join("; "));
        }
    }

    pub fn open_applet_buffer(&mut self, name: &str, view: impl AppletBuffer) -> BufferId {
        let mut buffer = Buffer::with_text(name, "");
        buffer.set_read_only(true);
        let id = buffer.id();
        self.add_buffer_with(buffer, Mode::Applet(Box::new(view)));
        id
    }

    pub fn add_applet_buffer_with<T: AppletBuffer>(
        &mut self,
        name: &str,
        make: impl FnOnce(&mut Buffer) -> T,
    ) -> BufferId {
        let mut buffer = Buffer::with_text(name, "");
        let view = make(&mut buffer);
        let id = buffer.id();
        self.buffers
            .push_back_with(buffer, Mode::Applet(Box::new(view)));
        id
    }

    pub(super) fn guard_input(&mut self, command: Command) -> bool {
        if !command.modifies_text() {
            return false;
        }
        let Some((buffer, Mode::Applet(view))) = self.buffers.entry_mut(0) else {
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

    pub fn with_applet_state<T: Any + Send, R>(
        &mut self,
        work: impl FnOnce(&mut Editor, &mut T) -> R,
    ) -> Option<R> {
        let mut state = self.applet_states.take::<T>()?;
        let result = work(self, &mut state);
        self.applet_states.set(state);
        Some(result)
    }

    pub fn applet_buffers<T: AppletBuffer>(&self) -> Vec<BufferId> {
        (0..self.buffers.len())
            .filter(|index| {
                matches!(self.buffers.mode(*index), Some(Mode::Applet(view)) if downcast::<T>(view.as_ref()).is_some())
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
