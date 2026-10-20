mod engine;
mod gui;
mod layout;
mod model;
#[cfg(test)]
mod sample;
mod view;

use std::sync::{Arc, Mutex};

use iced::time::Instant;
use iced::{Element, Task};

use oxmacs_core::applet::{Applet, Payload};
use oxmacs_core::command::{Command, CommandId, CommandSpec, ScopeId};
use oxmacs_core::editor::{Editor, Pane, WindowId};
use oxmacs_core::external::managed::{Checksum, Form, Managed};
use oxmacs_gui::applet::{Context, GuiApplet};

pub use gui::Rendered;
pub use layout::{Size, Zoom};
pub use model::{Document, Heading};
pub use view::{DocId, Move, OpenDocument, Opened, RenderPages, ScrollPdf, Viewport, Wanted};

use engine::Ink;
use gui::Cache;
use view::PdfState;

pub const PDFIUM: Managed = Managed {
    name: "pdfium",
    repository: "bblanchon/pdfium-binaries",
    summary: "Draws pages for the PDF viewer",
    form: Form::Library,
    tag_prefix: "chromium/",
    archives: &[
        ("windows", "x86_64", "pdfium-win-x64.tgz"),
        ("windows", "aarch64", "pdfium-win-arm64.tgz"),
        ("macos", "x86_64", "pdfium-mac-x64.tgz"),
        ("macos", "aarch64", "pdfium-mac-arm64.tgz"),
        ("linux", "x86_64", "pdfium-linux-x64.tgz"),
        ("linux", "aarch64", "pdfium-linux-arm64.tgz"),
    ],
    checksum: Checksum::Digest,
    setting: None,
};

struct Spec {
    name: &'static str,
    summary: &'static str,
    keys: &'static [&'static str],
    run: fn(&mut Editor),
}

const COMMANDS: &[Spec] = &[
    Spec {
        name: "pdf-next-page",
        summary: "Go to the next page",
        keys: &["n"],
        run: |editor| view::step_page(editor, 1),
    },
    Spec {
        name: "pdf-previous-page",
        summary: "Go to the previous page",
        keys: &["p"],
        run: |editor| view::step_page(editor, -1),
    },
    Spec {
        name: "pdf-scroll-down",
        summary: "Scroll the document down a little",
        keys: &["C-n", "<down>"],
        run: |editor| view::scroll_lines(editor, 1.0),
    },
    Spec {
        name: "pdf-scroll-up",
        summary: "Scroll the document up a little",
        keys: &["C-p", "<up>"],
        run: |editor| view::scroll_lines(editor, -1.0),
    },
    Spec {
        name: "pdf-scroll-page-down",
        summary: "Scroll down by a windowful",
        keys: &["SPC", "C-v", "<next>"],
        run: |editor| view::scroll_screens(editor, 1.0),
    },
    Spec {
        name: "pdf-scroll-page-up",
        summary: "Scroll up by a windowful",
        keys: &["DEL", "M-v", "<prior>"],
        run: |editor| view::scroll_screens(editor, -1.0),
    },
    Spec {
        name: "pdf-first-page",
        summary: "Go to the first page",
        keys: &["M-<", "<home>"],
        run: view::first_page,
    },
    Spec {
        name: "pdf-last-page",
        summary: "Go to the last page",
        keys: &["M->", "<end>"],
        run: view::last_page,
    },
    Spec {
        name: "pdf-goto-page",
        summary: "Go to a page by its number",
        keys: &["M-g g", "M-g M-g"],
        run: view::goto_page,
    },
    Spec {
        name: "pdf-zoom-in",
        summary: "Make the pages larger",
        keys: &["+"],
        run: |editor| view::zoom_by(editor, view::ZOOM_STEP),
    },
    Spec {
        name: "pdf-zoom-out",
        summary: "Make the pages smaller",
        keys: &["-"],
        run: |editor| view::zoom_by(editor, 1.0 / view::ZOOM_STEP),
    },
    Spec {
        name: "pdf-actual-size",
        summary: "Show the pages at their actual size",
        keys: &["="],
        run: |editor| view::set_zoom(editor, Zoom::Scale(1.0)),
    },
    Spec {
        name: "pdf-fit-width",
        summary: "Fit the pages to the window's width",
        keys: &["W"],
        run: |editor| view::set_zoom(editor, Zoom::FitWidth),
    },
    Spec {
        name: "pdf-fit-page",
        summary: "Fit whole pages in the window",
        keys: &["H"],
        run: |editor| view::set_zoom(editor, Zoom::FitPage),
    },
    Spec {
        name: "pdf-toggle-continuous",
        summary: "Switch between scrolling pages and one page at a time",
        keys: &["c"],
        run: view::toggle_layout,
    },
    Spec {
        name: "pdf-dark-mode",
        summary: "Draw pages in the theme's colors",
        keys: &["m"],
        run: view::toggle_dark,
    },
    Spec {
        name: "pdf-reload",
        summary: "Read the document again",
        keys: &["g"],
        run: view::reload,
    },
];

