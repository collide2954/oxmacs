use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferState {
    Unmodified,
    Modified,
    ReadOnly,
    UnsavedSettings,
}

impl BufferState {
    pub fn flag(self) -> &'static str {
        match self {
            Self::Unmodified => "--",
            Self::Modified => "**",
            Self::ReadOnly => "%%",
            Self::UnsavedSettings => "%*",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub state: BufferState,
    pub name: String,
    pub line: usize,
    pub column: usize,
    pub crlf: bool,
    pub mode: Option<&'static str>,
    pub lint: Option<String>,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {}   L{} C{}",
            self.state.flag(),
            self.name,
            self.line,
            self.column
        )?;
        if self.crlf {
            write!(f, "   CRLF")?;
        }
        if let Some(mode) = self.mode {
            write!(f, "   ({mode})")?;
        }
        if let Some(lint) = &self.lint {
            write!(f, "   {lint}")?;
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use std::fs;

    use super::{BufferState, Status};
    use crate::editor::Editor;
    use crate::editor::test_support::{editor_with, press};

    #[test]
    fn the_status_describes_the_buffer_in_parts() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("main.rs"), "fn main() {}\r\n").unwrap();
        let mut editor = Editor::new();
        editor.open_file(&dir.path().join("main.rs"));
        press(&mut editor, "C-e x");

        assert_eq!(
            editor.pane().status(),
            Status {
                state: BufferState::Modified,
                name: "main.rs".to_owned(),
                line: 1,
                column: 13,
                crlf: true,
                mode: Some("Rust"),
                lint: None,
            }
        );
    }

    #[test]
    fn read_only_buffers_say_so() {
        let mut editor = editor_with("ab", 0);
        press(&mut editor, "C-x C-q");

        assert_eq!(editor.pane().status().state, BufferState::ReadOnly);
        assert_eq!(BufferState::ReadOnly.flag(), "%%");
    }
}
