use std::path::Path;

use iced::font::Family;
use iced::futures::channel::mpsc;
use iced::theme::Mode;
use iced::time::{Duration, Instant};
use iced::widget::scrollable::{AbsoluteOffset, RelativeOffset};
use iced::widget::{markdown, operation};
use iced::{Task, clipboard, keyboard};
use jiff::Timestamp;
use oxmacs_core::editor::{ConnectionId, Effect, Job, Outcome, PreviewScroll, WindowId};
use oxmacs_core::external::net::{self, Endpoint};
use oxmacs_core::external::process;
use oxmacs_core::text::BufferId;
use oxmacs_core::theme::{Appearance, Theme as Faces};

use super::{App, Message, preview_id, transition};
use crate::keys::{self, Platform};
use crate::widgets::buffer_view::ViewEvent;
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
            ViewEvent::NumberPressed { row, extend } => {
                if self.focus(window) {
                    self.last_input = self.now;
                    self.editor.click_line_number(row, extend);
                }
            }
            ViewEvent::NumbersDragged(row) if selected => self.editor.drag_line_numbers(row),
            ViewEvent::Dragged(_) | ViewEvent::NumbersDragged(_) => {}
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

    fn refresh_previews(&mut self) {
        let mut fresh: Vec<(BufferId, u64, markdown::Content)> = Vec::new();
        for window in self.editor.tiling().windows() {
            let Some(source) = self
                .editor
                .pane_of(window)
                .and_then(|pane| pane.preview_source())
            else {
                continue;
            };
            let (id, revision) = (source.id(), source.revision());
            if fresh.iter().any(|(known, ..)| *known == id) {
                continue;
            }
            let content = match self
                .previews
                .iter()
                .position(|(known, seen, _)| *known == id && *seen == revision)
            {
                Some(index) => self.previews.swap_remove(index).2,
                None => markdown::Content::parse(&source.contents()),
            };
            fresh.push((id, revision, content));
        }
        self.previews = fresh;
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
            self.editor.set_font_note(format!(
                "{name} is not monospaced; using {}",
                fonts::EDITOR_FAMILY
            ));
        }
        self.font = fonts::EDITOR;
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        if !matches!(message, Message::Tick(_)) {
            self.now = Instant::now();
        }
        let shown = self.faces();
        match message {
            Message::Keyboard(keyboard::Event::KeyPressed {
                key,
                modified_key,
                modifiers,
                ..
            }) => {
                if let Some(key) =
                    keys::translate(&key, &modified_key, modifiers, Platform::current())
                {
                    self.last_input = self.now;
                    self.editor.handle_key(key);
                    self.sync_reveal();
                }
            }
            Message::Keyboard(_) | Message::Opened(Ok(())) | Message::Clock => {}
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
            Message::OpenLink(uri) => {
                if let Err(error) = open::that_detached(&uri) {
                    self.editor.show_message(format!("{uri}: {error}"));
                }
            }
            Message::AutoSave => self.editor.auto_save(),
            Message::FocusWindow(window) => {
                if self.focus(window) {
                    self.last_input = self.now;
                }
            }
            Message::PreviewSized(window, height) => {
                self.preview_heights.retain(|(known, _)| *known != window);
                self.preview_heights.push((window, height));
            }
            Message::Job(job, process::Event::Line(line)) => self.editor.job_output(job, &line),
            Message::Job(job, process::Event::Output(text)) => {
                self.editor.session_output(job, &text);
            }
            Message::Job(job, process::Event::Finished(outcome)) => {
                self.jobs.retain(|(id, _)| *id != job);
                self.editor.job_finished(job, &outcome);
            }
            Message::Fetched(id, result) => self.editor.fetched(id, result),
            Message::Irc(id, event) => {
                if matches!(event, net::Event::Closed(_)) {
                    self.connections.retain(|(known, _)| *known != id);
                }
                self.editor.irc_event(id, event);
            }
            Message::WeatherTick => self.editor.weather_tick(Timestamp::now()),
        }
        self.editor.prepare_display();
        self.refresh_previews();
        self.fade_theme_from(shown);
        if self.editor.should_quit() {
            self.editor.irc_farewell();
            let _ = self.pending_effects();
            for (_, connection) in self.connections.drain(..) {
                connection.wait(FAREWELL);
            }
            for (_, killer) in self.jobs.drain(..) {
                killer.kill();
            }
            return iced::exit();
        }
        self.pending_effects()
    }

    pub(super) fn pending_effects(&mut self) -> Task<Message> {
        let effects = self.editor.take_effects();
        Task::batch(effects.into_iter().map(|effect| self.perform(effect)))
    }

    fn perform(&mut self, effect: Effect) -> Task<Message> {
        match effect {
            Effect::WriteClipboard(text) => clipboard::write(text),
            Effect::ReadClipboard => clipboard::read().map(Message::Pasted),
            Effect::WritePrimary(text) if HAS_PRIMARY_SELECTION => clipboard::write_primary(text),
            Effect::ReadPrimary if HAS_PRIMARY_SELECTION => {
                clipboard::read_primary().map(Message::PastedPrimary)
            }
            Effect::WritePrimary(_) | Effect::ReadPrimary => Task::none(),
            Effect::OpenExternally(path) => Task::done(Message::Opened(open_externally(&path))),
            Effect::Spawn(job) => self.spawn(&job, false),
            Effect::StartSession(job) => self.spawn(&job, true),
            Effect::Send(job, text) => {
                let sent = self
                    .jobs
                    .iter()
                    .find(|(id, _)| *id == job)
                    .is_some_and(|(_, handle)| handle.send(&text).is_ok());
                if !sent {
                    self.editor.show_message("The process is not running");
                }
                Task::none()
            }
            Effect::Kill(job) => {
                if let Some((_, killer)) = self.jobs.iter().find(|(id, _)| *id == job) {
                    killer.kill();
                }
                Task::none()
            }
            Effect::ScrollPreview(window, scroll) => self.scroll_preview(window, scroll),
            Effect::Reconfigure => {
                let config = self.editor.config().clone();
                self.configure(&config);
                self.editor.weather_tick(Timestamp::now());
                self.pending_effects()
            }
            Effect::Fetch(id, url) => {
                Task::perform(fetch::get(url), move |result| Message::Fetched(id, result))
            }
            Effect::Connect(id, endpoint) => self.connect(id, &endpoint),
            Effect::Transmit(id, lines) => {
                if let Some((_, connection)) =
                    self.connections.iter().find(|(known, _)| *known == id)
                {
                    for line in lines {
                        connection.send(&line);
                    }
                }
                Task::none()
            }
            Effect::Disconnect(id) => {
                if let Some((_, connection)) =
                    self.connections.iter().find(|(known, _)| *known == id)
                {
                    connection.close();
                }
                Task::none()
            }
        }
    }

    fn scroll_preview(&self, window: WindowId, scroll: PreviewScroll) -> Task<Message> {
        let line = self.text_size * 1.6;
        let page = self
            .preview_heights
            .iter()
            .find(|(known, _)| *known == window)
            .map_or(line * 20.0, |(_, height)| height * 0.9);
        let id = preview_id(window);
        match preview_move(scroll, line, page) {
            PreviewMove::By(y) => operation::scroll_by(id, AbsoluteOffset { x: 0.0, y }),
            PreviewMove::To(y) => operation::snap_to(
                id,
                RelativeOffset {
                    x: None,
                    y: Some(y),
                },
            ),
        }
    }

    fn connect(&mut self, id: ConnectionId, endpoint: &Endpoint) -> Task<Message> {
        let (sender, events) = mpsc::unbounded();
        let connection = net::connect(endpoint, move |event| {
            let _ = sender.unbounded_send(event);
        });
        self.connections.push((id, connection));
        Task::run(events, move |event| Message::Irc(id, event))
    }

    fn spawn(&mut self, job: &Job, session: bool) -> Task<Message> {
        let id = job.id;
        let (sender, events) = mpsc::unbounded();
        let report = move |event| {
            let _ = sender.unbounded_send(event);
        };
        let started = if session {
            process::start_session(job, report)
        } else {
            process::spawn(job, report)
        };
        match started {
            Ok(killer) => {
                self.jobs.push((id, killer));
                Task::run(events, move |event| Message::Job(id, event))
            }
            Err(error) => Task::done(Message::Job(
                id,
                process::Event::Finished(Outcome::Failed(error.to_string())),
            )),
        }
    }
}

