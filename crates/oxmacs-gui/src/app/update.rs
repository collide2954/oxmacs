use std::path::Path;
use std::sync::atomic::Ordering;

use iced::font::Family;
use iced::theme::Mode;
use iced::time::Instant;
use iced::{Task, clipboard, keyboard, window};
use oxmacs_core::command::Command;
use oxmacs_core::editor::{Edge, Effect, WindowId};
use oxmacs_core::external::notify;
use oxmacs_core::theme::{Appearance, Theme as Faces};
use oxmacs_core::view::UiEvent;

use oxmacs_core::view::tabbar::TabEvent;
use oxmacs_core::view::takeover;
use oxmacs_script::api::games;

use super::{App, Message, transition};
use crate::keys::{self, Platform};
use crate::widgets::buffer_view::ViewEvent;
use crate::widgets::tab_bar::TabMessage;
use crate::{animation, fetch, fonts, units};
impl App {
    fn handle_view_event(&mut self, window: WindowId, event: ViewEvent) {
        let selected = window == self.editor.selected_window();
        match event {
            ViewEvent::Resized { rows, columns } => self.editor.resize_pane(window, rows, columns),
            ViewEvent::FontNotMonospaced => self.reject_font(),
            ViewEvent::Pressed {
                cell,
                clicks,
                extend,
            } => {
                if self.focus(window) {
                    self.last_input = self.now;
                    self.editor.click(cell, clicks, extend);
                }
            }
            ViewEvent::Dragged(cell) if selected => self.editor.drag(cell),
            ViewEvent::Released(cell) if selected => self.editor.release(cell),
            ViewEvent::NumberPressed { row, extend } => {
                if self.focus(window) {
                    self.last_input = self.now;
                    self.editor.click_line_number(row, extend);
                }
            }
            ViewEvent::NumbersDragged(row) if selected => self.editor.drag_line_numbers(row),
            ViewEvent::Dragged(_) | ViewEvent::Released(_) | ViewEvent::NumbersDragged(_) => {}
            ViewEvent::MiddleClicked(cell) => {
                if self.focus(window) {
                    self.last_input = self.now;
                    self.editor.middle_click(cell);
                }
            }
            ViewEvent::Scrolled(rows) => self.editor.scroll_window(window, rows),
        }
    }

    fn focus(&mut self, window: WindowId) -> bool {
        if window == self.editor.selected_window() {
            return true;
        }
        if self.editor.minibuffer().is_some() {
            return false;
        }
        self.editor.select_window(window);
        true
    }

    fn fade_theme_from(&mut self, shown: Faces) {
        if self.animations && shown.name != self.editor.theme().name {
            let fade = transition(false, animation::THEME_FADE).go(true, self.now);
            self.theme_fade = Some((shown, fade));
        }
    }

    fn sync_reveal(&mut self) {
        let open = self.editor.minibuffer().is_some();
        if open == self.reveal.value() {
            return;
        }
        if open && self.animations {
            self.reveal.go_mut(true, self.now);
        } else {
            self.reveal = transition(open, animation::REVEAL);
        }
    }

    fn tick(&mut self, now: Instant) {
        self.now = now;
        if self
            .theme_fade
            .as_ref()
            .is_some_and(|(_, fade)| !fade.is_animating(now))
        {
            self.theme_fade = None;
        }
        self.ox.games_frame(&mut self.editor, now);
        if let Some(size) = self.takeover_size {
            self.ox.frame(
                &mut self.editor,
                now,
                (size.width, size.height),
                self.text_size,
            );
        }
    }

    fn focus_changed(&mut self, focused: bool) {
        self.focused_since = focused.then_some(self.now);
        self.editor.set_focused(focused);
        if focused {
            self.editor.check_disk_changes();
        }
    }

    fn idle(&mut self) {
        self.editor.idle(self.last_input, self.now);
        if let Some(since) = self.rested_since() {
            self.ox.idle(&mut self.editor, since, self.now);
        }
    }

    fn end_takeover(&mut self) {
        self.last_input = self.now;
        self.ox.event(&mut self.editor, UiEvent::EndTakeOver);
    }

    fn go_fullscreen(&mut self, id: window::Id, mode: window::Mode) -> Task<Message> {
        self.before_fullscreen = Some(mode);
        if self.editor.presentation().fullscreen {
            window::set_mode(id, window::Mode::Fullscreen)
        } else {
            Task::none()
        }
    }

