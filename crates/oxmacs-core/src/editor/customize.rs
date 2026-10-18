use crate::config::Config;
use crate::editor::{Editor, Effect};

impl Editor {
    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn reconfigure(&mut self, config: &Config) {
        self.apply_config(config);
        self.effects.push(Effect::Reconfigure);
    }
}

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use crate::editor::{Editor, Effect};

    #[test]
    fn reconfiguring_applies_at_once_and_tells_the_ui() {
        let mut editor = Editor::new();

        editor.reconfigure(&Config::parse("tab-width = 4").unwrap());

        assert_eq!(editor.tab_width(), 4);
        assert_eq!(editor.config().tab_width, 4);
        assert_eq!(editor.take_effects(), [Effect::Reconfigure]);
    }
}