fn register(editor: &mut Editor, scope: ScopeId) -> Vec<CommandId> {
    let mut added = Vec::new();
    for spec in COMMANDS {
        let command = editor.add_command(CommandSpec {
            name: spec.name,
            summary: spec.summary,
            help: None,
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

#[derive(Debug)]
pub struct Pdf {
    cache: Arc<Mutex<Cache>>,
    ink: Ink,
}

impl Default for Pdf {
    fn default() -> Self {
        Self {
            cache: Arc::default(),
            ink: gui::ink(&oxmacs_core::theme::ADWAITA_DARK),
        }
    }
}

impl Applet for Pdf {
    fn name(&self) -> &'static str {
        "pdf"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn opt_in(&self) -> bool {
        true
    }

    fn install(&self, editor: &mut Editor) {
        let scope = editor.add_scope("PDF documents");
        editor.set_applet_state(PdfState::new(scope));
        register(editor, scope);
        editor.bind(scope, "q", Command::QuitWindow).ok();
        editor.add_managed_tool(PDFIUM);
        editor.add_file_opener(&["pdf"], view::open);
        editor.on_event(view::opened);
        editor.on_event(view::viewed);
        editor.add_mode_line_segment(view::page_counter);
    }
}

impl GuiApplet for Pdf {
    fn perform(&mut self, payload: &Payload) -> Option<Task<Payload>> {
        if let Some(request) = payload.get::<OpenDocument>() {
            return Some(gui::open(request));
        }
        if let Some(request) = payload.get::<RenderPages>() {
            return Some(gui::render(&self.cache, request, self.ink));
        }
        payload.get::<ScrollPdf>().map(gui::scroll)
    }

    fn tick(&mut self, editor: &Editor, _now: Instant) {
        self.ink = gui::ink(editor.theme());
    }

    fn body<'a>(
        &'a self,
        _editor: &'a Editor,
        pane: Pane<'a>,
        window: WindowId,
        context: &Context,
    ) -> Option<Element<'a, Payload>> {
        gui::body(&self.cache, self.ink, pane, window, context)
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use oxmacs_core::applet::Payload;
    use oxmacs_core::editor::{Editor, Effect};
    use oxmacs_core::input::KeySequence;

    pub(crate) fn press(editor: &mut Editor, notation: &str) {
        let keys: KeySequence = notation.parse().unwrap();
        for key in keys.keys() {
            editor.handle_key(*key);
        }
    }

    pub(crate) fn type_text(editor: &mut Editor, text: &str) {
        for character in text.chars() {
            press(editor, &character.to_string());
        }
    }

    pub(crate) fn requests<T: Clone + 'static>(editor: &mut Editor) -> Vec<T> {
        editor
            .take_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                Effect::Applet(payload) => payload.get::<T>().cloned(),
                _ => None,
            })
            .collect()
    }

    pub(crate) fn deliver<T: std::any::Any + Send + Sync>(editor: &mut Editor, event: T) {
        editor.deliver(&Payload::new(event));
    }
}
