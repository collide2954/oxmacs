use crate::editor::minibuffer::Purpose;
use crate::editor::{Choice, Completion, Editor};
use crate::theme;

const SYSTEM: &str = "system";

impl Editor {
    pub(super) fn load_theme(&mut self) {
        self.open_minibuffer("Load theme: ", Purpose::LoadTheme);
    }

    pub(super) fn theme_completions(&self, matches: impl Fn(&str) -> bool) -> Vec<Completion> {
        let current = self.configured_theme.map_or(SYSTEM, |theme| theme.name);
        theme::builtin()
            .into_iter()
            .map(|theme| (theme.name, Some(theme)))
            .chain([(SYSTEM, None)])
            .filter(|(name, _)| matches(name))
            .map(|(name, theme)| Completion {
                label: name.to_owned(),
                annotation: (name == current).then(|| "current".to_owned()),
                choice: Choice::Theme(theme),
            })
            .collect()
    }

    pub(super) fn toggle_theme(&mut self) {
        self.configured_theme = Some(theme::counterpart(self.theme()));
    }
}

#[cfg(test)]
mod tests {
    use crate::command::Command;
    use crate::editor::Editor;
    use crate::editor::test_support::{labels, press, type_text};
    use crate::theme::Appearance;

    fn load(editor: &mut Editor, name: &str) {
        press(editor, "M-x");
        type_text(editor, "load-theme");
        press(editor, "RET");
        type_text(editor, name);
        press(editor, "M-RET");
    }

    #[test]
    fn load_theme_offers_the_themes_and_system_marking_the_current_one() {
        let mut editor = Editor::new();

        press(&mut editor, "M-x");
        type_text(&mut editor, "load-theme");
        press(&mut editor, "RET");

        assert_eq!(editor.minibuffer().unwrap().prompt(), "Load theme: ");
        assert_eq!(labels(&editor), ["adwaita", "adwaita-dark", "system"]);
        let marks: Vec<Option<String>> = editor
            .completions()
            .into_iter()
            .map(|completion| completion.annotation)
            .collect();
        assert_eq!(marks, [None, None, Some("current".to_owned())]);
    }

    #[test]
    fn a_loaded_theme_overrides_the_os_appearance() {
        let mut editor = Editor::new();
        editor.set_appearance(Appearance::Dark);

        load(&mut editor, "adwaita");
        editor.set_appearance(Appearance::Dark);

        assert_eq!(editor.theme().name, "adwaita");
    }

    #[test]
    fn system_follows_the_os_again() {
        let mut editor = Editor::new();
        load(&mut editor, "adwaita-dark");

        load(&mut editor, "system");
        editor.set_appearance(Appearance::Light);

        assert_eq!(editor.theme().name, "adwaita");
    }

    #[test]
    fn unknown_themes_are_not_loaded() {
        let mut editor = Editor::new();

        load(&mut editor, "solarized");

        assert_eq!(editor.minibuffer().unwrap().note(), Some("[No match]"));
        assert_eq!(editor.theme().name, "adwaita");
    }

    #[test]
    fn toggle_theme_flips_between_light_and_dark() {
        let mut editor = Editor::new();

        editor.execute(Command::ToggleTheme);
        assert_eq!(editor.theme().name, "adwaita-dark");
        editor.set_appearance(Appearance::Light);
        assert_eq!(editor.theme().name, "adwaita-dark");

        editor.execute(Command::ToggleTheme);
        assert_eq!(editor.theme().name, "adwaita");
    }
}
