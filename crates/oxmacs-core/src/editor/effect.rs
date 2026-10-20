use std::path::PathBuf;
use std::{fmt, mem};

use crate::applet::Payload;
use crate::editor::{Editor, Job, JobId, PreviewScroll, WindowId};
use crate::external::managed::Install;
use crate::external::net::Endpoint;
use crate::external::tools::Tool;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FetchId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnectionId(pub u64);

type Then = Box<dyn FnOnce(&mut Editor, Result<String, String>) + Send>;

#[derive(Default)]
pub(super) struct Fetches(Vec<(FetchId, Then)>);

impl fmt::Debug for Fetches {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Fetches")
            .field("waiting", &self.0.len())
            .finish()
    }
}

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

    pub fn fetch(
        &mut self,
        url: impl Into<String>,
        then: impl FnOnce(&mut Editor, Result<String, String>) + Send + 'static,
    ) -> FetchId {
        let id = FetchId(self.next_fetch);
        self.next_fetch += 1;
        self.fetches.0.push((id, Box::new(then)));
        self.effects.push(Effect::Fetch(id, url.into()));
        id
    }

    pub fn fetched(&mut self, id: FetchId, result: Result<String, String>) {
        if let Some(at) = self.fetches.0.iter().position(|(known, _)| *known == id) {
            let (_, then) = self.fetches.0.swap_remove(at);
            then(self, result);
            return;
        }
        self.tool_release_fetched(id, &result);
    }
}
