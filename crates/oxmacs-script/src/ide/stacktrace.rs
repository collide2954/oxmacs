use std::path::Path;
use std::sync::Arc;

use ox_lang::Error;
use ox_lang::error::{ErrorKind, Frame};
use ox_lang::symbol::Sym;
use oxmacs_core::editor::{Editor, Place, Places};
use oxmacs_core::theme::Highlight;

pub const STACKTRACE: &str = "*ox stacktrace*";

fn places(frames: &[Frame]) -> Vec<Place> {
    let mut texts: Vec<(&str, Arc<str>)> = Vec::new();
    frames
        .iter()
        .map(|frame| {
            let file = frame.file();
            let source = frame
                .source
                .as_ref()
                .filter(|_| !Path::new(file).is_file())
                .map(|source| {
                    if let Some((_, text)) = texts.iter().find(|(name, _)| *name == file) {
                        return Arc::clone(text);
                    }
                    let text: Arc<str> = source.text.as_str().into();
                    texts.push((file, Arc::clone(&text)));
                    text
                });
            let (line, column) = frame.line_column();
            let name = frame.function.map_or("top level", Sym::name);
            Place {
                label: Some((name.to_owned(), Highlight::Function)),
                source,
                ..Place::at(file, line, column)
            }
        })
        .collect()
}

pub(crate) fn shown_file(file: &str) -> &str {
    let path = Path::new(file);
    if path.is_absolute() {
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(file)
    } else {
        file
    }
}

pub fn show(editor: &mut Editor, error: &Error) {
    if error.traceback.len() < 2 {
        return;
    }
    let kind = ErrorKind::NAMES
        .iter()
        .find(|(kind, _)| *kind == error.kind)
        .map_or("error", |(_, name)| *name);
    let root = editor
        .buffer()
        .file()
        .and_then(|file| file.path.parent().map(Path::to_path_buf));
    editor.show_places(
        STACKTRACE,
        Places {
            title: error.message.clone(),
            header: vec![kind.to_owned()],
            places: places(&error.traceback),
            ordered: true,
            root,
            ..Places::default()
        },
    );
}
