use ox_lang::Isolate;
use ox_lang::migrate::migrate;

fn migrated(source: &str) -> String {
    migrate(&mut Isolate::new(), "user", source).unwrap_or_else(|problem| panic!("{problem}"))
}

#[test]
fn with_becomes_assoc() {
    assert_eq!(migrated("(with p :x 10 :y 2)"), "(assoc p :x 10 :y 2)");
}

#[test]
fn defrule_becomes_defrules_with_one_rule() {
    assert_eq!(
        migrated("(defrule (my-unless test body ...)\n  (if test nil (do body ...)))"),
        "(defrules my-unless [] [(my-unless test body ...)\n  (if test nil (do body ...))])"
    );
}

#[test]
fn case_becomes_match_and_still_gives_nil_when_nothing_matches() {
    assert_eq!(
        migrated("(case key [:up 1] [(:left :right) 2] [else 3])"),
        "(match key [:up 1] [:left 2] [:right 2] [_ 3])"
    );
    assert_eq!(
        migrated("(case light [red :stop] (green :go))"),
        "(match light ['red :stop] ['green :go] [_ nil])"
    );
    assert_eq!(
        migrated("(case f.kind\n  [:tetra (paint 1)]\n  [:angel (paint 2)])"),
        "(match f.kind\n  [:tetra (paint 1)]\n  [:angel (paint 2)]\n  [_ nil])"
    );
    assert_eq!(migrated("(case x)"), "(match x [_ nil])");
}

#[test]
fn nested_forms_are_migrated_too() {
    assert_eq!(
        migrated("(case a [1 (case b [2 (with p :x 1)])] [else nil])"),
        "(match a [1 (match b [2 (assoc p :x 1)] [_ nil])] [_ nil])"
    );
}

#[test]
fn migrated_case_forms_give_the_same_answers() {
    let source = "(def (name n) (case n [1 :one] [(2 3) :few] [else :many]))\n\
                  (def (light l) (case l [red :stop] [green :go]))\n\
                  [(name 1) (name 3) (name 9) (light 'red) (light 'blue) (case \"x\" [\"x\" :found])]";
    let mut isolate = Isolate::new();
    let value = isolate.eval(&migrated(source)).unwrap();

    assert_eq!(isolate.write(value), "[:one :few :many :stop nil :found]");
}

#[test]
fn names_the_file_gives_its_own_meaning_are_left_alone() {
    for kept in [
        "(def (case x) [:mine x])\n(case 1)",
        "(let [with (fn [a b] b)] (with 1 2))",
        "'(case x [1 2])",
        "(match x [1 :one] [_ :other])",
    ] {
        assert_eq!(migrated(kept), kept);
    }
}

#[test]
fn files_that_cannot_be_read_are_refused() {
    let mut isolate = Isolate::new();

    assert!(migrate(&mut isolate, "user", "(case x [1 2]").is_err());
}
