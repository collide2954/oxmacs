mod editing;
mod preview;

use std::fmt;

use iced::time::Instant;
use iced::widget::scrollable::{AbsoluteOffset, RelativeOffset};
use iced::widget::{self, container, markdown, operation, scrollable, sensor};
use iced::{Element, Length, Task};
use oxmacs_core::applet::{Applet, Payload};
use oxmacs_core::command::{Command, CommandId, CommandSpec, Scope, ScopeId};
use oxmacs_core::editor::{Editor, Pane, WindowId};
use oxmacs_core::syntax::{Feature, Grammar, Indentation, Language, LanguageSpec, Outline};
use oxmacs_core::text::{Buffer, BufferId};
use oxmacs_gui::applet::{Context, GuiApplet};
use tree_sitter::Node;

pub use preview::{Move, OpenLink, PreviewSized, ScrollPreview};
use preview::{PreviewState, PreviewView, Step};

const LINE_HEIGHT: f32 = 1.6;
const PADDING: u16 = 12;

static MARKDOWN_GRAMMAR: Grammar = Grammar::new(
    "markdown",
    || tree_sitter_md::LANGUAGE.into(),
    include_str!("../queries/highlights.scm"),
    include_str!("../queries/injections.scm"),
);

static INLINE_GRAMMAR: Grammar = Grammar::new(
    "markdown_inline",
    || tree_sitter_md::INLINE_LANGUAGE.into(),
    include_str!("../queries/inline.scm"),
    "",
);

static SPEC: LanguageSpec = LanguageSpec {
    name: "Markdown",
    extensions: &["md", "markdown"],
    file_names: &[],
    aliases: &[],
    line_comment: None,
    pairs: &[],
    indentation: Indentation::KEEP,
    grammar: Some(&MARKDOWN_GRAMMAR),
    grammars: &[&INLINE_GRAMMAR],
    outline: Outline {
        item: heading,
        context: |_| false,
        defun: |node| node.kind() == "section",
        sections: false,
        statements: &[],
    },
    features: &[
        Feature::Highlighting,
        Feature::Indentation,
        Feature::Motion,
        Feature::Items,
        Feature::Preview,
    ],
};

pub const LANGUAGE: Language = Language::new(&SPEC);

fn heading(node: Node<'_>, buffer: &Buffer) -> Option<String> {
    matches!(node.kind(), "atx_heading" | "setext_heading").then(|| {
        let line = buffer.line_of(buffer.byte_to_char(node.start_byte()));
        buffer.line_text(line).trim().to_owned()
    })
}

struct Spec {
    name: &'static str,
    summary: &'static str,
    help: Option<&'static str>,
    keys: &'static [&'static str],
    run: fn(&mut Editor),
}

const GLOBAL: &[Spec] = &[Spec {
    name: "markdown-mode",
    summary: "Edit the buffer as Markdown",
    help: None,
    keys: &[],
    run: |editor| editor.set_major_mode(Some(LANGUAGE)),
}];

const MARKDOWN: &[Spec] = &[
    Spec {
        name: "markdown-insert-list-item",
        summary: "Continue the list with a new item",
        help: None,
        keys: &["M-RET"],
        run: editing::insert_list_item,
    },
    Spec {
        name: "markdown-promote",
        summary: "Raise the heading or list item one level",
        help: None,
        keys: &["M-<left>", "C-c C--"],
        run: |editor| editing::shift(editor, false),
    },
    Spec {
        name: "markdown-demote",
        summary: "Lower the heading or list item one level",
        help: None,
        keys: &["M-<right>", "C-c C-="],
        run: |editor| editing::shift(editor, true),
    },
    Spec {
        name: "markdown-preview",
        summary: "Show or hide a rendered preview beside the text",
        help: Some(
            "The preview follows your edits. In the preview, SPC and DEL scroll by a screen and q closes it.",
        ),
        keys: &["C-c C-c p"],
        run: preview::toggle,
    },
];

