use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Visit {
    pub path: PathBuf,
    pub line: Option<usize>,
    pub column: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Invocation {
    Edit(Vec<Visit>),
    IrcDaemon,
}

pub fn invocation(args: impl IntoIterator<Item = OsString>) -> Invocation {
    let args: Vec<OsString> = args.into_iter().collect();
    if args.len() == 1 && args[0] == "irc-daemon" {
        return Invocation::IrcDaemon;
    }
    Invocation::Edit(parse(args))
}

pub fn parse(args: impl IntoIterator<Item = OsString>) -> Vec<Visit> {
    let mut visits = Vec::new();
    let mut position = None;
    let mut options = true;
    for arg in args {
        if options && arg == "--" {
            options = false;
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
    visits
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
        parse(args.iter().map(OsString::from))
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

    fn invoked(args: &[&str]) -> Invocation {
        invocation(args.iter().map(OsString::from))
    }

    #[test]
    fn irc_daemon_runs_the_bouncer_instead_of_the_editor() {
        assert_eq!(invoked(&["irc-daemon"]), Invocation::IrcDaemon);
        assert_eq!(
            invoked(&["notes.txt"]),
            Invocation::Edit(vec![visit("notes.txt", None, None)])
        );
        assert_eq!(
            invoked(&["--", "irc-daemon"]),
            Invocation::Edit(vec![visit("irc-daemon", None, None)])
        );
        assert_eq!(
            invoked(&["notes.txt", "irc-daemon"]),
            Invocation::Edit(vec![
                visit("notes.txt", None, None),
                visit("irc-daemon", None, None)
            ])
        );
    }

    #[test]
    fn a_trailing_position_is_ignored() {
        assert_eq!(visits(&["a.rs", "+5"]), [visit("a.rs", None, None)]);
    }
}
