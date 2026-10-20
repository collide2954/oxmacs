use oxmacs_core::applet::AppletBuffer;
use oxmacs_core::command::ScopeId;
use oxmacs_core::editor::{Editor, WindowId};
use oxmacs_core::text::BufferId;

const DEFAULT_LINE: f32 = 24.0;
const PAGE_LINES: f32 = 20.0;
const PAGE_SHARE: f32 = 0.9;

#[derive(Debug)]
pub(crate) struct PreviewView {
    scope: ScopeId,
    pub(crate) source: BufferId,
}

impl AppletBuffer for PreviewView {
    fn scope(&self) -> ScopeId {
        self.scope
    }
}

#[derive(Debug)]
pub(crate) struct PreviewState {
    scope: ScopeId,
    sizes: Vec<(WindowId, f32, f32)>,
}

impl PreviewState {
    pub(crate) fn new(scope: ScopeId) -> Self {
        Self {
            scope,
            sizes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PreviewSized {
    pub window: WindowId,
    pub height: f32,
    pub line: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenLink(pub String);

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Move {
    By(f32),
    To(f32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollPreview {
    pub window: WindowId,
    pub to: Move,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    Lines(i8),
    Pages(i8),
    Top,
    Bottom,
}

pub(crate) fn movement(step: Step, line: f32, page: f32) -> Move {
    match step {
        Step::Lines(count) => Move::By(f32::from(count) * line),
        Step::Pages(count) => Move::By(f32::from(count) * page),
        Step::Top => Move::To(0.0),
        Step::Bottom => Move::To(1.0),
    }
}

pub(crate) fn toggle(editor: &mut Editor) {
    let source = editor.buffer().id();
    let existing = editor.find_applet_buffer::<PreviewView>(|view| view.source == source);
    if existing.is_some_and(|preview| editor.hide_buffer(preview)) {
        return;
    }
    let origin = editor.selected_window();
    let Some(id) = existing.or_else(|| add_preview(editor, source)) else {
        return;
    };
    editor.show_buffer_beside(id);
    editor.select_window(origin);
}

fn add_preview(editor: &mut Editor, source: BufferId) -> Option<BufferId> {
    let scope = editor.applet_state::<PreviewState>()?.scope;
    let name = format!("*Preview: {}*", editor.buffer().name());
    Some(editor.add_applet_buffer_with(&name, |buffer| {
        buffer.set_read_only(true);
        PreviewView { scope, source }
    }))
}

pub(crate) fn scroll(editor: &mut Editor, step: Step) {
    let window = editor.selected_window();
    let (line, page) = editor
        .applet_state::<PreviewState>()
        .and_then(|state| state.sizes.iter().find(|(known, ..)| *known == window))
        .map_or(
            (DEFAULT_LINE, DEFAULT_LINE * PAGE_LINES),
            |(_, height, line)| (*line, height * PAGE_SHARE),
        );
    editor.request(ScrollPreview {
        window,
        to: movement(step, line, page),
    });
}

pub(crate) fn sized(editor: &mut Editor, sized: &PreviewSized) {
    if let Some(state) = editor.applet_state_mut::<PreviewState>() {
        state.sizes.retain(|(known, ..)| *known != sized.window);
        state.sizes.push((sized.window, sized.height, sized.line));
    }
}

#[cfg(test)]
mod tests {
    use oxmacs_core::applet::Payload;
    use oxmacs_core::editor::{Editor, Effect};
    use oxmacs_core::text::Buffer;

    use super::*;
    use crate::testing::{editor_with, press};

    fn scrolls(editor: &mut Editor) -> Vec<ScrollPreview> {
        editor
            .take_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                Effect::Applet(payload) => payload.get::<ScrollPreview>().copied(),
                _ => None,
            })
            .collect()
    }

    fn source(editor: &Editor, window: WindowId) -> Option<String> {
        let pane = editor.pane_of(window)?;
        let view = pane.applet_buffer::<PreviewView>()?;
        editor.buffer_by_id(view.source).map(Buffer::contents)
    }

    #[test]
    fn the_preview_opens_beside_its_source() {
        let mut editor = editor_with("# Title\n", 0);
        let origin = editor.selected_window();

        press(&mut editor, "C-c C-c p");

        assert_eq!(editor.tiling().windows().len(), 2);
        assert_eq!(editor.selected_window(), origin);
        let other = editor.tiling().windows()[1];
        let pane = editor.pane_of(other).unwrap();
        assert_eq!(pane.buffer().name(), "*Preview: t.md*");
        assert_eq!(source(&editor, other).as_deref(), Some("# Title\n"));
    }

    #[test]
    fn the_preview_follows_edits_to_its_source() {
        let mut editor = editor_with("# Title\n", 0);
        press(&mut editor, "C-c C-c p M-> x");

        let other = editor.tiling().windows()[1];
        assert_eq!(source(&editor, other).as_deref(), Some("# Title\nx"));
    }

    #[test]
    fn previewing_again_closes_it() {
        let mut editor = editor_with("# Title\n", 0);
        let origin = editor.selected_window();

        press(&mut editor, "C-c C-c p C-c C-c p");

        assert_eq!(editor.tiling().windows(), [origin]);
    }

    fn previewed(text: &str) -> Editor {
        let mut editor = editor_with(text, 0);
        press(&mut editor, "C-c C-c p");
        editor.take_effects();
        editor
    }

    #[test]
    fn keys_in_the_preview_scroll_it_by_its_size() {
        let mut editor = previewed("# Title\n\ntext\n");
        press(&mut editor, "C-x o");
        let preview = editor.selected_window();
        editor.deliver(&Payload::new(PreviewSized {
            window: preview,
            height: 500.0,
            line: 20.0,
        }));
        editor.take_effects();

        for (keys, to) in [
            ("C-n", Move::By(20.0)),
            ("<up>", Move::By(-20.0)),
            ("C-v", Move::By(450.0)),
            ("M-v", Move::By(-450.0)),
            ("SPC", Move::By(450.0)),
            ("DEL", Move::By(-450.0)),
            ("M->", Move::To(1.0)),
            ("M-<", Move::To(0.0)),
        ] {
            press(&mut editor, keys);
            assert_eq!(
                scrolls(&mut editor),
                [ScrollPreview {
                    window: preview,
                    to
                }],
                "{keys}"
            );
        }
    }

    #[test]
    fn unmeasured_previews_scroll_by_a_default_page() {
        let mut editor = previewed("# A\n");
        press(&mut editor, "C-x o");

        press(&mut editor, "SPC");

        assert_eq!(scrolls(&mut editor)[0].to, Move::By(480.0));
    }

    #[test]
    fn q_closes_the_preview_window_view() {
        let mut editor = previewed("# A\n");
        press(&mut editor, "C-x o");

        press(&mut editor, "q");

        assert_ne!(editor.buffer().name(), "*Preview: t.md*");
    }

    #[test]
    fn an_existing_split_hosts_the_preview() {
        let mut editor = editor_with("# Title\n", 0);

        press(&mut editor, "C-x 3 C-c C-c p");

        assert_eq!(editor.tiling().windows().len(), 2);
    }

    #[test]
    fn links_in_the_preview_open_outside() {
        let mut editor = previewed("[site](https://example.org)\n");

        editor.deliver(&Payload::new(OpenLink("https://example.org".to_owned())));

        assert_eq!(
            editor.take_effects(),
            [Effect::OpenExternally("https://example.org".into())]
        );
    }

    #[test]
    fn moves_are_lines_pages_or_ends() {
        assert_eq!(movement(Step::Lines(2), 10.0, 100.0), Move::By(20.0));
        assert_eq!(movement(Step::Pages(-1), 10.0, 100.0), Move::By(-100.0));
        assert_eq!(movement(Step::Top, 10.0, 100.0), Move::To(0.0));
        assert_eq!(movement(Step::Bottom, 10.0, 100.0), Move::To(1.0));
    }
}
