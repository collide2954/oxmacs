use std::fmt::Write;

use super::{Diagnostic, Severity};
use crate::span::line_column;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Human,
    Json,
    Github,
}

impl Format {
    pub fn named(name: &str) -> Option<Self> {
        match name {
            "human" => Some(Self::Human),
            "json" => Some(Self::Json),
            "github" => Some(Self::Github),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Place {
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

pub fn place(source: &str, diagnostic: &Diagnostic) -> Place {
    let (line, column) = line_column(source, diagnostic.span.start);
    let (end_line, end_column) = line_column(source, diagnostic.span.end);
    Place {
        line,
        column,
        end_line,
        end_column,
    }
}

pub fn label(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
    }
}

pub fn human(file: &str, source: &str, diagnostic: &Diagnostic) -> String {
    let at = place(source, diagnostic);
    let text = source
        .lines()
        .nth(at.line.saturating_sub(1))
        .unwrap_or_default();
    let pad = " ".repeat(at.line.to_string().len());
    let width = if at.end_line == at.line {
        at.end_column.saturating_sub(at.column)
    } else {
        text.chars()
            .count()
            .saturating_sub(at.column.saturating_sub(1))
    }
    .max(1);
    let mut out = format!(
        "{}[{}]: {}\n",
        label(diagnostic.severity),
        diagnostic.code,
        diagnostic.message
    );
    let _ = writeln!(out, "{pad}--> {file}:{}:{}", at.line, at.column);
    let _ = writeln!(out, "{pad} |");
    let _ = writeln!(out, "{} | {text}", at.line);
    let _ = writeln!(
        out,
        "{pad} | {}{}",
        " ".repeat(at.column.saturating_sub(1)),
        "^".repeat(width)
    );
    out
}

pub fn json(findings: &[(&str, &str, &Diagnostic)]) -> String {
    let items: Vec<serde_json::Value> = findings
        .iter()
        .map(|(file, source, diagnostic)| {
            let at = place(source, diagnostic);
            serde_json::json!({
                "file": file,
                "line": at.line,
                "column": at.column,
                "end_line": at.end_line,
                "end_column": at.end_column,
                "severity": label(diagnostic.severity),
                "code": diagnostic.code,
                "message": diagnostic.message,
            })
        })
        .collect();
    serde_json::to_string_pretty(&items).unwrap_or_default()
}

fn escaped(text: &str, property: bool) -> String {
    let mut out = text
        .replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A");
    if property {
        out = out.replace(':', "%3A").replace(',', "%2C");
    }
    out
}

pub fn github(file: &str, source: &str, diagnostic: &Diagnostic) -> String {
    let at = place(source, diagnostic);
    format!(
        "::{} file={},line={},col={},endLine={},endColumn={},title={}::{}\n",
        label(diagnostic.severity),
        escaped(file, true),
        at.line,
        at.column,
        at.end_line,
        at.end_column,
        escaped(&format!("ox check ({})", diagnostic.code), true),
        escaped(&diagnostic.message, false)
    )
}

fn counted(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

pub fn summary(files: usize, errors: usize, warnings: usize) -> String {
    let mut parts = Vec::new();
    if errors > 0 {
        parts.push(counted(errors, "error"));
    }
    if warnings > 0 {
        parts.push(counted(warnings, "warning"));
    }
    if parts.is_empty() {
        parts.push("no problems".to_owned());
    }
    format!("Checked {}: {}", counted(files, "file"), parts.join(", "))
}
