use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Diagnostic {
    pub path: PathBuf,
    pub start: (usize, usize),
    pub end: (usize, usize),
    pub severity: Severity,
    pub message: String,
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

pub fn parse(line: &str, root: &Path) -> Option<Diagnostic> {
    if !line.starts_with('{') {
        return None;
    }
    let line: Line = serde_json::from_str(line).ok()?;
    let message = line.message.filter(|_| line.reason == "compiler-message")?;
    let severity = match message.level.as_str() {
        "warning" => Severity::Warning,
        level if level.starts_with("error") => Severity::Error,
        _ => return None,
    };
    let mut span = message.spans.iter().find(|span| span.is_primary)?;
    while Path::new(&span.file_name).is_absolute()
        && let Some(expansion) = &span.expansion
    {
        span = &expansion.span;
    }
    let text = match &message.code {
        Some(code) => format!("{} [{}]", message.message, code.code),
        None => message.message,
    };
    Some(Diagnostic {
        path: root.join(&span.file_name),
        start: (span.line_start, span.column_start),
        end: (span.line_end, span.column_end),
        severity,
        message: text,
    })
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

    #[test]
    fn compiler_messages_become_diagnostics_at_their_primary_span() {
        let root = Path::new("root");
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
            parse(&line, root),
            Some(Diagnostic {
                path: root.join("src/main.rs"),
                start: (3, 5),
                end: (3, 9),
                severity: Severity::Error,
                message: "cannot find value `oops` [E0425]".to_owned(),
            })
        );
    }

    #[test]
    fn warnings_without_codes_keep_their_message() {
        let line = message("warning", "null", &span("src/main.rs", true, "null"));

        let diagnostic = parse(&line, Path::new("root")).unwrap();

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

        let diagnostic = parse(&message("error", "null", &inner), Path::new("root")).unwrap();

        assert_eq!(diagnostic.path, Path::new("root").join("src/main.rs"));
    }

    #[test]
    fn other_lines_and_levels_are_ignored() {
        let root = Path::new("root");

        assert_eq!(parse("   Compiling demo v0.1.0", root), None);
        assert_eq!(
            parse(r#"{"reason":"build-finished","success":false}"#, root),
            None
        );
        assert_eq!(
            parse(
                &message("note", "null", &span("src/main.rs", true, "null")),
                root
            ),
            None
        );
        assert_eq!(parse(&message("error", "null", ""), root), None);
    }
}
