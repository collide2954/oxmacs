use std::fmt;

use crate::editor::Editor;
use crate::editor::prompts::minibuffer::{Choice, Completion, Purpose};

type Offer = Box<dyn Fn(&Editor) -> Vec<String> + Send>;
type Done = Box<dyn FnOnce(&mut Editor, String) + Send>;
type Then = Box<dyn FnOnce(&mut Editor, bool) + Send>;

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
    offer: Option<Offer>,
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
            AppletPrompt {
                offer: None,
                done: Box::new(done),
            },
        );
    }

    pub fn prompt_with(
        &mut self,
        prompt: &str,
        offer: impl Fn(&Editor) -> Vec<String> + Send + 'static,
        done: impl FnOnce(&mut Editor, String) + Send + 'static,
    ) {
        self.open_applet_prompt(
            prompt,
            AppletPrompt {
                offer: Some(Box::new(offer)),
                done: Box::new(done),
            },
        );
    }

    pub fn ask(&mut self, prompt: &str, then: impl FnOnce(&mut Editor, bool) + Send + 'static) {
        self.ask_for_applet(prompt, Answer(Box::new(then)));
    }

    fn open_applet_prompt(&mut self, prompt: &str, applet: AppletPrompt) {
        let completes = applet.offer.is_some();
        if self.open_minibuffer(prompt, Purpose::Applet { completes })
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
            .filter(|(_, label)| matches(label))
            .map(|(index, label)| Completion {
                label,
                annotation: None,
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