const HAS_PRIMARY_SELECTION: bool = cfg!(target_os = "linux");
const FAREWELL: Duration = Duration::from_secs(1);

fn open_externally(path: &Path) -> Result<(), String> {
    open::that_detached(path).map_err(|error| format!("{}: {error}", path.display()))
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum PreviewMove {
    By(f32),
    To(f32),
}

fn preview_move(scroll: PreviewScroll, line: f32, page: f32) -> PreviewMove {
    match scroll {
        PreviewScroll::Lines(count) => PreviewMove::By(units::signed_to_f32(count) * line),
        PreviewScroll::Pages(count) => PreviewMove::By(units::signed_to_f32(count) * page),
        PreviewScroll::Top => PreviewMove::To(0.0),
        PreviewScroll::Bottom => PreviewMove::To(1.0),
    }
}

fn appearance(mode: Mode) -> Option<Appearance> {
    match mode {
        Mode::Light => Some(Appearance::Light),
        Mode::Dark => Some(Appearance::Dark),
        Mode::None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_scrolls_move_by_lines_and_pages_or_snap() {
        assert_eq!(
            preview_move(PreviewScroll::Lines(2), 10.0, 100.0),
            PreviewMove::By(20.0)
        );
        assert_eq!(
            preview_move(PreviewScroll::Pages(-1), 10.0, 100.0),
            PreviewMove::By(-100.0)
        );
        assert_eq!(
            preview_move(PreviewScroll::Top, 10.0, 100.0),
            PreviewMove::To(0.0)
        );
        assert_eq!(
            preview_move(PreviewScroll::Bottom, 10.0, 100.0),
            PreviewMove::To(1.0)
        );
    }
}
