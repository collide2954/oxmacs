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
            ("(+ 9223372036854775807 1)", "error: Integer overflow"),
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