const PREVIEWS: &[Spec] = &[
    Spec {
        name: "markdown-preview-line-down",
        summary: "Scroll the preview down a line",
        help: None,
        keys: &["C-n", "<down>"],
        run: |editor| preview::scroll(editor, Step::Lines(1)),
    },
    Spec {
        name: "markdown-preview-line-up",
        summary: "Scroll the preview up a line",
        help: None,
        keys: &["C-p", "<up>"],
        run: |editor| preview::scroll(editor, Step::Lines(-1)),
    },
    Spec {
        name: "markdown-preview-page-down",
        summary: "Scroll the preview down a screen",
        help: None,
        keys: &["SPC", "C-v", "<next>"],
        run: |editor| preview::scroll(editor, Step::Pages(1)),
    },
    Spec {
        name: "markdown-preview-page-up",
        summary: "Scroll the preview up a screen",
        help: None,
        keys: &["DEL", "M-v", "<prior>"],
        run: |editor| preview::scroll(editor, Step::Pages(-1)),
    },
    Spec {
        name: "markdown-preview-top",
        summary: "Scroll to the top of the preview",
        help: None,
        keys: &["M-<", "C-<home>"],
        run: |editor| preview::scroll(editor, Step::Top),
    },
    Spec {
        name: "markdown-preview-bottom",
        summary: "Scroll to the bottom of the preview",
        help: None,
        keys: &["M->", "C-<end>"],
        run: |editor| preview::scroll(editor, Step::Bottom),
    },
];

fn register(editor: &mut Editor, scope: ScopeId, specs: &[Spec]) -> Vec<CommandId> {
    let mut added = Vec::new();
    for spec in specs {
        let command = editor.add_command(CommandSpec {
            name: spec.name,
            summary: spec.summary,
            help: spec.help,
            scope,
            run: spec.run,
        });
        for keys in spec.keys {
            editor.bind(scope, keys, command).ok();
        }
        added.push(command);
    }
    added
}

fn preview_id(window: WindowId) -> widget::Id {
    widget::Id::from(format!("preview-{window:?}"))
}

#[derive(Default)]
pub struct Markdown {
    previews: Vec<(BufferId, u64, markdown::Content)>,
}

impl fmt::Debug for Markdown {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Markdown")
            .field("previews", &self.previews.len())
            .finish()
    }
}

impl Applet for Markdown {
    fn name(&self) -> &'static str {
        "markdown"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn install(&self, editor: &mut Editor) {
        let markdown = editor.add_scope("Markdown buffers");
        let previews = editor.add_scope("previews");
        editor.set_applet_state(PreviewState::new(previews));
        register(editor, Scope::Global.into(), GLOBAL);
        register(editor, markdown, MARKDOWN);
        register(editor, previews, PREVIEWS);
        editor.bind(previews, "q", Command::QuitWindow).ok();
        editor.add_language(LANGUAGE, Some(markdown));
        editor.on_event(preview::sized);
        editor.on_event(|editor, link: &OpenLink| editor.open_externally(link.0.clone()));
    }
}

impl GuiApplet for Markdown {
    fn perform(&mut self, effect: &Payload) -> Option<Task<Payload>> {
        let scroll = effect.get::<ScrollPreview>()?;
        let id = preview_id(scroll.window);
        Some(match scroll.to {
            Move::By(y) => operation::scroll_by(id, AbsoluteOffset { x: 0.0, y }),
            Move::To(y) => operation::snap_to(
                id,
                RelativeOffset {
                    x: None,
                    y: Some(y),
                },
            ),
        })
    }

    fn tick(&mut self, editor: &Editor, _now: Instant) {
        let mut fresh: Vec<(BufferId, u64, markdown::Content)> = Vec::new();
        for window in editor.tiling().windows() {
            let Some(source) = editor
                .pane_of(window)
                .and_then(|pane| pane.applet_buffer::<PreviewView>())
                .and_then(|view| editor.buffer_by_id(view.source))
            else {
                continue;
            };
            let (id, revision) = (source.id(), source.revision());
            if fresh.iter().any(|(known, ..)| *known == id) {
                continue;
            }
            let content = match self
                .previews
                .iter()
                .position(|(known, seen, _)| *known == id && *seen == revision)
            {
                Some(index) => self.previews.swap_remove(index).2,
                None => markdown::Content::parse(&source.contents()),
            };
            fresh.push((id, revision, content));
        }
        self.previews = fresh;
    }

