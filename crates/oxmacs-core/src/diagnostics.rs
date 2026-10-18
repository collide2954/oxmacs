use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Diagnostic {
    pub path: PathBuf,
    pub start: (usize, usize),
    pub end: (usize, usize),
    pub severity: Severity,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    Diagnostic(Diagnostic),
    Finished,
    Other,
}

#[derive(Deserialize)]
struct Line {
    reason: String,
    message: Option<Report>,
}

#[derive(Deserialize)]
struct Report {
    message: String,
    level: String,
    code: Option<Code>,
    spans: Vec<Span>,
}

#[derive(Deserialize)]
struct Code {
    code: String,
}

#[derive(Deserialize)]
struct Span {
    file_name: String,
    line_start: usize,
    line_end: usize,
    column_start: usize,
    column_end: usize,
    is_primary: bool,
    expansion: Option<Box<Expansion>>,
}

#[derive(Deserialize)]
struct Expansion {
    span: Span,
}

pub fn parse(line: &str, root: &Path) -> Output {
    let Some(line) = line
        .starts_with('{')
        .then(|| serde_json::from_str::<Line>(line).ok())
        .flatten()
    else {
        return Output::Other;
    };
    match line.reason.as_str() {
        "build-finished" => Output::Finished,
        "compiler-message" => line
            .message
            .and_then(|report| diagnostic(report, root))
            .map_or(Output::Other, Output::Diagnostic),
        _ => Output::Other,
    }
}

fn diagnostic(report: Report, root: &Path) -> Option<Diagnostic> {
    let severity = match report.level.as_str() {
        "warning" => Severity::Warning,
        level if level.starts_with("error") => Severity::Error,
        _ => return None,
    };
    let mut span = report.spans.iter().find(|span| span.is_primary)?;
    while Path::new(&span.file_name).is_absolute()
        && let Some(expansion) = &span.expansion
    {
        span = &expansion.span;
    }
    let message = match &report.code {
        Some(code) => format!("{} [{}]", report.message, code.code),
        None => report.message,
    };
    Some(Diagnostic {
        path: root.join(&span.file_name),
        start: (span.line_start, span.column_start),
        end: (span.line_end, span.column_end),
        severity,
        message,
    })
}

#[cfg(test)]
pub(crate) fn sample(
    level: &str,
    file: &str,
    start: (usize, usize),
    end: (usize, usize),
    text: &str,
) -> String {
    format!(
        r#"{{"reason":"compiler-message","package_id":"demo","message":{{"rendered":"...","children":[],"code":null,"level":"{level}","message":"{text}","spans":[{{"column_end":{},"column_start":{},"expansion":null,"file_name":"{file}","is_primary":true,"line_end":{},"line_start":{}}}]}}}}"#,
        end.1, start.1, end.0, start.0
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(level: &str, code: &str, spans: &str) -> String {
        format!(
            r#"{{"reason":"compiler-message","package_id":"demo","message":{{"rendered":"...","$message_type":"diagnostic","children":[],"code":{code},"level":"{level}","message":"cannot find value `oops`","spans":[{spans}]}}}}"#
        )
    }

    fn span(file: &str, primary: bool, expansion: &str) -> String {
        format!(
            r#"{{"byte_end":40,"byte_start":36,"column_end":9,"column_start":5,"expansion":{expansion},"file_name":"{file}","is_primary":{primary},"label":null,"line_end":3,"line_start":3,"suggested_replacement":null,"suggestion_applicability":null,"text":[]}}"#
        )
    }

    fn diagnostic_in(line: &str) -> Diagnostic {
        match parse(line, Path::new("root")) {
            Output::Diagnostic(diagnostic) => diagnostic,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn compiler_messages_become_diagnostics_at_their_primary_span() {
        let line = message(
            "error",
            r#"{"code":"E0425","explanation":null}"#,
            &[
                span("src/other.rs", false, "null"),
                span("src/main.rs", true, "null"),
            ]
            .join(","),
        );

        assert_eq!(
            diagnostic_in(&line),
            Diagnostic {
                path: Path::new("root").join("src/main.rs"),
                start: (3, 5),
                end: (3, 9),
                severity: Severity::Error,
                message: "cannot find value `oops` [E0425]".to_owned(),
            }
        );
    }

    #[test]
    fn warnings_without_codes_keep_their_message() {
        let diagnostic = diagnostic_in(&message(
            "warning",
            "null",
            &span("src/main.rs", true, "null"),
        ));

        assert_eq!(diagnostic.severity, Severity::Warning);
        assert_eq!(diagnostic.message, "cannot find value `oops`");
    }

    #[test]
    fn spans_inside_external_macros_point_at_the_call() {
        let external = if cfg!(windows) {
            r"C:\\rustc\\library\\core\\src\\macros.rs"
        } else {
            "/rustc/library/core/src/macros.rs"
        };
        let call = span("src/main.rs", false, "null");
        let inner = span(
            external,
            true,
            &format!(r#"{{"def_site_span":null,"macro_decl_name":"panic!","span":{call}}}"#),
        );

        let diagnostic = diagnostic_in(&message("error", "null", &inner));

        assert_eq!(diagnostic.path, Path::new("root").join("src/main.rs"));
    }

    #[test]
    fn the_end_of_the_build_is_reported() {
        assert_eq!(
            parse(
                r#"{"reason":"build-finished","success":false}"#,
                Path::new("root")
            ),
            Output::Finished
        );
    }

    #[test]
    fn samples_parse_like_real_messages() {
        let diagnostic = diagnostic_in(&sample("warning", "src/a.rs", (2, 1), (2, 4), "unused"));

        assert_eq!(diagnostic.start, (2, 1));
        assert_eq!(diagnostic.end, (2, 4));
        assert_eq!(diagnostic.message, "unused");
    }

    #[test]
    fn other_lines_and_levels_are_ignored() {
        let root = Path::new("root");

        assert_eq!(parse("   Compiling demo v0.1.0", root), Output::Other);
        assert_eq!(parse("{not json", root), Output::Other);
        assert_eq!(
            parse(
                &message("note", "null", &span("src/main.rs", true, "null")),
                root
            ),
            Output::Other
        );
        assert_eq!(parse(&message("error", "null", ""), root), Output::Other);
    }
}
