use std::rc::Rc;

use crate::builtins;
use crate::compile::compile;
use crate::error::Error;
use crate::heap::Heap;
use crate::native::Native;
use crate::symbol::Sym;
use crate::value::Value;
use crate::vm::Runtime;
use crate::{print, reader};

#[derive(Debug)]
pub struct Isolate {
    runtime: Runtime,
}

impl Default for Isolate {
    fn default() -> Self {
        Self::new()
    }
}

impl Isolate {
    pub fn new() -> Self {
        let mut isolate = Self {
            runtime: Runtime::default(),
        };
        builtins::install(&mut isolate);
        isolate
    }

    pub fn eval(&mut self, source: &str) -> Result<Value, Error> {
        let (forms, errors) = reader::read(source);
        if let Some(error) = errors.into_iter().next() {
            return Err(Error::at(error.span, error.message));
        }
        let mut result = Value::Nil;
        for form in &forms {
            let code = Rc::new(compile(&mut self.runtime.heap, form)?);
            result = self.runtime.run(&code)?;
        }
        Ok(result)
    }

    pub fn define_native(
        &mut self,
        name: &str,
        least: usize,
        most: Option<usize>,
        function: impl Fn(&mut Heap, &[Value]) -> Result<Value, Error> + 'static,
    ) {
        let name = Sym::intern(name);
        if let Ok(value) = self.runtime.heap.native(Native {
            name,
            least,
            most,
            function: Rc::new(function),
        }) {
            self.runtime.globals.insert(name, value);
        }
    }

    pub fn write(&self, value: Value) -> String {
        print::write(&self.runtime.heap, value)
    }
}
#[cfg(test)]
mod tests {
    use super::Isolate;
    use crate::error::Error;
    use crate::span::Span;
    use crate::value::Value;

    fn evaluated(source: &str) -> String {
        let mut isolate = Isolate::new();
        match isolate.eval(source) {
            Ok(value) => isolate.write(value),
            Err(error) => format!("error: {}", error.message),
        }
    }

