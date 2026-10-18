use iced::widget::{column, container, text};
use iced::{Element, Font, Length, Subscription, Task, Theme, keyboard};
use mx_core::editor::Editor;

use crate::buffer_view::BufferView;
use crate::keys::{self, Platform};

#[cfg(target_os = "windows")]
pub const EDITOR_FONT: Font = Font::with_name("Consolas");
#[cfg(target_os = "macos")]
pub const EDITOR_FONT: Font = Font::with_name("Menlo");
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub const EDITOR_FONT: Font = Font::MONOSPACE;

const TEXT_SIZE: f32 = 15.0;

#[derive(Debug, Default)]
pub struct App {
    editor: Editor,
}

#[derive(Debug, Clone)]
pub enum Message {
    Keyboard(keyboard::Event),
    RowsChanged(usize),
}

impl App {
    pub fn editor(&self) -> &Editor {
        &self.editor
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Keyboard(keyboard::Event::KeyPressed {
                key,
                modified_key,
                modifiers,
                ..
            }) => {
                if let Some(key) =
                    keys::translate(&key, &modified_key, modifiers, Platform::current())
                {
                    self.editor.handle_key(key);
                }
            }
            Message::Keyboard(_) => {}
            Message::RowsChanged(rows) => self.editor.resize_window(rows),
        }
        if self.editor.should_quit() {
            iced::exit()
        } else {
            Task::none()
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        keyboard::listen().map(Message::Keyboard)
    }

    pub fn title(&self) -> String {
        format!("{} - mx", self.editor.buffer().name())
    }

    pub fn view(&self) -> Element<'_, Message> {
        let line = |content: String| text(content).font(EDITOR_FONT).size(TEXT_SIZE);
        column![
            BufferView::new(&self.editor, EDITOR_FONT, TEXT_SIZE, Message::RowsChanged),
            container(line(self.editor.mode_line()))
                .width(Length::Fill)
                .padding([1, 4])
                .style(mode_line_style),
            container(line(self.editor.message().unwrap_or(" ").to_owned()))
                .width(Length::Fill)
                .padding([1, 4]),
        ]
        .into()
    }
}

fn mode_line_style(theme: &Theme) -> container::Style {
    let strong = theme.extended_palette().background.strong;
    container::Style {
        background: Some(strong.color.into()),
        text_color: Some(strong.text),
        ..container::Style::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::keyboard::key::{Code, Physical};
    use iced::keyboard::{Key, Location, Modifiers};

    fn press(app: &mut App, c: &str, modifiers: Modifiers) {
        let _ = app.update(Message::Keyboard(keyboard::Event::KeyPressed {
            key: Key::Character(c.into()),
            modified_key: Key::Character(c.into()),
            physical_key: Physical::Code(Code::KeyA),
            location: Location::Standard,
            modifiers,
            text: Some(c.into()),
            repeat: false,
        }));
    }

    #[test]
    fn title_names_the_current_buffer() {
        assert_eq!(App::default().title(), "*scratch* - mx");
    }

    #[test]
    fn key_presses_reach_the_editor() {
        let mut app = App::default();

        press(&mut app, "a", Modifiers::empty());

        assert_eq!(app.editor().buffer().contents(), "a");
    }

    #[test]
    fn key_releases_are_ignored() {
        let mut app = App::default();

        let _ = app.update(Message::Keyboard(keyboard::Event::KeyReleased {
            key: Key::Character("a".into()),
            modified_key: Key::Character("a".into()),
            physical_key: Physical::Code(Code::KeyA),
            location: Location::Standard,
            modifiers: Modifiers::empty(),
        }));

        assert_eq!(app.editor().buffer().contents(), "");
    }

    #[test]
    fn window_rows_resize_the_editor_viewport() {
        let mut app = App::default();

        let _ = app.update(Message::RowsChanged(7));

        assert_eq!(app.editor().viewport().rows(), 7);
    }

    #[test]
    fn quit_sequence_sets_the_editor_to_quit() {
        let mut app = App::default();

        press(&mut app, "x", Modifiers::CTRL);
        press(&mut app, "c", Modifiers::CTRL);

        assert!(app.editor().should_quit());
    }
}
