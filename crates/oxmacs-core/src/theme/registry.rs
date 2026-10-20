use std::sync::{PoisonError, RwLock};

use super::{ADWAITA, ADWAITA_DARK, Appearance, Theme, for_appearance};

#[derive(Debug)]
pub(super) struct Registry(RwLock<Vec<&'static Theme>>);

impl Registry {
    pub(super) const fn new() -> Self {
        Self(RwLock::new(Vec::new()))
    }

    pub(super) fn register(&self, themes: &'static [Theme]) {
        let mut known = self.0.write().unwrap_or_else(PoisonError::into_inner);
        for theme in themes {
            let taken = |name: &str| {
                [ADWAITA.name, ADWAITA_DARK.name].contains(&name)
                    || known.iter().any(|other| other.name == name)
            };
            if !taken(theme.name) {
                known.push(theme);
            }
        }
    }

    pub(super) fn all(&self) -> Vec<&'static Theme> {
        let known = self.0.read().unwrap_or_else(PoisonError::into_inner);
        [&ADWAITA, &ADWAITA_DARK]
            .into_iter()
            .chain(known.iter().copied())
            .collect()
    }

    pub(super) fn find(&self, name: &str) -> Option<&'static Theme> {
        self.all().into_iter().find(|theme| theme.name == name)
    }

    pub(super) fn counterpart(&self, theme: &Theme) -> &'static Theme {
        let family = |name: &str| name.split('-').next().unwrap_or_default().to_owned();
        let other = match theme.appearance {
            Appearance::Light => Appearance::Dark,
            Appearance::Dark => Appearance::Light,
        };
        self.all()
            .into_iter()
            .find(|candidate| {
                candidate.appearance == other && family(candidate.name) == family(theme.name)
            })
            .unwrap_or_else(|| for_appearance(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn themes(list: &[(&'static str, Appearance)]) -> &'static [Theme] {
        list.iter()
            .map(|(name, appearance)| Theme {
                name,
                appearance: *appearance,
                ..ADWAITA
            })
            .collect::<Vec<_>>()
            .leak()
    }

    fn names(registry: &Registry) -> Vec<&'static str> {
        registry.all().iter().map(|theme| theme.name).collect()
    }

    #[test]
    fn the_builtin_themes_come_first_then_registered_ones_in_order() {
        let registry = Registry::new();

        registry.register(themes(&[
            ("gruvbox-dark", Appearance::Dark),
            ("gruvbox-light", Appearance::Light),
        ]));

        assert_eq!(
            names(&registry),
            ["adwaita", "adwaita-dark", "gruvbox-dark", "gruvbox-light"]
        );
        assert_eq!(
            registry.find("gruvbox-light").map(|theme| theme.name),
            Some("gruvbox-light")
        );
    }

    #[test]
    fn registering_a_name_twice_keeps_the_first() {
        let registry = Registry::new();

        registry.register(themes(&[("nord", Appearance::Dark)]));
        registry.register(themes(&[
            ("nord", Appearance::Light),
            ("adwaita", Appearance::Dark),
        ]));

        assert_eq!(names(&registry), ["adwaita", "adwaita-dark", "nord"]);
        assert_eq!(registry.find("nord").unwrap().appearance, Appearance::Dark);
        assert_eq!(
            registry.find("adwaita").unwrap().appearance,
            Appearance::Light
        );
    }

    #[test]
    fn counterparts_stay_in_the_family_named_before_the_first_dash() {
        let registry = Registry::new();
        registry.register(themes(&[
            ("catppuccin-mocha", Appearance::Dark),
            ("catppuccin-latte", Appearance::Light),
            ("dracula", Appearance::Dark),
        ]));
        let named = |name| registry.find(name).unwrap();

        assert_eq!(
            registry.counterpart(named("catppuccin-latte")).name,
            "catppuccin-mocha"
        );
        assert_eq!(
            registry.counterpart(named("catppuccin-mocha")).name,
            "catppuccin-latte"
        );
        assert_eq!(registry.counterpart(named("dracula")).name, "adwaita");
        assert_eq!(registry.counterpart(named("adwaita")).name, "adwaita-dark");
    }
}
