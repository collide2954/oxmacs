use std::ffi::OsString;
use std::path::PathBuf;

use oxmacs_core::editor::Visit;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Launch {
    Edit { visits: Vec<Visit>, init: bool },
    Help,
    Version,
    Unknown(String),
}

pub const HELP: &str = "\
Usage: oxmacs [OPTION]... [+LINE[:COLUMN]] [FILE]...

Opens each FILE, at LINE and COLUMN when +LINE[:COLUMN] comes before it.

Options:
  -q, --no-init   start without running custom.ox and init.ox
      --help      show this help
      --version   show the version
      --          take everything after it as a file name
";

pub fn parse(args: impl IntoIterator<Item = OsString>) -> Launch {
    let mut visits = Vec::new();
    let mut init = true;
    let mut position = None;
    let mut options = true;
    for arg in args {
        if options && let Some(option) = arg.to_str().filter(|arg| arg.starts_with('-')) {
            match option {
                "--" => options = false,
                "--help" => return Launch::Help,
                "--version" => return Launch::Version,
                "-q" | "--no-init" => init = false,
                other => return Launch::Unknown(other.to_owned()),
            }
            continue;
        }
        if options && let Some(found) = arg.to_str().and_then(parse_position) {
            position = Some(found);
            continue;
        }
        let (line, column) = position.take().unzip();
        visits.push(Visit {
            path: PathBuf::from(arg),
            line,
            column: column.flatten(),
        });
    }
    Launch::Edit { visits, init }
}

fn parse_position(arg: &str) -> Option<(usize, Option<usize>)> {
    let position = arg.strip_prefix('+')?;
    match position.split_once(':') {
        Some((line, column)) => Some((line.parse().ok()?, Some(column.parse().ok()?))),
        None => Some((position.parse().ok()?, None)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn visits(args: &[&str]) -> Vec<Visit> {
        match parse(args.iter().map(OsString::from)) {
            Launch::Edit { visits, .. } => visits,
            other => panic!("{other:?}"),
        }
    }

    fn visit(path: &str, line: Option<usize>, column: Option<usize>) -> Visit {
        Visit {
            path: PathBuf::from(path),
            line,
            column,
        }
    }

    #[test]
    fn plain_arguments_are_files() {
        assert_eq!(
            visits(&["a.rs", "b.rs"]),
            [visit("a.rs", None, None), visit("b.rs", None, None)]
        );
    }

    #[test]
    fn a_plus_line_applies_to_the_next_file_only() {
        assert_eq!(
            visits(&["+12", "a.rs", "b.rs"]),
            [visit("a.rs", Some(12), None), visit("b.rs", None, None)]
        );
    }

    #[test]
    fn a_plus_line_can_carry_a_column() {
        assert_eq!(visits(&["+3:7", "a.rs"]), [visit("a.rs", Some(3), Some(7))]);
    }

    #[test]
    fn malformed_positions_are_file_names() {
        assert_eq!(
            visits(&["+x", "+3:", "+:4"]),
            [
                visit("+x", None, None),
                visit("+3:", None, None),
                visit("+:4", None, None)
            ]
        );
    }

    #[test]
    fn a_double_dash_makes_the_rest_file_names() {
        assert_eq!(
            visits(&["--", "+5", "--"]),
            [visit("+5", None, None), visit("--", None, None)]
        );
    }

    fn launch(args: &[&str]) -> Launch {
        parse(args.iter().map(OsString::from))
    }

    #[test]
    fn options_ask_for_help_the_version_or_no_init_files() {
        assert_eq!(launch(&["--help"]), Launch::Help);
        assert_eq!(launch(&["a.rs", "--version"]), Launch::Version);
        assert_eq!(
            launch(&["-q", "a.rs"]),
            Launch::Edit {
                visits: vec![visit("a.rs", None, None)],
                init: false
            }
        );
        assert_eq!(
            launch(&["--no-init"]),
            Launch::Edit {
                visits: Vec::new(),
                init: false
            }
        );
        assert_eq!(
            launch(&["a.rs"]),
            Launch::Edit {
                visits: vec![visit("a.rs", None, None)],
                init: true
            }
        );
    }

    #[test]
    fn unknown_options_are_refused_but_anything_after_a_double_dash_is_a_file() {
        assert_eq!(
            launch(&["--frobnicate", "a.rs"]),
            Launch::Unknown("--frobnicate".to_owned())
        );
        assert_eq!(launch(&["-x"]), Launch::Unknown("-x".to_owned()));
        assert_eq!(
            launch(&["--", "--help", "-q"]),
            Launch::Edit {
                visits: vec![visit("--help", None, None), visit("-q", None, None)],
                init: true
            }
        );
    }

    #[test]
    fn the_help_names_every_option() {
        for option in [
            "--help",
            "--version",
            "-q, --no-init",
            "+LINE[:COLUMN]",
            "--",
        ] {
            assert!(HELP.contains(option), "{option}");
        }
    }
}
