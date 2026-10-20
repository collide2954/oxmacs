const OPT_IN: &[(&str, &str, bool)] = &[(
    "animated-splash",
    "0.1.0",
    cfg!(feature = "animated-splash"),
)];

pub fn opt_in() -> Vec<(&'static str, &'static str)> {
    OPT_IN
        .iter()
        .filter(|(.., built)| *built)
        .map(|(name, version, _)| (*name, *version))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_features_in_this_build_are_listed_with_their_versions() {
        assert_eq!(
            opt_in().contains(&("animated-splash", "0.1.0")),
            cfg!(feature = "animated-splash")
        );
    }
}
