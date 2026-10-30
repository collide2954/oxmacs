use crate::support::evaluated;

const TOY: &str = r#"(def toy
  (lexer/spec {token: #"(?P<comment>//.*)|(?P<block>/\*|`)|(?P<key>\"[^\"]*\")\s*:|(?P<string>\"[^\"]*\")|(?P<attribute>@\w+)|(?P<macro>^#\w+)|(?P<number>\b\d+)|(?P<call>[a-z]\w*)\s*\(|(?P<word>[A-Za-z]\w*)"
               blocks: {"/*" {close: #"\*/" face: :comment}
                        "`" {close: #"^(?:[^`\\]|\\.)*`" face: :string}}
               keywords: #{"if" "return"}
               constants: #{"nil"}
               types: #{"Int"}}))
(def (lexed lines)
  (get (reduce (fn [acc text]
                 (let [done (lexer/lex toy text acc.state)]
                   {state: done.state spans: (conj acc.spans done.spans)}))
               {state: nil spans: []}
               lines)
       :spans))"#;

fn lexed(source: &str) -> String {
    evaluated("(ox lexer)", &format!("{TOY} {source}"))
}

#[test]
fn words_numbers_strings_calls_and_comments_get_their_faces() {
    assert_eq!(
        lexed(r#"(lexer/lex toy "if f(1) return nil // done" nil)"#),
        "{spans: [[0 2 :keyword] [3 4 :function] [5 6 :constant] [8 14 :keyword] [15 18 :constant] [19 26 :comment]] state: nil}"
    );
    assert_eq!(
        lexed(r#"(get (lexer/lex toy "{\"a\": \"b\", x: Int}" nil) :spans)"#),
        "[[1 4 :attribute] [6 9 :string] [14 17 :type]]"
    );
    assert_eq!(
        lexed(r##"(get (lexer/lex toy "#define @wrap if(" nil) :spans)"##),
        "[[0 7 :macro] [8 13 :attribute] [14 16 :keyword]]"
    );
}

#[test]
fn comments_and_strings_that_run_past_a_line_carry_on_into_the_next() {
    assert_eq!(
        lexed(r#"(lexed ["a /* one" "two" "" "three */ if" "`x" "y` 2"])"#),
        "[[[2 8 :comment]] [[0 3 :comment]] [] [[0 8 :comment] [9 11 :keyword]] [[0 2 :string]] [[0 2 :string] [3 4 :constant]]]"
    );
}

#[test]
fn a_block_closed_on_its_own_line_lets_the_rest_be_read_afresh() {
    assert_eq!(
        lexed(r#"(get (lexer/lex toy "/* \"*/ if \"s\" `a\\`b` 1" nil) :spans)"#),
        "[[0 6 :comment] [7 9 :keyword] [10 13 :string] [14 20 :string] [21 22 :constant]]"
    );
}

#[test]
fn positions_count_characters_and_specs_are_checked() {
    assert_eq!(
        lexed(r#"(get (lexer/lex toy "\"é\" if" nil) :spans)"#),
        "[[0 3 :string] [4 6 :keyword]]"
    );
    assert_eq!(
        evaluated("(ox lexer)", "(lexer/spec 5)"),
        "error: lexer/spec expects a map of :token, :blocks, :keywords, :constants and :types, got 5"
    );
}
