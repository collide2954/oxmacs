use crate::editor::prompts::minibuffer::Purpose;
use crate::editor::{Choice, Completion, Editor};
use crate::theme::{self, Theme};

const SYSTEM: &str = "system";

impl Editor {
    pub fn themes(&self) -> &[&'static Theme] {
        &self.themes
    }

    pub(in crate::editor) fn load_theme(&mut self) {
        self.open_minibuffer("Load theme: ", Purpose::LoadTheme);
    }

    pub(in crate::editor) fn theme_completions(
        &self,
        matches: impl Fn(&str) -> bool,
    ) -> Vec<Completion> {
        let current = self.configured_theme.map_or(SYSTEM, |theme| theme.name);
        self.themes
            .iter()
            .map(|theme| (theme.name, Some(*theme)))
            .chain([(SYSTEM, None)])
            .filter(|(name, _)| matches(name))
            .map(|(name, theme)| Completion {
                label: name.to_owned(),
                annotation: (name == current).then(|| "current".to_owned()),
                summary: None,
                choice: Choice::Theme(theme),
            })
            .collect()
    }

    pub(in crate::editor) fn previewed_theme(&self) -> Option<&'static Theme> {
        if self.minibuffer.as_ref()?.purpose != Purpose::LoadTheme {
            return None;
        }
        match self.selected_completion()?.choice {
            Choice::Theme(theme) => {
                Some(theme.unwrap_or_else(|| theme::for_appearance(self.appearance)))
            }
            _ => None,
        }
    }

    pub(in crate::editor) fn theme_in_use_position(&self) -> usize {
        self.theme_completions(|_| true)
            .iter()
            .position(|completion| completion.annotation.is_some())
            .unwrap_or(0)
    }

    pub(in crate::editor) fn toggle_theme(&mut self) {
        self.use_theme(Some(theme::counterpart(self.theme())));
    }

    pub(in crate::editor) fn use_theme(&mut self, theme: Option<&'static Theme>) {
        let name = theme.map(|theme| serde_json::Value::from(theme.name));
        self.change_config("theme", name);
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

    fn open(editor: &mut Editor) {
        press(editor, "M-x");
        type_text(editor, "load-theme");
        press(editor, "RET");
    }

    fn selected(editor: &Editor) -> usize {
        editor.minibuffer().unwrap().selected()
    }

    #[test]
    fn the_list_starts_on_the_theme_in_use() {
        let mut editor = Editor::new();
        load(&mut editor, "adwaita-dark");

        open(&mut editor);
        assert_eq!(selected(&editor), 1);

        type_text(&mut editor, "a");
        assert_eq!(selected(&editor), 0);
        press(&mut editor, "DEL");
        assert_eq!(selected(&editor), 1);
    }

    #[test]
    fn moving_through_the_list_previews_each_theme_without_choosing_it() {
        let mut editor = Editor::new();
        open(&mut editor);
        assert_eq!(editor.theme().name, "adwaita");

        press(&mut editor, "C-p");
        assert_eq!(editor.theme().name, "adwaita-dark");
        assert_eq!(editor.config().theme, None);

        press(&mut editor, "C-p");
        assert_eq!(editor.theme().name, "adwaita");
    }

    #[test]
    fn quitting_puts_the_theme_in_use_back() {
        let mut editor = Editor::new();
        open(&mut editor);

        press(&mut editor, "C-p C-g");

        assert_eq!(editor.theme().name, "adwaita");
        assert_eq!(editor.config().theme, None);
    }

    #[test]
    fn return_keeps_the_previewed_theme() {
        let mut editor = Editor::new();
        open(&mut editor);

        press(&mut editor, "C-p RET");

        assert_eq!(editor.theme().name, "adwaita-dark");
        assert_eq!(editor.config().theme.as_deref(), Some("adwaita-dark"));
    }

    #[test]
    fn typing_previews_the_first_match_and_no_match_previews_nothing() {
        let mut editor = Editor::new();
        open(&mut editor);

        type_text(&mut editor, "dark");
        assert_eq!(editor.theme().name, "adwaita-dark");

        type_text(&mut editor, "x");
        assert_eq!(editor.theme().name, "adwaita");
    }

    #[test]
    fn system_previews_the_os_appearance() {
        let mut editor = Editor::new();
        load(&mut editor, "adwaita");
        editor.set_appearance(Appearance::Dark);
        open(&mut editor);
        assert_eq!(editor.theme().name, "adwaita");

        press(&mut editor, "C-n C-n");

        assert_eq!(editor.theme().name, "adwaita-dark");
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
