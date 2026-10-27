pub(super) mod callbacks;
pub(super) mod input_line;
pub(super) mod minibuffer;
pub(super) mod popup;
pub(super) mod question;
pub(super) mod transient;

#[derive(Debug, Default)]
pub(in crate::editor) struct Prompts {
    minibuffer: Option<minibuffer::Minibuffer>,
    popup: Option<popup::Popup>,
    transients: transient::Transients,
    question: Option<question::Question>,
    answer: Option<callbacks::Answer>,
}

impl Prompts {
    pub(in crate::editor) fn quiet(&self) -> bool {
        self.minibuffer.is_none() && self.popup.is_none() && self.question.is_none()
    }
}