    fn body<'a>(
        &'a self,
        _editor: &'a Editor,
        pane: Pane<'a>,
        window: WindowId,
        context: &Context,
    ) -> Option<Element<'a, Payload>> {
        let view = pane.applet_buffer::<PreviewView>()?;
        let (_, _, parsed) = self.previews.iter().find(|(id, ..)| *id == view.source)?;
        let settings = markdown::Settings::with_text_size(
            context.size,
            markdown::Style::from_palette(context.palette),
        );
        let line = context.size * LINE_HEIGHT;
        let rendered = scrollable(
            container(
                markdown::view(parsed.items(), settings).map(|uri| Payload::new(OpenLink(uri))),
            )
            .padding(PADDING),
        )
        .id(preview_id(window))
        .height(Length::Fill);
        let measured = move |size: iced::Size| {
            Payload::new(PreviewSized {
                window,
                height: size.height,
                line,
            })
        };
        Some(
            sensor(rendered)
                .on_show(measured)
                .on_resize(measured)
                .into(),
        )
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use oxmacs_core::editor::Editor;
    use oxmacs_core::input::KeySequence;
    use oxmacs_core::text::Buffer;

    use crate::{LANGUAGE, Markdown};

    pub(crate) fn editor_with(text: &str, point: usize) -> Editor {
        let mut buffer = Buffer::with_text("t.md", text);
        buffer.set_point(point);
        let mut editor = Editor::with_buffer(buffer);
        editor.install(&Markdown::default());
        editor.set_major_mode(Some(LANGUAGE));
        editor
    }

    pub(crate) fn press(editor: &mut Editor, notation: &str) {
        let keys: KeySequence = notation.parse().unwrap();
        for key in keys.keys() {
            editor.handle_key(*key);
        }
    }

    pub(crate) fn type_text(editor: &mut Editor, text: &str) {
        for character in text.chars() {
            let notation = match character {
                ' ' => "SPC".to_owned(),
                other => other.to_string(),
            };
            press(editor, &notation);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::ops::Range;
    use std::path::Path;

    use oxmacs_core::editor::Editor;
    use oxmacs_core::syntax::{Language, Syntax};
    use oxmacs_core::text::Buffer;
    use oxmacs_core::theme::Highlight;

    use crate::testing::{press, type_text};
    use crate::{LANGUAGE, Markdown};

    const KNOWN: [Language; 3] = [Language::RUST, Language::TOML, Language::R];

    fn faces_knowing(
        known: &[Language],
        text: &str,
        line: usize,
    ) -> Vec<(Range<usize>, Highlight)> {
        let mut buffer = Buffer::with_text("t", text);
        let mut syntax = Syntax::knowing(LANGUAGE, known);
        syntax.sync(&mut buffer);
        syntax.highlights(&buffer, line)
    }

    fn faces(text: &str, line: usize) -> Vec<(Range<usize>, Highlight)> {
        faces_knowing(&KNOWN, text, line)
    }

    fn installed() -> Editor {
        let mut editor = Editor::new();
        editor.install(&Markdown::default());
        editor
    }

    #[test]
    fn markdown_blocks_are_highlighted() {
        let text = "# Title\n\n- item\n\n```\ncode\n```\n\n> quote\n";

        assert_eq!(faces(text, 0), [(0..7, Highlight::Heading)]);
        assert_eq!(faces(text, 2), [(0..2, Highlight::Keyword)]);
        assert_eq!(faces(text, 4), [(0..3, Highlight::Comment)]);
        assert_eq!(faces(text, 5), [(0..4, Highlight::String)]);
        assert_eq!(faces(text, 8), [(0..7, Highlight::DocComment)]);
    }

    #[test]
    fn markdown_inlines_are_highlighted() {
        let text = "Some *em* and **strong** with `code` and [link](url).\n# A *b*\n";

        assert_eq!(
            faces(text, 0),
            [
                (5..9, Highlight::Constant),
                (14..24, Highlight::Keyword),
                (30..36, Highlight::String),
                (42..46, Highlight::Link),
                (48..51, Highlight::Comment)
            ]
        );
        assert_eq!(
            faces(text, 1),
            [(0..4, Highlight::Heading), (4..7, Highlight::Constant)]
        );
    }

    #[test]
    fn fenced_code_is_highlighted_in_its_language() {
        let text = "```rust\nfn a() {}\n```\n\n```py\nx = 1\n```\n";

        assert_eq!(
            faces(text, 1),
            [(0..2, Highlight::Keyword), (3..4, Highlight::Function)]
        );
        assert_eq!(faces(text, 5), [(0..5, Highlight::String)]);
    }

    #[test]
    fn fenced_r_code_is_highlighted_as_r() {
        let text = "```r\nf <- function() NULL\n```\n";

        assert_eq!(
            faces(text, 1),
            [
                (0..1, Highlight::Function),
                (5..13, Highlight::Keyword),
                (16..20, Highlight::Constant)
            ]
        );
    }

    #[test]
    fn fences_use_only_the_languages_the_build_knows() {
        let text = "```toml\nname = 1\n```\n";

        assert_eq!(
            faces_knowing(&[Language::TOML], text, 1),
            [(0..4, Highlight::Function), (7..8, Highlight::Constant)]
        );
        assert_eq!(faces_knowing(&[], text, 1), [(0..8, Highlight::String)]);
    }

    #[test]
    fn headings_are_items() {
        let text = "# Guide\n\nIntro.\n\n## Install\n\nSteps\n=====\n";
        let mut buffer = Buffer::with_text("t", text);
        let mut syntax = Syntax::new(LANGUAGE);
        syntax.sync(&mut buffer);

        let labels: Vec<String> = syntax
            .items(&buffer)
            .into_iter()
            .map(|item| item.label)
            .collect();

        assert_eq!(labels, ["# Guide", "## Install", "Steps"]);
    }

    #[test]
    fn markdown_files_open_in_markdown_mode_with_fences_highlighted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("README.md");
        fs::write(&path, "# Notes\n```toml\nname = 1\n```\n").unwrap();
        let mut editor = installed();

        editor.open_file(&path);
        editor.prepare_display();

        assert_eq!(editor.language(), Some(LANGUAGE));
        assert!(
            editor.mode_line().ends_with("(Markdown)"),
            "{}",
            editor.mode_line()
        );
        assert_eq!(
            editor.line_style(2).spans,
            [(0..4, Highlight::Function), (7..8, Highlight::Constant)]
        );
        assert_eq!(
            editor.language_for(Path::new("notes.MARKDOWN")),
            Some(LANGUAGE)
        );
    }

    #[test]
    fn markdown_files_stay_plain_text_without_the_applet() {
        let editor = Editor::new();

        assert_eq!(editor.language_for(Path::new("README.md")), None);
    }

    #[test]
    fn markdown_mode_is_available_by_command() {
        let mut editor = installed();

        press(&mut editor, "M-x");
        type_text(&mut editor, "markdown-mode");
        press(&mut editor, "RET");

        assert_eq!(editor.language(), Some(LANGUAGE));
        assert!(editor.mode_line().ends_with("(Markdown)"));
    }

    #[test]
    fn the_doctor_shows_what_markdown_supports() {
        let mut editor = installed();

        press(&mut editor, "M-x");
        type_text(&mut editor, "oxmacs-doctor");
        press(&mut editor, "RET");

        let row = editor
            .buffer()
            .contents()
            .lines()
            .find(|line| line.trim_start().starts_with("Markdown (.md .markdown)"))
            .map(str::to_owned);
        assert!(row.is_some_and(|row| row.contains('✓')));
    }
    #[test]
    fn previews_parse_their_source_again_after_edits() {
        use oxmacs_gui::applet::GuiApplet;

        let mut editor = crate::testing::editor_with("# Title\n", 0);
        press(&mut editor, "C-c C-c p");
        let mut applet = Markdown::default();

        applet.tick(&editor, iced::time::Instant::now());
        assert_eq!(applet.previews.len(), 1);
        let first = applet.previews[0].1;
        press(&mut editor, "M-> x");
        applet.tick(&editor, iced::time::Instant::now());

        assert_eq!(applet.previews.len(), 1);
        assert_eq!(applet.previews[0].1, editor.buffer().revision());
        assert_ne!(applet.previews[0].1, first);
    }
}
