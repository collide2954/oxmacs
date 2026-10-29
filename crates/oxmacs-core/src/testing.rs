use crate::editor::{Editor, ErrorPattern};
use crate::external::managed::{Checksum, Form, Managed, ToolSpec};
use crate::input::KeySequence;
use crate::syntax::Language;
use crate::text::Buffer;

pub const RUST_ERRORS: [&str; 2] = [
    r"^\s*--> (?P<file>.+):(?P<line>\d+):(?P<column>\d+):?\s*$",
    r"panicked at (?P<file>.+):(?P<line>\d+):(?P<column>\d+):?\s*$",
];

pub fn rust_errors() -> Vec<ErrorPattern> {
    RUST_ERRORS
        .iter()
        .filter_map(|pattern| ErrorPattern::new(pattern).ok())
        .collect()
}

fn archives(names: [&str; 6]) -> Vec<(String, String, String)> {
    let systems = [
        ("windows", "x86_64"),
        ("windows", "aarch64"),
        ("macos", "x86_64"),
        ("macos", "aarch64"),
        ("linux", "x86_64"),
        ("linux", "aarch64"),
    ];
    systems
        .into_iter()
        .zip(names)
        .map(|((os, arch), name)| (os.to_owned(), arch.to_owned(), name.to_owned()))
        .collect()
}

pub fn air() -> Managed {
    Managed::new(ToolSpec {
        name: "air".to_owned(),
        repository: "posit-dev/air".to_owned(),
        summary: "R formatter".to_owned(),
        group: "R".to_owned(),
        form: Form::Program,
        tag_prefix: "v".to_owned(),
        archives: archives([
            "air-x86_64-pc-windows-msvc.zip",
            "air-aarch64-pc-windows-msvc.zip",
            "air-x86_64-apple-darwin.tar.gz",
            "air-aarch64-apple-darwin.tar.gz",
            "air-x86_64-unknown-linux-gnu.tar.gz",
            "air-aarch64-unknown-linux-gnu.tar.gz",
        ]),
        checksum: Checksum::Sidecar,
        setting: Some("air-program".to_owned()),
    })
}

pub fn pdfium() -> Managed {
    Managed::new(ToolSpec {
        name: "pdfium".to_owned(),
        repository: "bblanchon/pdfium-binaries".to_owned(),
        summary: "Draws pages for the PDF viewer".to_owned(),
        group: "PDF".to_owned(),
        form: Form::Library,
        tag_prefix: "chromium/".to_owned(),
        archives: archives([
            "pdfium-win-x64.tgz",
            "pdfium-win-arm64.tgz",
            "pdfium-mac-x64.tgz",
            "pdfium-mac-arm64.tgz",
            "pdfium-linux-x64.tgz",
            "pdfium-linux-arm64.tgz",
        ]),
        checksum: Checksum::Digest,
        setting: Some("library".to_owned()),
    })
}

pub fn add_kits(editor: &mut Editor) {
    for language in Language::built_in() {
        editor.add_language(language, None);
    }
    editor.set_error_patterns("rust", rust_errors());
}

pub fn kits() -> Editor {
    let mut editor = Editor::new();
    add_kits(&mut editor);
    editor
}

pub fn press(editor: &mut Editor, notation: &str) {
    let sequence: KeySequence = notation.parse().unwrap();
    for key in sequence.keys() {
        editor.handle_key(*key);
    }
}

pub fn type_text(editor: &mut Editor, text: &str) {
    for c in text.chars() {
        let notation = if c == ' ' {
            "SPC".to_owned()
        } else {
            c.to_string()
        };
        press(editor, &notation);
    }
}

pub fn labels(editor: &Editor) -> Vec<String> {
    editor
        .completions()
        .into_iter()
        .map(|completion| completion.label)
        .collect()
}

pub fn question(editor: &Editor) -> Option<String> {
    editor
        .question()
        .map(|question| question.prompt().to_owned())
}

pub fn minibuffer_input(editor: &Editor) -> String {
    editor.minibuffer().unwrap().input().contents()
}

pub fn lines(editor: &Editor) -> Vec<String> {
    editor
        .buffer()
        .contents()
        .lines()
        .map(str::to_owned)
        .collect()
}

pub fn at_point(editor: &Editor) -> String {
    let buffer = editor.buffer();
    let line = buffer.line_of(buffer.point());
    buffer.text_in(buffer.point()..buffer.line_end(line))
}

pub fn editor_with(text: &str, point: usize) -> Editor {
    let mut buffer = Buffer::with_text("t", text);
    buffer.set_point(point);
    Editor::with_buffer(buffer)
}

pub fn point_after(text: &str, point: usize, notation: &str) -> usize {
    let mut editor = editor_with(text, point);
    press(&mut editor, notation);
    editor.buffer().point()
}

pub fn contents_after(text: &str, point: usize, notation: &str) -> (String, usize) {
    let mut editor = editor_with(text, point);
    press(&mut editor, notation);
    (editor.buffer().contents(), editor.buffer().point())
}
