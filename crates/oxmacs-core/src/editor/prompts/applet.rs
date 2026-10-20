use std::fmt;

use crate::editor::Editor;
use crate::editor::prompts::minibuffer::{Choice, Completion, Purpose};

type Offers = Box<dyn Fn(&Editor) -> Vec<Offer> + Send>;
type Done = Box<dyn FnOnce(&mut Editor, String) + Send>;
type Then = Box<dyn FnOnce(&mut Editor, bool) + Send>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    pub label: String,
    pub note: Option<String>,
}

impl Offer {
    pub fn noted(label: impl Into<String>, note: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            note: Some(note.into()),
        }
    }
}

impl From<String> for Offer {
    fn from(label: String) -> Self {
        Self { label, note: None }
    }
}

impl From<&str> for Offer {
    fn from(label: &str) -> Self {
        Self::from(label.to_owned())
    }
}

pub(in crate::editor) struct Answer(Then);

impl Answer {
    pub(in crate::editor) fn give(self, editor: &mut Editor, yes: bool) {
        (self.0)(editor, yes);
    }
}

impl fmt::Debug for Answer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Answer")
    }
}

pub(in crate::editor) struct AppletPrompt {
    offer: Option<Offers>,
    done: Done,
}

impl fmt::Debug for AppletPrompt {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AppletPrompt")
            .field("offers", &self.offer.is_some())
            .finish_non_exhaustive()
    }
}

impl Editor {
    pub fn prompt(
        &mut self,
        prompt: &str,
        done: impl FnOnce(&mut Editor, String) + Send + 'static,
    ) {
        self.open_applet_prompt(
            prompt,
            false,
            AppletPrompt {
                offer: None,
                done: Box::new(done),
            },
        );
    }

    pub fn prompt_with_text(
        &mut self,
        prompt: &str,
        text: &str,
        done: impl FnOnce(&mut Editor, String) + Send + 'static,
    ) {
        self.prompt(prompt, done);
        if self
            .minibuffer
            .as_ref()
            .is_some_and(|minibuffer| minibuffer.applet.is_some())
        {
            self.set_minibuffer_input(text);
        }
    }

    pub fn prompt_secret(
        &mut self,
        prompt: &str,
        done: impl FnOnce(&mut Editor, String) + Send + 'static,
    ) {
        self.open_applet_prompt(
            prompt,
            true,
            AppletPrompt {
                offer: None,
                done: Box::new(done),
            },
        );
    }

    pub fn prompt_with(
        &mut self,
        prompt: &str,
        offer: impl Fn(&Editor) -> Vec<Offer> + Send + 'static,
        done: impl FnOnce(&mut Editor, String) + Send + 'static,
    ) {
        self.open_applet_prompt(
            prompt,
            false,
            AppletPrompt {
                offer: Some(Box::new(offer)),
                done: Box::new(done),
            },
        );
    }

    pub fn ask(&mut self, prompt: &str, then: impl FnOnce(&mut Editor, bool) + Send + 'static) {
        self.ask_for_applet(prompt, Answer(Box::new(then)));
    }

    fn open_applet_prompt(&mut self, prompt: &str, secret: bool, applet: AppletPrompt) {
        let completes = applet.offer.is_some();
        if self.open_minibuffer(prompt, Purpose::Applet { completes, secret })
            && let Some(minibuffer) = &mut self.minibuffer
        {
            minibuffer.applet = Some(applet);
        }
    }

    pub(in crate::editor) fn applet_completions(
        &self,
        matches: impl Fn(&str) -> bool,
    ) -> Vec<Completion> {
        let Some(offer) = self
            .minibuffer
            .as_ref()
            .and_then(|minibuffer| minibuffer.applet.as_ref())
            .and_then(|applet| applet.offer.as_ref())
        else {
            return Vec::new();
        };
        offer(self)
            .into_iter()
            .enumerate()
            .filter(|(_, offer)| matches(&offer.label))
            .map(|(index, offer)| Completion {
                label: offer.label,
                annotation: offer.note,
                summary: None,
                choice: Choice::Offered(index),
            })
            .collect()
    }

    pub(in crate::editor) fn exit_applet_prompt(&mut self) {
        let value = self
            .selected_completion()
            .map_or_else(|| self.minibuffer_input(), |completion| completion.label);
        let applet = self
            .minibuffer
            .take()
            .and_then(|minibuffer| minibuffer.applet);
        if let Some(applet) = applet {
            (applet.done)(self, value);
        }
    }
}
