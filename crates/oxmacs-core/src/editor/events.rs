use std::collections::HashMap;
use std::path::{Path, PathBuf};

use strum::EnumDiscriminants;

use crate::editor::buffers::Mode;
use crate::editor::{Arrangement, Cell, Clicks, Editor, WindowId};
use crate::input::{Key, KeyCode};
use crate::special_buffer::SpecialBuffer;
use crate::text::{BufferId, Splice};

#[derive(Debug, Clone, PartialEq, Eq, EnumDiscriminants)]
#[strum_discriminants(name(EventKind), derive(Hash))]
pub enum Event {
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
    Saved(BufferId),
    DiskChanged,
    Configured,
    Processes,
    Opened(BufferId),
    Killed(BufferId),
    Edited(BufferId, Vec<Splice>),
    Windows,
    Diagnostics,
    ModeChanged(BufferId, String),
    OpenFile {
        path: PathBuf,
        owner: String,
    },
    OpenDirectory {
        path: PathBuf,
        owner: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Claim {
    Extension(String),
    Directories,
}

#[derive(Debug, Default)]
pub(super) struct Events {
    queued: Vec<Event>,
    subscribed: Vec<EventKind>,
    claims: Vec<(Claim, String)>,
    windows: Option<(WindowId, BufferId, Arrangement)>,
    diagnostics: u64,
    modes: HashMap<BufferId, String>,
}

impl Editor {
    pub fn subscribe(&mut self, kind: EventKind) {
        if self.subscribed(kind) {
            return;
        }
        self.events.subscribed.push(kind);
        match kind {
            EventKind::Edited => {
                for index in 0..self.buffers.len() {
                    if let Some((buffer, Mode::Text(_))) = self.buffers.entry_mut(index) {
                        buffer.track_splices();
                    }
                }
            }
            EventKind::Windows => self.events.windows = Some(self.windows_now()),
            EventKind::Diagnostics => self.events.diagnostics = self.diagnostics_revision(),
            EventKind::ModeChanged => self.events.modes = self.modes_now().into_iter().collect(),
            _ => {}
        }
    }

    fn windows_now(&self) -> (WindowId, BufferId, Arrangement) {
        (
            self.selected_window(),
            self.buffer().id(),
            self.window_arrangement(),
        )
    }

    fn modes_now(&self) -> Vec<(BufferId, String)> {
        self.buffer_ids()
            .into_iter()
            .filter_map(|id| Some((id, self.buffer_language(id)?.name().to_owned())))
            .collect()
    }

    pub(super) fn subscribed(&self, kind: EventKind) -> bool {
        self.events.subscribed.contains(&kind)
    }

    pub(super) fn record(&mut self, event: Event) {
        let once = matches!(
            event,
            Event::DiskChanged | Event::Configured | Event::Processes
        );
        if self.subscribed(EventKind::from(&event))
            && !(once && self.events.queued.contains(&event))
        {
            self.events.queued.push(event);
        }
    }

    pub fn settings_changed(&mut self) {
        self.record(Event::Configured);
    }

    pub fn take_events(&mut self) -> Vec<Event> {
        self.notice_changes();
        std::mem::take(&mut self.events.queued)
    }

    pub fn events_waiting(&mut self) -> bool {
        self.notice_changes();
        !self.events.queued.is_empty()
    }

    fn notice_changes(&mut self) {
        if self.subscribed(EventKind::Edited) {
            for index in 0..self.buffers.len() {
                if let Some((buffer, Mode::Text(_))) = self.buffers.entry_mut(index) {
                    let splices = buffer.take_splices();
                    if !splices.is_empty() {
                        let id = buffer.id();
                        self.events.queued.push(Event::Edited(id, splices));
                    }
                }
            }
        }
        if self.subscribed(EventKind::Windows) {
            let now = Some(self.windows_now());
            if self.events.windows != now {
                self.events.windows = now;
                self.events.queued.push(Event::Windows);
            }
        }
        if self.subscribed(EventKind::Diagnostics) {
            let revision = self.diagnostics_revision();
            if self.events.diagnostics != revision {
                self.events.diagnostics = revision;
                self.events.queued.push(Event::Diagnostics);
            }
        }
        if self.subscribed(EventKind::ModeChanged) {
            for (id, name) in self.modes_now() {
                if self.events.modes.get(&id) != Some(&name) {
                    self.events.modes.insert(id, name.clone());
                    self.events.queued.push(Event::ModeChanged(id, name));
                }
            }
        }
    }

    pub fn claim_files(&mut self, owner: &str, extension: &str) {
        let claim = Claim::Extension(extension.to_lowercase());
        self.events.claims.retain(|(known, _)| *known != claim);
        self.events.claims.push((claim, owner.to_owned()));
    }

    pub fn claim_directories(&mut self, owner: &str) {
        self.events
            .claims
            .retain(|(known, _)| *known != Claim::Directories);
        self.events
            .claims
            .push((Claim::Directories, owner.to_owned()));
    }

    pub(super) fn claimed(&mut self, path: &Path) -> bool {
        let claim = if path.is_dir() {
            Claim::Directories
        } else {
            let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
                return false;
            };
            Claim::Extension(extension.to_lowercase())
        };
        let Some(owner) = self
            .events
            .claims
            .iter()
            .find(|(known, _)| *known == claim)
            .map(|(_, owner)| owner.clone())
        else {
            return false;
        };
        let path = path.to_path_buf();
        self.events.queued.push(match claim {
            Claim::Directories => Event::OpenDirectory { path, owner },
            Claim::Extension(_) => Event::OpenFile { path, owner },
        });
        true
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
        self.record(Event::Clicked {
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
            self.record(Event::Dropped {
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
        self.record(Event::Typed {
            buffer,
            text: typed.to_string(),
        });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::{Event, EventKind};
    use crate::command::ScopeId;
    use crate::editor::{Cell, Clicks, Editor};
    use crate::special_buffer::SpecialBuffer;
    use crate::syntax::Language;
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
        for kind in [EventKind::Clicked, EventKind::Dropped, EventKind::Typed] {
            editor.subscribe(kind);
        }
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
            editor.take_events(),
            [
                Event::Clicked {
                    buffer: id,
                    line: 1,
                    clicks: 1
                },
                Event::Clicked {
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

        let heard = editor.take_events();
        assert_eq!(
            heard[1],
            Event::Dropped {
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
            editor.take_events(),
            [
                Event::Typed {
                    buffer: id,
                    text: "a".to_owned()
                },
                Event::Typed {
                    buffer: id,
                    text: " ".to_owned()
                },
            ]
        );
        assert_eq!(editor.buffer().contents(), "a\nb\nc");
        assert_eq!(editor.message(), None);
    }

    #[test]
    fn subscribing_starts_from_the_current_windows_and_modes() {
        let mut editor = Editor::new();
        editor.set_major_mode(Some(Language::RUST));
        for kind in [
            EventKind::Windows,
            EventKind::Diagnostics,
            EventKind::ModeChanged,
        ] {
            editor.subscribe(kind);
        }

        assert_eq!(editor.take_events(), []);
        press(&mut editor, "C-x 2");
        editor.set_major_mode(Some(Language::TOML));

        let id = editor.buffer().id();
        assert_eq!(
            editor.take_events(),
            [
                Event::Windows,
                Event::ModeChanged(id, Language::TOML.name().to_owned())
            ]
        );
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
        assert_eq!(editor.take_events(), []);
    }
}
