use std::ffi::OsString;

#[derive(Debug, Default)]
pub struct Parsed {
    flags: Vec<&'static str>,
    values: Vec<(&'static str, OsString)>,
    pub positional: Vec<OsString>,
}

impl Parsed {
    pub fn flag(&self, name: &str) -> bool {
        self.flags.contains(&name)
    }

    pub fn values(&self, name: &str) -> impl Iterator<Item = &OsString> {
        self.values
            .iter()
            .filter(move |(given, _)| *given == name)
            .map(|(_, value)| value)
    }

    pub fn value(&self, name: &str) -> Option<&OsString> {
        self.values(name).last()
    }
}

pub fn parse(
    arguments: &[OsString],
    flags: &[&'static str],
    valued: &[&'static str],
) -> Result<Parsed, String> {
    let mut parsed = Parsed::default();
    let mut given = arguments.iter();
    let mut options = true;
    while let Some(argument) = given.next() {
        let option = argument
            .to_str()
            .filter(|text| options && text.starts_with('-') && *text != "-");
        let Some(option) = option else {
            parsed.positional.push(argument.clone());
            continue;
        };
        if option == "--" {
            options = false;
        } else if let Some(flag) = flags.iter().find(|flag| **flag == option) {
            parsed.flags.push(flag);
        } else if let Some(name) = valued.iter().find(|name| **name == option) {
            let value = given
                .next()
                .ok_or_else(|| format!("{name} needs a value"))?;
            parsed.values.push((name, value.clone()));
        } else {
            return Err(format!("unknown option {option}"));
        }
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::parse;

    fn given(words: &[&str]) -> Vec<OsString> {
        words.iter().map(OsString::from).collect()
    }

    #[test]
    fn options_sit_anywhere_and_a_double_dash_ends_them() {
        let parsed = parse(
            &given(&["a.ox", "--strict", "--root", "lib", "--", "--strict"]),
            &["--strict"],
            &["--root"],
        )
        .unwrap();

        assert!(parsed.flag("--strict"));
        assert_eq!(parsed.value("--root"), Some(&OsString::from("lib")));
        assert_eq!(parsed.positional, given(&["a.ox", "--strict"]));
    }

    #[test]
    fn unknown_options_and_missing_values_are_refused() {
        assert_eq!(
            parse(&given(&["--frob"]), &[], &[]).unwrap_err(),
            "unknown option --frob"
        );
        assert_eq!(
            parse(&given(&["--root"]), &[], &["--root"]).unwrap_err(),
            "--root needs a value"
        );
    }
}
