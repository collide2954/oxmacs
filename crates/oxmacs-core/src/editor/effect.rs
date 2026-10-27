use std::path::PathBuf;
use std::{fmt, mem};

use crate::editor::{Editor, Job, JobId, ServerId};
use crate::external::lsp::Launch;
use crate::external::managed::Install;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FetchId(pub u64);

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
    Reconfigure,
    Fetch(FetchId, String),
    InstallTool(Box<Install>),
    StartServer(ServerId, Launch),
    ToServer(ServerId, String),
    Notify { title: String, body: String },
    Fullscreen(bool),
}

impl Editor {
    pub fn take_effects(&mut self) -> Vec<Effect> {
        self.sync_servers();
        self.start_due_lint();
        mem::take(&mut self.effects)
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