    fn drag_side(&mut self, delta: f32) {
        let Some(side) = self.editor.side_window() else {
            return;
        };
        let cell = units::cell_width(self.text_size);
        let grown = match side.edge {
            Edge::Left => delta,
            Edge::Right => -delta,
        };
        let columns = ((units::to_f32(side.columns) * cell + grown) / cell).round();
        self.editor
            .resize_side(units::floor_to_usize(columns).max(SIDE_COLUMNS));
    }

    fn tab_message(&mut self, event: TabMessage) {
        let happened = match event {
            TabMessage::Pressed(at) => {
                self.tab_drag = Some(at);
                Some(TabEvent::Select(at))
            }
            TabMessage::Released(to) => self
                .tab_drag
                .take()
                .filter(|from| *from != to)
                .map(|from| TabEvent::Move { from, to }),
            TabMessage::BarReleased => {
                self.tab_drag = None;
                None
            }
            TabMessage::Entered(at) => {
                self.tab_hover = Some(at);
                None
            }
            TabMessage::Left(at) => {
                if self.tab_hover == Some(at) {
                    self.tab_hover = None;
                }
                None
            }
            TabMessage::Close(at) => Some(TabEvent::Close(at)),
            TabMessage::New => Some(TabEvent::New),
        };
        if let Some(happened) = happened {
            self.last_input = self.now;
            self.ox.event(&mut self.editor, UiEvent::Tab(happened));
        }
    }

    fn reject_font(&mut self) {
        if self.font == fonts::EDITOR {
            return;
        }
        if let Family::Name(name) = self.font.family {
            self.editor.show_message(format!(
                "{name} is not a monospaced font; using {}",
                fonts::EDITOR_FAMILY
            ));
        }
        self.font = fonts::EDITOR;
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        self.woken.store(false, Ordering::Release);
        if !matches!(message, Message::Tick(_)) {
            self.now = Instant::now();
        }
        let shown = self.faces();
        match message {
            Message::Keyboard(event) => self.keyboard(event),
            Message::EndTakeover => {
                if takeover::current(&self.editor).is_some() {
                    self.end_takeover();
                }
            }
            Message::Opened(Ok(())) | Message::Clock | Message::OxTick => {}
            Message::View(window, event) => self.handle_view_event(window, event),
            Message::Divider { path, index, delta } => {
                if let Some(at) = self.editor.tiling().boundary(&path, index) {
                    self.editor.drag_divider(&path, index, at + delta);
                }
            }
            Message::AppearanceChanged(mode) => {
                if let Some(appearance) = appearance(mode) {
                    self.editor.set_appearance(appearance);
                }
            }
            Message::Tick(now) => self.tick(now),
            Message::Pasted(text) => self.editor.paste(text),
            Message::PastedPrimary(text) => self.editor.paste_primary(text),
            Message::DiskChanged => self.editor.check_disk_changes(),
            Message::Opened(Err(error)) => self.editor.show_message(error),
            Message::ProcessTick => self.sample_processes(),
            Message::FocusWindow(window) => {
                if self.focus(window) {
                    self.last_input = self.now;
                }
            }
            Message::Delivered(delivery) => self.runner.deliver(&mut self.editor, delivery),
            Message::Fetched(id, result) => self.editor.fetched(id, result),
            Message::ToolInstalled(tool, result) => self.editor.tool_installed(tool, result),
            Message::OxPress(buffer, slot) => {
                self.ox
                    .event(&mut self.editor, UiEvent::Press { buffer, slot });
            }
            Message::OxCall(buffer, slot, args) => {
                self.ox
                    .event(&mut self.editor, UiEvent::Call { buffer, slot, args });
            }
            Message::ViewSized(buffer, size) => self.view_sized(buffer, size),
            Message::TakeoverSized(size) => self.takeover_size = Some(size),
            Message::Tab(event) => self.tab_message(event),
            Message::SideDragged(delta) => self.drag_side(delta),
            Message::WindowModeBefore(id, mode) => return self.go_fullscreen(id, mode),
            Message::Focus(focused) => self.focus_changed(focused),
            Message::Idle => self.idle(),
            Message::CloseRequested => self.editor.execute(Command::SaveBuffersKillOxmacs),
        }
        self.ox.tick(&mut self.editor);
        self.settle();
        self.sync_reveal();
        self.fade_theme_from(shown);
        if self.editor.should_quit() {
            self.quit();
            return iced::exit();
        }
        self.pending_effects()
    }

