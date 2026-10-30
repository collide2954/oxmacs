fn evaluated(source: &str) -> String {
    let mut isolate = ox_library::isolate();
    match isolate.eval(&format!("(module (import (ox csv))) {source}")) {
        Ok(value) => isolate.write(value),
        Err(error) => format!("error: {}", error.message),
    }
}

#[test]
fn rows_are_split_on_commas_outside_quotes() {
    assert_eq!(
        evaluated(
            r#"(csv/read "\u{FEFF}name,note\r\nana,\"says \"\"hi\"\", then\nleaves\"\r\nbo,\n")"#
        ),
        r#"[["name" "note"] ["ana" "says \"hi\", then\nleaves"] ["bo" ""]]"#
    );
}

#[test]
fn a_header_row_turns_rows_into_maps() {
    assert_eq!(
        evaluated(
            r#"(let [rows (csv/read "Title,Login URI\nmail,https://example.com\n" :header true)] [(count rows) (get (first rows) :"Login URI") (first rows)])"#
        ),
        r#"[1 "https://example.com" {Title: "mail" :"Login URI" "https://example.com"}]"#
    );
}

#[test]
fn unclosed_quotes_are_an_error() {
    assert_eq!(
        evaluated(r#"(csv/read "a,\"b\n")"#),
        "error: csv/read found a quote that never closes"
    );
}
