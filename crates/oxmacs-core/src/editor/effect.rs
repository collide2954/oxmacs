use std::mem;
use std::path::PathBuf;

use crate::applet::Payload;
use crate::editor::{Editor, Job, JobId, PreviewScroll, WindowId};
use crate::external::managed::Install;
use crate::external::net::Endpoint;
use crate::external::tools::Tool;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FetchId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnectionId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    WriteClipboard(String),
    ReadClipboard,
    WritePrimary(String),
    ReadPrimary,
    OpenExternally(PathBuf),
    Spawn(Job),
    StartSession(Job),
    Send(JobId, String),
    Kill(JobId),
    Reconfigure,
    ScrollPreview(WindowId, PreviewScroll),
    Fetch(FetchId, String),
    Connect(ConnectionId, Endpoint),
    Transmit(ConnectionId, Vec<String>),
    Disconnect(ConnectionId),
    LaunchDaemon,
    ProbeTools(Vec<Tool>),
    InstallTool(Install),
    Applet(Payload),
}

impl Editor {
    pub fn take_effects(&mut self) -> Vec<Effect> {
        self.start_due_lint();
        mem::take(&mut self.effects)
    }

    pub fn request<T: std::any::Any + Send + Sync>(&mut self, effect: T) {
        self.effects.push(Effect::Applet(Payload::new(effect)));
    }

    pub fn on_event<T: std::any::Any + Send + Sync>(&mut self, handler: fn(&mut Editor, &T)) {
        self.event_handlers.add(handler);
    }

    pub fn deliver(&mut self, event: &Payload) {
        if let Some(handler) = self.event_handlers.find(event) {
            handler(self, event);
        }
    }

    pub fn fetched(&mut self, id: FetchId, result: Result<String, String>) {
        if !self.tool_release_fetched(id, &result) {
            self.weather_fetched(id, result);
        }
    }
}