    fn keyboard(&mut self, event: keyboard::Event) {
        match event {
            keyboard::Event::KeyPressed { .. } if takeover::current(&self.editor).is_some() => {
                self.end_takeover();
            }
            keyboard::Event::KeyPressed { key, modifiers, .. }
                if games::wants_keys(&self.editor) && !keys::starts_prefix(&key, modifiers) =>
            {
                self.last_input = self.now;
                if let Some(name) = keys::game_key(&key) {
                    self.ox
                        .event(&mut self.editor, UiEvent::GameKey { name, down: true });
                }
            }
            keyboard::Event::KeyReleased { key, .. } if games::active(&self.editor) => {
                if let Some(name) = keys::game_key(&key) {
                    self.ox
                        .event(&mut self.editor, UiEvent::GameKey { name, down: false });
                }
            }
            keyboard::Event::KeyPressed {
                key,
                modified_key,
                modifiers,
                ..
            } => {
                if let Some(key) =
                    keys::translate(&key, &modified_key, modifiers, Platform::current())
                {
                    self.last_input = self.now;
                    self.editor.handle_key(key);
                }
            }
            _ => {}
        }
    }

    fn view_sized(&mut self, buffer: oxmacs_core::text::BufferId, size: iced::Size) {
        let pixels = |extent: f32| u32::try_from(units::floor_to_usize(extent)).unwrap_or(u32::MAX);
        self.ox.event(
            &mut self.editor,
            UiEvent::Sized {
                buffer,
                width: pixels(size.width),
                height: pixels(size.height),
            },
        );
    }

    fn quit(&mut self) {
        let _ = self.pending_effects();
        self.runner.shutdown(&mut self.editor);
    }

    fn settle(&mut self) {
        self.editor.prepare_display();
    }

    fn sample_processes(&mut self) {
        let samples = self.usage.sample(&self.editor.process_pids());
        self.editor.processes_tick(&samples);
    }

    pub(super) fn pending_effects(&mut self) -> Task<Message> {
        let effects = self.editor.take_effects();
        Task::batch(effects.into_iter().map(|effect| self.perform(effect)))
    }

    fn perform(&mut self, effect: Effect) -> Task<Message> {
        let Some(effect) = self.runner.perform(&mut self.editor, effect) else {
            return Task::none();
        };
        match effect {
            Effect::WriteClipboard(text) => clipboard::write(text),
            Effect::ReadClipboard => clipboard::read().map(Message::Pasted),
            Effect::WritePrimary(text) if HAS_PRIMARY_SELECTION => clipboard::write_primary(text),
            Effect::ReadPrimary if HAS_PRIMARY_SELECTION => {
                clipboard::read_primary().map(Message::PastedPrimary)
            }
            Effect::WritePrimary(_)
            | Effect::ReadPrimary
            | Effect::Spawn(_)
            | Effect::StartSession(_)
            | Effect::Send(..)
            | Effect::StartServer(..)
            | Effect::ToServer(..) => Task::none(),
            Effect::OpenExternally(path) => Task::done(Message::Opened(open_externally(&path))),
            Effect::Reconfigure => {
                let config = self.editor.config().clone();
                self.configure(&config);
                self.pending_effects()
            }
            Effect::Fetch(id, url) => {
                Task::perform(fetch::get(url), move |result| Message::Fetched(id, result))
            }
            Effect::InstallTool(install) => {
                let tool = install.tool;
                Task::perform(fetch::install(*install), move |result| {
                    Message::ToolInstalled(tool, result)
                })
            }
            Effect::Notify { title, body } => {
                notify::send(&title, &body);
                Task::none()
            }
            Effect::Fullscreen(true) => window::latest().and_then(|id| {
                window::mode(id).map(move |mode| Message::WindowModeBefore(id, mode))
            }),
            Effect::Fullscreen(false) => {
                let mode = self
                    .before_fullscreen
                    .take()
                    .unwrap_or(window::Mode::Windowed);
                window::latest().and_then(move |id| window::set_mode(id, mode))
            }
        }
    }
}

const HAS_PRIMARY_SELECTION: bool = cfg!(target_os = "linux");
const SIDE_COLUMNS: usize = 10;

fn open_externally(path: &Path) -> Result<(), String> {
    open::that_detached(path).map_err(|error| format!("{}: {error}", path.display()))
}

fn appearance(mode: Mode) -> Option<Appearance> {
    match mode {
        Mode::Light => Some(Appearance::Light),
        Mode::Dark => Some(Appearance::Dark),
        Mode::None => None,
    }
}
