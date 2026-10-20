use std::fmt;
use std::ops::Range;

use crate::editor::Editor;
use crate::editor::prompts::minibuffer::{Choice, Completion, Purpose};
use crate::theme::Highlight;

type Offers = Box<dyn Fn(&Editor) -> Vec<Offer> + Send>;
type Then = Box<dyn FnOnce(&mut Editor, bool) + Send>;
type Answered = Box<dyn FnOnce(&mut Editor, String) + Send>;
type Chose = Box<dyn FnOnce(&mut Editor, Picked) + Send>;

pub type InputStyle = fn(&Editor, &str) -> Vec<(Range<usize>, Highlight)>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Picked {
    Offered(usize),
    Typed(String),
}

enum Done {
    Text(Answered),
    Pick(Chose),
}

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
    style: Option<InputStyle>,
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
                done: Done::Text(Box::new(done)),
                style: None,
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
                done: Done::Text(Box::new(done)),
                style: None,
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
                done: Done::Text(Box::new(done)),
                style: None,
            },
        );
    }

    pub fn prompt_pick(
        &mut self,
        prompt: &str,
        offer: impl Fn(&Editor) -> Vec<Offer> + Send + 'static,
        done: impl FnOnce(&mut Editor, Picked) + Send + 'static,
    ) {
        self.open_applet_prompt(
            prompt,
            false,
            AppletPrompt {
                offer: Some(Box::new(offer)),
                done: Done::Pick(Box::new(done)),
                style: None,
            },
        );
    }

    pub fn prompt_styled(
        &mut self,
        prompt: &str,
        style: InputStyle,
        done: impl FnOnce(&mut Editor, String) + Send + 'static,
    ) {
        self.open_applet_prompt(
            prompt,
            false,
            AppletPrompt {
                offer: None,
                done: Done::Text(Box::new(done)),
                style: Some(style),
            },
        );
    }

    pub fn minibuffer_highlights(&self) -> Vec<(Range<usize>, Highlight)> {
        let Some(minibuffer) = &self.minibuffer else {
            return Vec::new();
        };
        match minibuffer.applet.as_ref().and_then(|applet| applet.style) {
            Some(style) => style(self, &minibuffer.input().contents()),
            None => self.atlas_capture_highlights(),
        }
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

    pub(in crate::editor) fn exit_applet_prompt(&mut self, typed: bool) {
        let input = self.minibuffer_input();
        let selected = if typed {
            None
        } else {
            self.selected_completion()
        };
        let applet = self
            .minibuffer
            .take()
            .and_then(|minibuffer| minibuffer.applet);
        let Some(applet) = applet else {
            return;
        };
        match applet.done {
            Done::Text(done) => done(self, selected.map_or(input, |completion| completion.label)),
            Done::Pick(done) => {
                let picked = match selected.map(|completion| completion.choice) {
                    Some(Choice::Offered(index)) => Picked::Offered(index),
                    _ => Picked::Typed(input),
                };
                done(self, picked);
            }
        }
    }
}
