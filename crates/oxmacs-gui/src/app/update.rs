use std::path::{Path, PathBuf};
use std::thread;

use iced::font::Family;
use iced::futures::channel::{mpsc, oneshot};
use iced::theme::Mode;
use iced::time::{Duration, Instant};
use iced::widget::image;
use iced::{Task, clipboard, keyboard};
use jiff::Timestamp;
use oxmacs_core::editor::{ConnectionId, Effect, Job, Outcome, ServerId, WindowId};
use oxmacs_core::external::lsp;
use oxmacs_core::external::net::{self, Endpoint};
use oxmacs_core::external::notify;
use oxmacs_core::external::process;
use oxmacs_core::external::tools::{self, Probe, Tool};
use oxmacs_core::theme::{Appearance, Theme as Faces};

use oxmacs_ox::takeover;

use super::{App, Message, transition};
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
        if let Some(since) = self.rested_since() {
            self.ox.idle(&mut self.editor, since, self.now);
        }
    }

    fn end_takeover(&mut self) {
        self.last_input = self.now;
        self.ox.end_take_over(&mut self.editor);
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
            Message::Keyboard(keyboard::Event::KeyPressed { .. }) | Message::EndTakeover
                if takeover::current(&self.editor).is_some() =>
            {
                self.end_takeover();
            }
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
            Message::Keyboard(_)
            | Message::Opened(Ok(()))
            | Message::Clock
            | Message::OxTick
            | Message::EndTakeover => {}
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
            Message::AutoSave => self.editor.auto_save(),
            Message::AutoComplete => self.editor.auto_complete(),
            Message::Signature => self.editor.show_signature(),
            Message::ProcessTick => self.sample_processes(),
            Message::FocusWindow(window) => {
                if self.focus(window) {
                    self.last_input = self.now;
                }
            }
            Message::PlotLoaded(path, allocation) => self.plot_loaded(path, allocation),
            Message::PickPlot(window, position) => {
                self.focus(window);
                self.editor.pick_plot_in(window, position);
            }
            Message::PlotSized(window, size) => self.plot_sized(window, size),
            Message::Job(job, process::Event::Line(line)) => self.editor.job_output(job, &line),
            Message::Job(job, process::Event::Output(text)) => {
                self.editor.session_output(job, &text);
            }
            Message::Job(job, process::Event::Finished(outcome)) => {
                self.jobs.retain(|(id, _)| *id != job);
                self.editor.job_finished(job, &outcome);
            }
            Message::Fetched(id, result) => self.editor.fetched(id, result),
            Message::ToolsProbed(probes) => self.editor.tools_probed(probes),
            Message::ToolInstalled(tool, result) => self.editor.tool_installed(tool, result),
            Message::Connection(id, event) => {
                if matches!(event, net::Event::Closed(_)) {
                    self.connections.retain(|(known, _)| *known != id);
                }
                self.editor.connection_event(id, event);
            }
            Message::Server(id, event) => {
                if matches!(event, lsp::Event::Exited(_)) {
                    self.servers.retain(|(known, _)| *known != id);
                }
                self.editor.server_event(id, event);
            }
            Message::OxPress(buffer, slot) => self.ox.press(&mut self.editor, buffer, slot),
            Message::TakeoverSized(size) => self.takeover_size = Some(size),
            Message::Focus(focused) => self.focus_changed(focused),
            Message::Idle => self.idle(),
        }
        self.ox.tick(&mut self.editor);
        self.settle();
        let plots = self.load_plots();
        self.fade_theme_from(shown);
        if self.editor.should_quit() {
            self.quit();
            return iced::exit();
        }
        Task::batch([self.pending_effects(), plots])
    }

    fn quit(&mut self) {
        self.editor.quitting();
        let _ = self.pending_effects();
        for (_, connection) in self.connections.drain(..) {
            connection.wait(FAREWELL);
        }
        for (_, killer) in self.jobs.drain(..) {
            killer.kill();
        }
        for (_, server) in self.servers.drain(..) {
            let _ = server.send(EXIT);
            server.kill();
        }
    }

    fn settle(&mut self) {
        self.editor.prepare_display();
    }

    fn sample_processes(&mut self) {
        let samples = self.usage.sample(&self.editor.process_pids());
        self.editor.processes_tick(Timestamp::now(), &samples);
    }

    fn plot_sized(&mut self, window: WindowId, size: iced::Size) {
        let pixels = |extent: f32| u32::try_from(units::floor_to_usize(extent)).unwrap_or(u32::MAX);
        self.editor
            .plots_sized(window, pixels(size.width), pixels(size.height));
    }

    fn load_plots(&mut self) -> Task<Message> {
        let fresh: Vec<PathBuf> = self
            .editor
            .tiling()
            .windows()
            .into_iter()
            .filter_map(|window| self.editor.pane_of(window)?.plot())
            .map(|plot| plot.path)
            .filter(|path| {
                !self.plot_loading.contains(path)
                    && !self.plot_images.iter().any(|(known, _)| known == path)
            })
            .collect();
        self.plot_loading.extend(fresh.iter().cloned());
        Task::batch(fresh.into_iter().map(|path| {
            image::allocate(image::Handle::from_path(&path))
                .map(move |allocation| Message::PlotLoaded(path.clone(), allocation.ok()))
        }))
    }

    fn plot_loaded(&mut self, path: PathBuf, allocation: Option<image::Allocation>) {
        self.plot_loading.retain(|known| *known != path);
        if let Some(allocation) = allocation {
            self.plot_images.push((path, allocation));
        }
        let excess = self.plot_images.len().saturating_sub(KEPT_PLOTS);
        self.plot_images.drain(..excess);
    }

    pub(super) fn plot_image(&self, path: &Path) -> image::Handle {
        shown_plot(&self.plot_images, path).map_or_else(
            || image::Handle::from_path(path),
            |allocation| allocation.handle().clone(),
        )
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
            Effect::Reconfigure => {
                let config = self.editor.config().clone();
                self.configure(&config);
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
            Effect::ProbeTools(tools) => Task::perform(probe_tools(tools), Message::ToolsProbed),
            Effect::InstallTool(install) => {
                let tool = install.tool;
                Task::perform(fetch::install(*install), move |result| {
                    Message::ToolInstalled(tool, result)
                })
            }
            Effect::StartServer(id, launch) => self.start_server(id, &launch),
            Effect::ToServer(id, body) => {
                if let Some((_, server)) = self.servers.iter().find(|(known, _)| *known == id) {
                    let _ = server.send(&body);
                }
                Task::none()
            }
            Effect::StopServer(id) => {
                if let Some(at) = self.servers.iter().position(|(known, _)| *known == id) {
                    let (_, server) = self.servers.remove(at);
                    server.kill();
                }
                Task::none()
            }
            Effect::Notify { title, body } => {
                notify::send(&title, &body);
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

    fn connect(&mut self, id: ConnectionId, endpoint: &Endpoint) -> Task<Message> {
        let (sender, events) = mpsc::unbounded();
        let connection = net::connect(endpoint, move |event| {
            let _ = sender.unbounded_send(event);
        });
        self.connections.push((id, connection));
        Task::run(events, move |event| Message::Connection(id, event))
    }

    fn start_server(&mut self, id: ServerId, launch: &lsp::Launch) -> Task<Message> {
        let (sender, events) = mpsc::unbounded();
        let started = lsp::start(launch, move |event| {
            let _ = sender.unbounded_send(event);
        });
        match started {
            Ok(server) => {
                self.servers.push((id, server));
                Task::run(events, move |event| Message::Server(id, event))
            }
            Err(error) => Task::done(Message::Server(
                id,
                lsp::Event::Exited(Some(error.to_string())),
            )),
        }
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
                self.editor.job_started(id, killer.pid());
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
const EXIT: &str = r#"{"jsonrpc":"2.0","method":"exit"}"#;
const KEPT_PLOTS: usize = 8;

pub fn shown_plot<'a, T>(loaded: &'a [(PathBuf, T)], path: &Path) -> Option<&'a T> {
    loaded
        .iter()
        .rev()
        .find(|(known, _)| known == path)
        .or_else(|| loaded.last())
        .map(|(_, item)| item)
}
const FAREWELL: Duration = Duration::from_secs(1);

async fn probe_tools(tools: Vec<Tool>) -> Vec<(Tool, Probe)> {
    let (sender, receiver) = oneshot::channel();
    thread::spawn(move || sender.send(tools::probe_all(tools)));
    receiver.await.unwrap_or_default()
}

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
