use std::any::Any;

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::applet::{AppletBuffer, DoctorLine};
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
    pub fn on_quit(&mut self, handler: fn(&mut Editor)) {
        self.quit_handlers.push(handler);
    }

    pub fn quitting(&mut self) {
        self.irc_farewell();
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