    #[test]
    fn constants_evaluate_to_themselves() {
        for (source, printed) in [
            ("42", "42"),
            ("-3.5", "-3.5"),
            (r#""hi""#, r#""hi""#),
            ("true", "true"),
            ("nil", "nil"),
            (":key", ":key"),
            (r"\a", r"\a"),
        ] {
            assert_eq!(evaluated(source), printed, "{source}");
        }
    }

    #[test]
    fn the_last_form_gives_the_result() {
        assert_eq!(evaluated("1 2 3"), "3");
        assert_eq!(evaluated(""), "nil");
    }

    #[test]
    fn read_errors_stop_evaluation() {
        assert_eq!(evaluated("1 (2"), "error: This ( is never closed");
    }

    fn all(cases: &[(&str, &str)]) {
        for (source, printed) in cases {
            assert_eq!(evaluated(source), *printed, "{source}");
        }
    }

    #[test]
    fn arithmetic_works_on_integers_and_floats() {
        all(&[
            ("(+ 1 2)", "3"),
            ("(+ 1 2.5)", "3.5"),
            ("(+)", "0"),
            ("(*)", "1"),
            ("(- 5)", "-5"),
            ("(- 10 1 2)", "7"),
            ("(* 2 3 4)", "24"),
            ("(/ 7 2)", "3.5"),
            ("(/ 6 3)", "2.0"),
            ("(// 7 2)", "3"),
            ("(// -7 2)", "-4"),
            ("(// 7 -2)", "-4"),
            ("(// 7.5 2)", "3.0"),
            ("(+ 1 (* 2 3))", "7"),
            ("(- (* 2 3) 1)", "5"),
            ("(+ (* 2 (+ 1 1)) (- 10 (* 2 3)))", "8"),
        ]);
    }

    #[test]
    fn comparisons_chain_across_their_arguments() {
        all(&[
            ("(< 1 2 3)", "true"),
            ("(< 1 3 2)", "false"),
            ("(<= 1 1 2)", "true"),
            ("(>= 2 2 1)", "true"),
            ("(> 2.5 1)", "true"),
            ("(> 1 2.5)", "false"),
        ]);
    }

    #[test]
    fn arithmetic_mistakes_are_errors() {
        all(&[
            ("(/ 1 0)", "error: Division by zero"),
            ("(// 1 0)", "error: Division by zero"),
            (r#"(+ 1 "a")"#, r#"error: + expects numbers, got "a""#),
            ("(1 2)", "error: 1 is not a function"),
            ("(nope 1)", "error: nope is not defined"),
            ("(<)", "error: < takes at least 1 argument, got 0"),
        ]);
    }

    #[test]
    fn errors_point_at_the_call_that_failed() {
        let error = Isolate::new().eval("(+ 1\n (/ 1 0))").unwrap_err();

        assert_eq!(error.span, Some(Span::new(6, 13)));
    }

    #[test]
    fn if_chooses_a_branch_by_truth() {
        all(&[
            ("(if true 1 2)", "1"),
            ("(if false 1 2)", "2"),
            ("(if nil 1 2)", "2"),
            ("(if 0 1 2)", "1"),
            (r#"(if "" 1 2)"#, "1"),
            ("(if () 1 2)", "2"),
            ("(if false 1)", "nil"),
            ("(if (< 1 2) (+ 1 1) (/ 1 0))", "2"),
            ("(if (> 1 2) (/ 1 0) (* 3 3))", "9"),
            ("(+ 1 (if true 10 20))", "11"),
        ]);
    }

    #[test]
    fn do_runs_its_forms_in_order_and_gives_the_last() {
        all(&[
            ("(do 1 2 3)", "3"),
            ("(do)", "nil"),
            ("(do (+ 1 1))", "2"),
            ("(+ 1 (do 5 6))", "7"),
        ]);
    }

    #[test]
    fn malformed_ifs_are_reported() {
        let message = "error: if takes a test, a then form and an optional else form";
        all(&[
            ("(if)", message),
            ("(if 1)", message),
            ("(if 1 2 3 4)", message),
        ]);
    }

    #[test]
    fn def_names_a_global_value() {
        all(&[
            ("(def x 10) (+ x 1)", "11"),
            ("(def x 1)", "x"),
            ("(def x 1) (def x 2) x", "2"),
            ("(def twice-ten (* 2 10)) twice-ten", "20"),
        ]);
    }

    #[test]
    fn globals_last_between_evaluations() {
        let mut isolate = Isolate::new();

        isolate.eval("(def y 5)").unwrap();

        assert_eq!(isolate.eval("(* y y)"), Ok(Value::Int(25)));
    }

    #[test]
    fn malformed_defs_are_reported() {
        let message = "error: def takes a name and a value";
        all(&[
            ("(def)", message),
            ("(def x)", message),
            ("(def 1 2)", message),
            ("(def x 1 2)", message),
        ]);
    }

    #[test]
    fn functions_are_made_with_fn_and_called() {
        all(&[
            ("((fn [x] (* x x)) 5)", "25"),
            ("(def square (fn [x] (* x x))) (square 7)", "49"),
            ("(def (add a b) (+ a b)) (add 2 3)", "5"),
            ("(def (answer) 42) (answer)", "42"),
            ("(def (g x) 1 2 (+ x 1)) (g 1)", "2"),
            ("((fn [x]) 1)", "nil"),
            ("(def (inc x) (+ x 1)) (inc (inc (inc 0)))", "3"),
            ("(def x 100) (def (f x) x) (f 1)", "1"),
            ("(def (f a b) (- a b)) (+ 10 (f 5 (f 3 1)))", "13"),
        ]);
    }

    #[test]
    fn functions_call_themselves_through_their_global_name() {
        all(&[(
            "(def (fact n) (if (< n 2) 1 (* n (fact (- n 1))))) (fact 10)",
            "3628800",
        )]);
    }

    #[test]
    fn functions_print_with_their_names() {
        all(&[("(fn [x] x)", "#<fn>"), ("(def (f) 1) f", "#<fn f>")]);
    }

    #[test]
    fn calls_with_the_wrong_number_of_arguments_are_errors() {
        all(&[
            ("((fn [x] x))", "error: fn takes 1 argument, got 0"),
            ("(def (f a) a) (f 1 2)", "error: f takes 1 argument, got 2"),
        ]);
    }

    #[test]
    fn malformed_functions_are_reported() {
        all(&[
            ("(fn)", "error: fn takes a parameter vector and a body"),
            ("(fn x 1)", "error: fn takes a parameter vector and a body"),
            ("(fn [1] 1)", "error: Parameters must be names"),
            ("(def (1 x) x)", "error: def takes a name and a value"),
        ]);
    }

    #[test]
    fn let_binds_names_in_order_for_its_body() {
        all(&[
            ("(let [x 1 y 2] (+ x y))", "3"),
            ("(let [x 1 y (+ x 1)] (* x y))", "2"),
            ("(let [x 1] (let [x (+ x 10)] x))", "11"),
            ("(let [x 1] (let [x 5] x) x)", "1"),
            ("(def x 100) (let [x 1] x)", "1"),
            ("(def x 100) (let [y 1] x)", "100"),
            ("(let [] 5)", "5"),
            ("(let [x 2] 0 (+ x x))", "4"),
            ("(def (f a) (let [b (* a 2)] (+ a b))) (f 3)", "9"),
            ("(+ 1 (let [x 2] (* x 10)) (let [y 3] y))", "24"),
        ]);
    }

    #[test]
    fn malformed_lets_are_reported() {
        all(&[
            ("(let)", "error: let takes a binding vector and a body"),
            ("(let x 1)", "error: let takes a binding vector and a body"),
            (
                "(let [x] x)",
                "error: let bindings come in name and value pairs",
            ),
            ("(let [1 2] 3)", "error: Bindings must be names"),
        ]);
    }

    #[test]
    fn closures_capture_the_variables_around_them() {
        all(&[
            ("(def (adder n) (fn [x] (+ x n))) ((adder 10) 5)", "15"),
            ("(def (f a) (fn [b] (fn [c] (+ a b c)))) (((f 1) 2) 3)", "6"),
            ("(let [x 5] ((fn [] (* x 2))))", "10"),
            ("(let [y 2] (def h (fn [] y))) (h)", "2"),
            (
                "(def (f a) (let [b 2] (fn [] (fn [] (* a b))))) (((f 3)))",
                "6",
            ),
        ]);
    }

    #[test]
    fn set_assigns_locals_captured_variables_and_globals() {
        all(&[
            ("(let [x 1] (set! x (+ x 1)) x)", "2"),
            ("(def (f x) (set! x 10) x) (f 1)", "10"),
            ("(let [x 1] (set! x 7))", "7"),
            ("(def g 1) (set! g 5) g", "5"),
            (
                "(def (counter) (let [n 0] (fn [] (set! n (+ n 1)) n))) (def c (counter)) (c) (c) (c)",
                "3",
            ),
            (
                "(let [n 0] (def inc! (fn [] (set! n (+ n 1)))) (def get (fn [] n))) (inc!) (inc!) (get)",
                "2",
            ),
            (
                "(def (make) (let [n 0] (fn [] (set! n (+ n 1)) n))) (def a (make)) (def b (make)) (a) (a) (b)",
                "1",
            ),
        ]);
    }

    #[test]
    fn malformed_and_undefined_assignments_are_errors() {
        all(&[
            ("(set! nowhere 1)", "error: nowhere is not defined"),
            ("(set!)", "error: set! takes a name and a value"),
            ("(set! x)", "error: set! takes a name and a value"),
            ("(set! 1 2)", "error: set! takes a name and a value"),
        ]);
    }

    #[test]
    fn deep_recursion_runs_on_the_vms_own_stack() {
        all(&[(
            "(def (down n) (if (< n 1) 0 (+ 1 (down (- n 1))))) (down 100000)",
            "100000",
        )]);
    }

    #[test]
    fn tail_calls_run_in_constant_space() {
        all(&[
            (
                "(def (count-down n) (if (< n 1) :done (count-down (- n 1)))) (count-down 500000)",
                ":done",
            ),
            (
                "(def (even? n) (if (< n 1) true (odd? (- n 1)))) (def (odd? n) (if (< n 1) false (even? (- n 1)))) (even? 500001)",
                "false",
            ),
            (
                "(def (sum n total) (if (< n 1) total (let [next (- n 1)] (sum next (+ total n))))) (sum 500000 0)",
                "125000250000",
            ),
            (
                "(def (ends-native n) (if (< n 1) (+ 1 1) (ends-native (- n 1)))) (ends-native 10)",
                "2",
            ),
            (
                "(def (grow n) (let [k n] (set! k (+ k 1)) (if (< k 500000) (grow k) k))) (grow 0)",
                "500000",
            ),
        ]);
    }

    #[test]
    fn runaway_recursion_is_a_stack_overflow() {
        all(&[(
            "(def (forever n) (+ 1 (forever n))) (forever 0)",
            "error: Stack overflow",
        )]);
    }

    #[test]
    fn optional_parameters_take_their_defaults() {
        all(&[
            ("(def (f a (b 10)) (+ a b)) (f 1)", "11"),
            ("(def (f a (b 10)) (+ a b)) (f 1 2)", "3"),
            ("(def (f a (b (* a 2)) (c (+ b 1))) (+ a b c)) (f 1)", "6"),
            (
                "(def (f a (b (* a 2)) (c (+ b 1))) (+ a b c)) (f 1 5)",
                "12",
            ),
            ("(def (f (x)) x) (f)", "nil"),
        ]);
    }

    #[test]
    fn rest_parameters_collect_the_remaining_arguments() {
        all(&[
            ("(def (f a & more) more) (f 1 2 3)", "(2 3)"),
            ("(def (f a & more) more) (f 1)", "nil"),
            ("(def (f & all) all) (f)", "nil"),
            ("(def (f (a 0) & more) more) (f 1 2)", "(2)"),
        ]);
    }

    #[test]
    fn keyword_parameters_take_names_and_defaults() {
        let scaled = "(def (f x {scale 2 shift 0}) (+ (* x scale) shift))";
        all(&[
            (&format!("{scaled} (f 5)"), "10"),
            (&format!("{scaled} (f 5 :shift 1)"), "11"),
            (&format!("{scaled} (f 5 shift: 1 scale: 3)"), "16"),
            ("(def (f (a 1) {b 2}) (+ a b)) (f :b 5)", "6"),
            ("(def (f (a 1) {b 2}) (+ a b)) (f 10 :b 5)", "15"),
            ("(def (f & xs {k 0}) k) (f 1 2 :k 9)", "9"),
            ("(def (f {k (+ 1 1)}) k) (f)", "2"),
            (
                "(def (g n {total 0}) (if (< n 1) total (g (- n 1) :total (+ total n)))) (g 1000)",
                "500500",
            ),
        ]);
    }

    #[test]
    fn parameter_mistakes_are_reported() {
        all(&[
            (
                "(def (f a (b 1)) b) (f)",
                "error: f takes 1 to 2 arguments, got 0",
            ),
            (
                "(def (f a (b 1)) b) (f 1 2 3)",
                "error: f takes 1 to 2 arguments, got 3",
            ),
            (
                "(def (f a & r) r) (f)",
                "error: f takes at least 1 argument, got 0",
            ),
            (
                "(def (f {k 1}) k) (f :j 2)",
                "error: f has no keyword parameter :j",
            ),
            (
                "(def (f {k 1}) k) (f :k)",
                "error: f was given :k without a value",
            ),
            (
                "(fn [(a 1) b] a)",
                "error: Required parameters come before optional ones",
            ),
            (
                "(fn [& a b] a)",
                "error: & takes one name for the rest of the arguments",
            ),
            ("(fn [{k 1} a] a)", "error: Keyword parameters come last"),
            ("(fn [(1 2)] 1)", "error: Parameters must be names"),
        ]);
    }

    #[test]
    fn integers_grow_into_bignums_and_shrink_back() {
        all(&[
            ("(+ 9223372036854775807 1)", "9223372036854775808"),
            ("(- -9223372036854775808 1)", "-9223372036854775809"),
            ("(- -9223372036854775808)", "9223372036854775808"),
            (
                "(* 99999999999 99999999999 99999999999)",
                "999999999970000000000299999999999",
            ),
            (
                "123456789012345678901234567890",
                "123456789012345678901234567890",
            ),
            (
                "(+ 123456789012345678901234567890 1)",
                "123456789012345678901234567891",
            ),
            ("(- (+ 9223372036854775807 1) 1)", "9223372036854775807"),
            ("(// 100000000000000000000 3)", "33333333333333333333"),
            ("(// -100000000000000000000 3)", "-33333333333333333334"),
            ("(/ 100000000000000000000 4)", "2.5e19"),
            ("(< 9223372036854775807 9223372036854775808)", "true"),
            ("(> 100000000000000000000 1.5)", "true"),
        ]);
    }

    #[test]
    fn shrunken_bignums_are_plain_integers_again() {
        let mut isolate = Isolate::new();

        assert_eq!(
            isolate.eval("(- (+ 9223372036854775807 10) 10)"),
            Ok(Value::Int(i64::MAX))
        );
    }
    #[test]
    fn str_joins_what_its_arguments_display_as() {
        all(&[
            (r#"(str "a" 1 :k nil 2.5 \c)"#, r#""a1:k2.5c""#),
            ("(str)", r#""""#),
            (r#"(def name "Ana") "Hi {name}!""#, r#""Hi Ana!""#),
            (r#""{(+ 1 2)} items""#, r#""3 items""#),
            (r#"(let [a 1 b 2] "{a}+{b}={(+ a b)}")"#, r#""1+2=3""#),
        ]);
    }

    #[test]
    fn strings_count_and_index_by_character() {
        all(&[
            (r#"(count "héllo")"#, "5"),
            (r#"(count "")"#, "0"),
            ("(count nil)", "0"),
            ("(def (f & xs) (count xs)) (f 1 2 3)", "3"),
            (r#"(nth "héllo" 1)"#, r"\é"),
            (r#"(nth "日本語" 2)"#, r"\語"),
            (
                r#"(nth "abc" 5)"#,
                "error: Index 5 is out of range for a string of 3 characters",
            ),
            ("(count 5)", "error: count expects a collection, got 5"),
        ]);
    }

    #[test]
    fn vector_literals_evaluate_their_items() {
        all(&[
            ("[]", "[]"),
            ("[1 (+ 1 1) 3]", "[1 2 3]"),
            (r#"[[1 2] "a" :k nil \c]"#, r#"[[1 2] "a" :k nil \c]"#),
            ("(let [x 5] [x (* x x)])", "[5 25]"),
            ("(def (f & xs) xs) (f [1] [2])", "([1] [2])"),
            (
                "[1 (undefined-thing)]",
                "error: undefined-thing is not defined",
            ),
        ]);
    }

    #[test]
    fn map_literals_evaluate_and_print_in_insertion_order() {
        all(&[
            ("{}", "{}"),
            (r#"{name: "Ana" age: (+ 1 2)}"#, r#"{name: "Ana" age: 3}"#),
            ("{:b 1 :a 2}", "{b: 1 a: 2}"),
            (
                r#"{"a b" 1 42 [1 2] nil :k}"#,
                r#"{"a b" 1 42 [1 2] nil :k}"#,
            ),
            ("{a: {b: 1}}", "{a: {b: 1}}"),
            ("(let [k :z] {k 1})", "{z: 1}"),
            (
                "{a: 1 b: 2 c: 3 d: 4 e: 5 f: 6 g: 7 h: 8 i: 9 j: 10}",
                "{a: 1 b: 2 c: 3 d: 4 e: 5 f: 6 g: 7 h: 8 i: 9 j: 10}",
            ),
            (
                "{a: 1 b: 2 a: 3}",
                "error: Duplicate key :a in a map literal",
            ),
            (
                "{[1] 1 (list 1) 2}",
                "error: Duplicate key (1) in a map literal",
            ),
        ]);
    }

    #[test]
    fn set_literals_evaluate_and_print_in_insertion_order() {
        all(&[
            ("#{}", "#{}"),
            ("#{3 1 2}", "#{3 1 2}"),
            (r#"#{[1] :a "s"}"#, r#"#{[1] :a "s"}"#),
            ("(let [x 5] #{x (+ x 1)})", "#{5 6}"),
            ("#{1 2 1}", "error: Duplicate item 1 in a set literal"),
            (
                "#{[1] (list 1)}",
                "error: Duplicate item (1) in a set literal",
            ),
        ]);
    }

    #[test]
    fn sets_are_built_read_and_changed_without_changing_the_original() {
        all(&[
            ("(set [1 2 1 3])", "#{1 2 3}"),
            ("(set (list :a :b))", "#{:a :b}"),
            ("(set nil)", "#{}"),
            (r#"(set "aba")"#, r"#{\a \b}"),
            ("(set {a: 1})", "#{[:a 1]}"),
            ("(set 5)", "error: set expects a collection, got 5"),
            ("(set? #{})", "true"),
            ("(set? {})", "false"),
            ("(map? #{})", "false"),
            ("(count #{1 2})", "2"),
            ("(contains? #{1 2} 2)", "true"),
            ("(contains? #{1 2} 3)", "false"),
            ("(contains? #{[1 2]} (list 1 2))", "true"),
            ("(get #{[1 2]} (list 1 2))", "[1 2]"),
            ("(get #{1 2} 3 :none)", ":none"),
            ("(conj #{1} 2 1 3)", "#{1 2 3}"),
            ("(def s #{1}) (conj s 2) s", "#{1}"),
            ("(disj #{1 2 3} 2)", "#{1 3}"),
            ("(disj #{1 2 3} 1 3 9)", "#{2}"),
            ("(disj nil 1)", "nil"),
            ("(disj [1] 1)", "error: disj expects a set, got [1]"),
            ("(vec #{2 1})", "[2 1]"),
        ]);
    }

    #[test]
    fn sets_with_the_same_items_are_equal_in_any_order() {
        all(&[
            ("(= #{1 2} #{2 1})", "true"),
            ("(= #{1 2} #{1 2 3})", "false"),
            ("(= #{1 2} #{1 3})", "false"),
            ("(= #{[1]} #{(list 1)})", "true"),
            ("(= #{} {})", "false"),
            ("(= #{} [])", "false"),
            ("(= #{} nil)", "false"),
            ("(= (hash #{1 2}) (hash #{2 1}))", "true"),
            ("(= (hash #{1 2}) (hash [1 2]))", "false"),
            ("(= (hash #{}) (hash {}))", "false"),
            ("(get {#{1 2} :found} #{2 1})", ":found"),
        ]);
    }

    #[test]
    fn records_are_defined_built_and_read() {
        let point = "(defrecord Point [x y])";
        let full = "(defrecord Point [x y (z 0) {label nil}])";
        all(&[
            (point, "Point"),
            (&format!("{point} Point"), "#<record Point>"),
            (&format!("{point} (Point 1 2)"), "#Point{x: 1 y: 2}"),
            (
                &format!("{full} (Point 1 2)"),
                "#Point{x: 1 y: 2 z: 0 label: nil}",
            ),
            (
                &format!(r#"{full} (Point 1 2 3 label: "a")"#),
                r#"#Point{x: 1 y: 2 z: 3 label: "a"}"#,
            ),
            (
                "(defrecord Point [x (y (* x 2))]) (Point 3)",
                "#Point{x: 3 y: 6}",
            ),
            (&format!("{point} (def p (Point 1 2)) (+ p.x p.y)"), "3"),
            (&format!("{point} (:y (Point 1 2))"), "2"),
            (&format!("{point} (get (Point 1 2) :x)"), "1"),
            (&format!("{point} (get (Point 1 2) :w :none)"), ":none"),
            (&format!("{point} (record? (Point 1 2))"), "true"),
            ("(record? {x: 1})", "false"),
            (
                &format!("{point} (Point 1)"),
                "error: Point takes 2 arguments, got 1",
            ),
            (
                &format!("{point} (def p (Point 1 2)) p.z"),
                "error: Point has no field z",
            ),
            (
                "(defrecord Point [x & more])",
                "error: Records have no rest field",
            ),
            (
                "(defrecord Point x)",
                "error: defrecord takes a name and a field vector",
            ),
        ]);
    }

    #[test]
    fn record_literals_read_back_what_records_print() {
        let point = "(defrecord Point [x y])";
        all(&[
            (&format!("{point} #Point{{x: 1 y: 2}}"), "#Point{x: 1 y: 2}"),
            (
                &format!("{point} (= #Point{{y: 2 x: 1}} (Point 1 2))"),
                "true",
            ),
            (
                &format!("{point} (let [a 5] #Point{{x: a y: (* a 2)}})"),
                "#Point{x: 5 y: 10}",
            ),
            (
                &format!("{point} #Point{{x: 1}}"),
                "error: This Point literal is missing field y",
            ),
            (
                &format!("{point} #Point{{x: 1 y: 2 z: 3}}"),
                "error: Point has no field z",
            ),
            (
                &format!("{point} #Point{{x: 1 x: 2 y: 3}}"),
                "error: Duplicate field x in a Point literal",
            ),
            ("#Nothing{}", "error: Nothing is not defined"),
            (
                "(def Nothing 5) #Nothing{}",
                "error: 5 is not a record type",
            ),
        ]);
    }

    #[test]
    fn records_change_by_copying() {
        let point = "(defrecord Point [x y])";
        all(&[
            (
                &format!("{point} (with (Point 1 2) :x 10)"),
                "#Point{x: 10 y: 2}",
            ),
            (
                &format!("{point} (with (Point 1 2) :x 10 :y 20)"),
                "#Point{x: 10 y: 20}",
            ),
            (
                &format!("{point} (assoc (Point 1 2) :y 5)"),
                "#Point{x: 1 y: 5}",
            ),
            (
                &format!("{point} (def p (Point 1 2)) (with p :x 10) p"),
                "#Point{x: 1 y: 2}",
            ),
            (
                &format!("{point} (with (Point 1 2) :color :red)"),
                "error: Point has no field color",
            ),
            (
                &format!("{point} (assoc (Point 1 2) :color :red)"),
                "error: Point has no field color",
            ),
            (
                &format!("{point} (with (Point 1 2) :x)"),
                "error: with takes a record and field value pairs",
            ),
            (
                "(with {a: 1} :a 2)",
                "error: with expects a record, got {a: 1}",
            ),
        ]);
    }

    #[test]
    fn records_are_equal_by_type_and_fields() {
        let point = "(defrecord Point [x y])";
        all(&[
            (&format!("{point} (= (Point 1 2) (Point 1 2))"), "true"),
            (&format!("{point} (= (Point 1 2) (Point 1 3))"), "false"),
            (&format!("{point} (= (Point 1 2) {{x: 1 y: 2}})"), "false"),
            (
                &format!("{point} (defrecord Pair [x y]) (= (Point 1 2) (Pair 1 2))"),
                "false",
            ),
            (
                &format!("{point} (= (Point [1] 2) (Point (list 1) 2))"),
                "true",
            ),
            (
                &format!("{point} (= (hash (Point 1 2)) (hash (Point 1 2)))"),
                "true",
            ),
            (
                &format!("{point} (= (hash (Point 1 2)) (hash [1 2]))"),
                "false",
            ),
            (
                &format!("{point} (get {{(Point 1 2) :found}} (Point 1 2))"),
                ":found",
            ),
        ]);
    }

    #[test]
    fn keywords_maps_and_sets_can_be_called() {
        all(&[
            ("(:a {a: 1})", "1"),
            ("(:b {a: 1})", "nil"),
            ("(:b {a: 1} 0)", "0"),
            ("(:a nil)", "nil"),
            ("(:a 5)", "nil"),
            ("({a: 1} :a)", "1"),
            ("({a: 1} :b :none)", ":none"),
            ("(#{:a :b} :a)", ":a"),
            ("(#{:a :b} :c)", "nil"),
            ("(#{[1]} (list 1))", "[1]"),
            ("(def (twice f x) (f (f x))) (twice :a {a: {a: 5}})", "5"),
            ("(def (pick m) (:a m)) (pick {a: 7})", "7"),
            ("(:a)", "error: :a takes 1 to 2 arguments, got 0"),
            ("({} 1 2 3)", "error: {} takes 1 to 2 arguments, got 3"),
            ("(#{} 1 2)", "error: #{} takes 1 argument, got 2"),
            ("([1 2] 0)", "error: [1 2] is not a function"),
        ]);
    }

    #[test]
    fn fields_are_read_with_dots() {
        all(&[
            (
                r#"(def city {name: "Lyon" size: 3}) city.name"#,
                r#""Lyon""#,
            ),
            (r#"(def city {name: "Lyon"}) city.missing"#, "nil"),
            ("(def a {b: {c: {d: 4}}}) a.b.c.d", "4"),
            ("(def a {b: nil}) a.b.c.d", "nil"),
            ("(def a nil) a.b", "nil"),
            (
                r#"(def city {name: "Lyon"}) "Hi {city.name}!""#,
                r#""Hi Lyon!""#,
            ),
            ("(let [p {x: 1 y: 2}] (+ p.x p.y))", "3"),
            ("(def (f m) m.k) (f {k: :v})", ":v"),
            ("(def n 5) n.x", "error: 5 has no field x"),
            (r#"(def s "abc") s.x"#, r#"error: "abc" has no field x"#),
            ("(. {a: 1})", "error: . takes a value and field names"),
            (r#"(. {a: 1} "a")"#, "error: Field names must be names"),
        ]);
    }

    #[test]
    fn maps_are_read_by_key() {
        all(&[
            ("(get {a: 1} :a)", "1"),
            ("(get {a: 1} :b)", "nil"),
            ("(get {a: 1} :b 0)", "0"),
            ("(get {[1 2] :v} (list 1 2))", ":v"),
            ("(count {a: 1 b: 2})", "2"),
            ("(map? {})", "true"),
            ("(map? [])", "false"),
            ("(contains? {a: nil} :a)", "true"),
            ("(contains? {a: 1} :b)", "false"),
            ("(contains? nil :a)", "false"),
            (
                "(contains? [1] 0)",
                "error: contains? expects a map or set, got [1]",
            ),
            ("(keys {b: 1 a: 2})", "[:b :a]"),
            ("(vals {b: 1 a: 2})", "[1 2]"),
            ("(entries {b: 1 a: 2})", "[[:b 1] [:a 2]]"),
            ("(keys {})", "[]"),
            ("(keys nil)", "[]"),
            ("(vals 5)", "error: vals expects a map, got 5"),
            ("(vec {a: 1})", "[[:a 1]]"),
        ]);
    }

    #[test]
    fn maps_change_without_changing_the_original() {
        all(&[
            ("(assoc {a: 1} :b 2)", "{a: 1 b: 2}"),
            ("(assoc {a: 1 b: 2} :a 3)", "{a: 3 b: 2}"),
            ("(assoc {} :a 1 :b 2)", "{a: 1 b: 2}"),
            ("(assoc nil :a 1)", "{a: 1}"),
            ("(def m {a: 1}) (assoc m :b 2) m", "{a: 1}"),
            (
                "(assoc 5 :a 1)",
                "error: assoc expects a map or vector, got 5",
            ),
            ("(dissoc {a: 1 b: 2 c: 3} :b)", "{a: 1 c: 3}"),
            ("(dissoc {a: 1 b: 2 c: 3} :a :c :z)", "{b: 2}"),
            ("(def m {a: 1}) (dissoc m :a) m", "{a: 1}"),
            ("(dissoc nil :a)", "nil"),
            ("(dissoc [1] 0)", "error: dissoc expects a map, got [1]"),
            ("(conj {a: 1} [:b 2])", "{a: 1 b: 2}"),
            ("(conj {a: 1} {b: 2 a: 3})", "{a: 3 b: 2}"),
            (
                "(conj {} 5)",
                "error: conj on a map expects [key value] pairs or maps, got 5",
            ),
        ]);
    }

    #[test]
    fn vectors_and_lists_are_built_counted_and_indexed() {
        all(&[
            ("(vector 1 2 3)", "[1 2 3]"),
            ("(vector)", "[]"),
            ("(list 1 2 3)", "(1 2 3)"),
            ("(list)", "nil"),
            ("(vector? [1])", "true"),
            ("(vector? (list 1))", "false"),
            ("(count [1 2 3])", "3"),
            ("(count (list 1 2))", "2"),
            ("(nth [:a :b :c] 2)", ":c"),
            ("(nth (list :a :b :c) 1)", ":b"),
            (
                "(nth [:a] 3)",
                "error: Index 3 is out of range for a vector of 1 item",
            ),
            (
                "(nth (list :a :b) 2)",
                "error: Index 2 is out of range for a list of 2 items",
            ),
            ("(nth 5 0)", "error: nth expects a collection, got 5"),
            ("(get [1 2 3] 1)", "2"),
            ("(get [1 2 3] 5)", "nil"),
            ("(get [1 2 3] 5 :none)", ":none"),
            ("(get [1 2 3] :a)", "nil"),
            (r#"(get "abc" 2)"#, r"\c"),
            ("(get nil 0)", "nil"),
            ("(vec (list 1 2))", "[1 2]"),
            ("(vec [1 2])", "[1 2]"),
            ("(vec nil)", "[]"),
            (r#"(vec "héllo")"#, r"[\h \é \l \l \o]"),
            ("(vec 5)", "error: vec expects a collection, got 5"),
        ]);
    }

    #[test]
    fn vectors_and_lists_change_without_changing_the_original() {
        all(&[
            ("(conj [1 2] 3)", "[1 2 3]"),
            ("(conj [] 1 2 3)", "[1 2 3]"),
            ("(conj (list 1 2) 0)", "(0 1 2)"),
            ("(conj nil 1 2)", "(2 1)"),
            ("(def v [1 2 3]) (conj v 4) v", "[1 2 3]"),
            ("(conj 5 1)", "error: conj expects a collection, got 5"),
            ("(assoc [1 2 3] 0 :a)", "[:a 2 3]"),
            ("(assoc [1 2 3] 3 4)", "[1 2 3 4]"),
            ("(assoc [1 2 3] 0 :a 2 :c)", "[:a 2 :c]"),
            ("(def v [1 2 3]) (assoc v 1 :b) v", "[1 2 3]"),
            (
                "(assoc [1 2 3] 5 :x)",
                "error: Index 5 is out of range for a vector of 3 items",
            ),
            (
                "(assoc [1 2 3] 0 :a 1)",
                "error: assoc takes a collection and key value pairs",
            ),
            (
                "(assoc [1] :k 2)",
                "error: assoc expects an integer index, got :k",
            ),
            ("(pop [1 2 3])", "[1 2]"),
            ("(pop (list 1 2 3))", "(2 3)"),
            ("(pop nil)", "nil"),
            ("(pop [])", "error: Cannot pop an empty vector"),
            ("(peek [1 2 3])", "3"),
            ("(peek (list 1 2 3))", "1"),
            ("(peek [])", "nil"),
            ("(peek 5)", "error: peek expects a vector or list, got 5"),
        ]);
    }

    #[test]
    fn equality_is_exact_and_structural() {
        all(&[
            ("(= 1 1)", "true"),
            ("(= 1 1.0)", "false"),
            ("(= 1.5 1.5)", "true"),
            (r#"(= "abc" (str "a" "bc"))"#, "true"),
            (r#"(= "abc" "abd")"#, "false"),
            ("(= :a :a)", "true"),
            (r"(= \a \a)", "true"),
            ("(= nil nil)", "true"),
            ("(= nil false)", "false"),
            (
                "(= 100000000000000000000 (* 10000000000 10000000000))",
                "true",
            ),
            ("(= 1)", "true"),
            ("(= 1 1 1)", "true"),
            ("(= 1 1 2)", "false"),
            (
                r#"(def (f & xs) xs) (= (f 1 "a" (f 2)) (f 1 "a" (f 2)))"#,
                "true",
            ),
            ("(def (f & xs) xs) (= (f 1 2) (f 1 2 3))", "false"),
            ("(= + +)", "true"),
            ("(= (fn [] 1) (fn [] 1))", "false"),
            ("(not= 1 2)", "true"),
            ("(not= 1 1)", "false"),
        ]);
    }

    #[test]
    fn lists_and_vectors_with_the_same_items_are_equal() {
        all(&[
            ("(= [1 2] [1 2])", "true"),
            ("(= [1 2] [2 1])", "false"),
            ("(= [1 2] (list 1 2))", "true"),
            ("(= (list 1 2) [1 2])", "true"),
            ("(= [1 [2 3]] (list 1 (list 2 3)))", "true"),
            ("(= [1 2] (list 1 2 3))", "false"),
            ("(= [1 2 3] (list 1 2))", "false"),
            ("(= [1 1.0] [1 1])", "false"),
            ("(= [] nil)", "true"),
            ("(= nil [])", "true"),
            ("(= [] (list))", "true"),
            ("(= [nil] [])", "false"),
            ("(= [nil] nil)", "false"),
            (r#"(= [] "")"#, "false"),
            ("(not= [1] (list 1))", "false"),
        ]);
    }

    #[test]
    fn maps_with_the_same_entries_are_equal_in_any_order() {
        let ten = "{a: 1 b: 2 c: 3 d: 4 e: 5 f: 6 g: 7 h: 8 i: 9 j: 10}";
        let backwards = "{j: 10 i: 9 h: 8 g: 7 f: 6 e: 5 d: 4 c: 3 b: 2 a: 1}";
        all(&[
            ("(= {a: 1 b: 2} {b: 2 a: 1})", "true"),
            (&format!("(= {ten} {backwards})"), "true"),
            ("(= {a: 1} {a: 1 b: 2})", "false"),
            ("(= {a: 1 b: 2} {a: 1})", "false"),
            ("(= {a: 1} {a: 2})", "false"),
            ("(= {a: 1} {b: 1})", "false"),
            ("(= {a: [1 2]} {a: (list 1 2)})", "true"),
            ("(= {} {})", "true"),
            ("(= {} nil)", "false"),
            ("(= {} [])", "false"),
            ("(= (dissoc {a: 1 b: 2} :a) {b: 2})", "true"),
            ("(= (hash {a: 1 b: 2}) (hash {b: 2 a: 1}))", "true"),
            (&format!("(= (hash {ten}) (hash {backwards}))"), "true"),
            ("(= (hash {a: 1}) (hash {a: 2}))", "false"),
            ("(= (hash {a: 1 b: 2}) (hash [[:a 1] [:b 2]]))", "false"),
            ("(get {{a: 1 b: 2} :found} {b: 2 a: 1})", ":found"),
        ]);
    }

    #[test]
    fn equal_values_hash_alike() {
        all(&[
            ("(= (hash [1 2]) (hash (list 1 2)))", "true"),
            ("(= (hash [1 [2]]) (hash (list 1 (list 2))))", "true"),
            ("(= (hash []) (hash nil))", "true"),
            (r#"(= (hash "ab") (hash (str "a" "b")))"#, "true"),
            (
                "(= (hash 100000000000000000000) (hash (* 10000000000 10000000000)))",
                "true",
            ),
            ("(= (hash :a) (hash :a))", "true"),
            ("(= (hash 1) (hash 2))", "false"),
            ("(= (hash [1 2]) (hash [2 1]))", "false"),
            ("(= (hash [1]) (hash 1))", "false"),
            (
                "(def big (* 1e308 10)) (= (hash (- big big)) (hash (- big big)))",
                "true",
            ),
        ]);
    }

    #[test]
    fn equality_on_floats_is_an_equivalence() {
        let not_a_number = "(def big (* 1e308 10)) (def x (- big big))";
        all(&[
            (&format!("{not_a_number} (= x x)"), "true"),
            (&format!("{not_a_number} (= x (- big big))"), "true"),
            (&format!("{not_a_number} (== x x)"), "false"),
            ("(= 0.0 -0.0)", "false"),
            ("(== 0.0 -0.0)", "true"),
        ]);
    }

    #[test]
    fn numeric_equality_compares_numbers_by_value() {
        all(&[
            ("(== 1 1.0)", "true"),
            ("(== 1 1.0 1)", "true"),
            ("(== 1 2)", "false"),
            ("(== 9223372036854775808 9223372036854775808.0)", "true"),
            (r#"(== "a" 1)"#, r#"error: == expects numbers, got "a""#),
        ]);
    }

    #[test]
    fn identical_compares_references() {
        all(&[
            (r#"(def s "abc") (identical? s s)"#, "true"),
            (r#"(identical? (str "a") (str "a"))"#, "false"),
            ("(identical? :a :a)", "true"),
            ("(identical? 1 1)", "true"),
            ("(identical? 1 1.0)", "false"),
        ]);
    }

    #[test]
    fn each_field_site_scans_a_record_type_once() {
        let mut isolate = Isolate::new();
        isolate
            .eval(
                "(defrecord P [a b c d]) (def (sum p n total) (if (< n 1) total (sum p (- n 1) (+ total p.d))))",
            )
            .unwrap();

        assert_eq!(isolate.eval("(sum (P 1 2 3 4) 100 0)"), Ok(Value::Int(400)));
        assert_eq!(isolate.runtime.field_scans, 1);
    }

    #[test]
    fn field_sites_follow_a_change_of_record_type() {
        all(&[(
            "(defrecord A [x y]) (defrecord B [y x]) (def (gety r) r.y) [(gety (A 1 2)) (gety (B 3 4)) (gety (A 5 6)) (gety {y: 7})]",
            "[2 3 6 7]",
        )]);
    }

    #[test]
    fn rust_functions_can_be_called_from_ox() {
        let mut isolate = Isolate::new();
        isolate.define_native("twice", 1, Some(1), |_, arguments| match arguments {
            [Value::Int(number)] => Ok(Value::Int(number * 2)),
            _ => Err(Error::new("twice expects an integer")),
        });

        assert_eq!(isolate.eval("(twice 21)"), Ok(Value::Int(42)));
        assert_eq!(
            isolate.eval("(twice)").unwrap_err().message,
            "twice takes 1 argument, got 0"
        );
    }
}
