use std::fmt;
use std::ops::Range;
use std::path::PathBuf;

use crate::editor::Editor;
use crate::editor::prompts::minibuffer::{Choice, Completion, Purpose};
use crate::theme::Highlight;

const YES_NO: &[char] = &['y', 'n'];

type Offers = Box<dyn Fn(&Editor) -> Vec<Offer> + Send>;
type Then = Box<dyn FnOnce(&mut Editor, bool) + Send>;
type Answered = Box<dyn FnOnce(&mut Editor, String) + Send>;
type Chose = Box<dyn FnOnce(&mut Editor, Picked) + Send>;
type Located = Box<dyn FnOnce(&mut Editor, PathBuf) + Send>;

pub type InputStyle = fn(&Editor, &str) -> Vec<(Range<usize>, Highlight)>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Picked {
    Offered(usize),
    Typed(String),
}

enum Done {
    Text(Answered),
    Pick(Chose),
    Path(Located),
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

pub(in crate::editor) enum Answer {
    YesNo(Then),
}

impl Answer {
    pub(in crate::editor) fn give(self, editor: &mut Editor, answer: char) {
        match self {
            Self::YesNo(then) => then(editor, answer == 'y'),
        }
    }
}

impl fmt::Debug for Answer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Answer")
    }
}

pub(in crate::editor) struct CallbackPrompt {
    offer: Option<Offers>,
    done: Done,
    style: Option<InputStyle>,
}

impl fmt::Debug for CallbackPrompt {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CallbackPrompt")
            .field("offers", &self.offer.is_some())
            .finish_non_exhaustive()
    }
}

impl CallbackPrompt {
    pub(in crate::editor) fn answer_path(self, editor: &mut Editor, path: PathBuf) {
        match self.done {
            Done::Path(done) => done(editor, path),
            Done::Text(done) => done(editor, path.display().to_string()),
            Done::Pick(done) => done(editor, Picked::Typed(path.display().to_string())),
        }
    }
}

impl Editor {
    pub fn prompt(
        &mut self,
        prompt: &str,
        done: impl FnOnce(&mut Editor, String) + Send + 'static,
    ) {
        self.open_callback_prompt(
            prompt,
            false,
            CallbackPrompt {
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
            .prompts
            .minibuffer()
            .is_some_and(|minibuffer| minibuffer.callback.is_some())
        {
            self.prompts.set_input(text);
        }
    }

    pub fn prompt_secret(
        &mut self,
        prompt: &str,
        done: impl FnOnce(&mut Editor, String) + Send + 'static,
    ) {
        self.open_callback_prompt(
            prompt,
            true,
            CallbackPrompt {
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
        self.open_callback_prompt(
            prompt,
            false,
            CallbackPrompt {
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
        self.open_callback_prompt(
            prompt,
            false,
            CallbackPrompt {
                offer: Some(Box::new(offer)),
                done: Done::Pick(Box::new(done)),
                style: None,
            },
        );
    }

    pub fn prompt_path(
        &mut self,
        prompt: &str,
        directories: bool,
        done: impl FnOnce(&mut Editor, PathBuf) + Send + 'static,
    ) {
        if self.open_file_prompt(prompt, Purpose::CallbackPath { directories })
            && let Some(minibuffer) = self.prompts.minibuffer_mut()
        {
            minibuffer.callback = Some(CallbackPrompt {
                offer: None,
                done: Done::Path(Box::new(done)),
                style: None,
            });
        }
    }

    pub fn prompt_path_with(
        &mut self,
        prompt: &str,
        directories: bool,
        name: &str,
        done: impl FnOnce(&mut Editor, PathBuf) + Send + 'static,
    ) {
        self.prompt_path(prompt, directories, done);
        if self.prompts.reading() && !name.is_empty() {
            let start = self.prompts.input();
            self.prompts.set_input(&format!("{start}{name}"));
            self.refresh_minibuffer();
        }
    }

    #[cfg(test)]
    pub fn prompt_styled(
        &mut self,
        prompt: &str,
        style: InputStyle,
        done: impl FnOnce(&mut Editor, String) + Send + 'static,
    ) {
        self.open_callback_prompt(
            prompt,
            false,
            CallbackPrompt {
                offer: None,
                done: Done::Text(Box::new(done)),
                style: Some(style),
            },
        );
    }

    pub fn minibuffer_highlights(&self) -> Vec<(Range<usize>, Highlight)> {
        let Some(minibuffer) = self.prompts.minibuffer() else {
            return Vec::new();
        };
        minibuffer
            .callback
            .as_ref()
            .and_then(|callback| callback.style)
            .map_or_else(Vec::new, |style| {
                style(self, &minibuffer.input().contents())
            })
    }

    pub fn ask(&mut self, prompt: &str, then: impl FnOnce(&mut Editor, bool) + Send + 'static) {
        self.ask_for_callback(
            prompt,
            "y or n",
            YES_NO,
            false,
            Answer::YesNo(Box::new(then)),
        );
    }

    #[cfg(test)]
    pub fn ask_typed(
        &mut self,
        prompt: &str,
        then: impl FnOnce(&mut Editor, bool) + Send + 'static,
    ) {
        self.ask_for_callback(
            prompt,
            "yes or no",
            YES_NO,
            true,
            Answer::YesNo(Box::new(then)),
        );
    }

    fn open_callback_prompt(&mut self, prompt: &str, secret: bool, callback: CallbackPrompt) {
        let completes = callback.offer.is_some();
        if self.open_minibuffer(prompt, Purpose::Callback { completes, secret })
            && let Some(minibuffer) = self.prompts.minibuffer_mut()
        {
            minibuffer.callback = Some(callback);
        }
    }

    pub(in crate::editor) fn callback_completions(
        &self,
        matches: impl Fn(&str) -> bool,
    ) -> Vec<Completion> {
        let Some(offer) = self
            .prompts
            .minibuffer()
            .and_then(|minibuffer| minibuffer.callback.as_ref())
            .and_then(|callback| callback.offer.as_ref())
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

    pub(in crate::editor) fn exit_callback_prompt(&mut self, typed: bool) {
        let input = self.prompts.input();
        let selected = if typed {
            None
        } else {
            self.selected_completion()
        };
        let callback = self
            .prompts
            .done_reading()
            .and_then(|minibuffer| minibuffer.callback);
        let Some(callback) = callback else {
            return;
        };
        match callback.done {
            Done::Text(done) => done(self, selected.map_or(input, |completion| completion.label)),
            Done::Path(done) => done(self, PathBuf::from(input)),
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
