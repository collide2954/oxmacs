use std::rc::Rc;

use crate::compile::compile;
use crate::error::{Error, ErrorKind};
use crate::expand::expand;
use crate::heap::{Heap, Rooted};
use crate::library;
use crate::native::Native;
use crate::read;
use crate::symbol::Sym;
use crate::value::Value;
use crate::value::print;
use crate::value::record::Family;
use crate::vm::Runtime;

const PRELUDE: &str = include_str!("library/prelude/core.ox");

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
        library::install(&mut isolate);
        let apply = isolate.runtime.globals.get(&Sym::intern("apply")).copied();
        isolate.runtime.apply = apply.unwrap_or(Value::Nil);
        if let Err(error) = isolate.eval(PRELUDE) {
            panic!("The Ox prelude failed to load: {error}");
        }
        let found: Vec<(ErrorKind, Value)> = ErrorKind::NAMES
            .iter()
            .filter_map(|(kind, name)| {
                let value = isolate.runtime.globals.get(&Sym::intern(name))?;
                Some((*kind, *value))
            })
            .collect();
        for (kind, value) in found {
            if kind == ErrorKind::Other {
                isolate.runtime.heap.set_family(value, Family::AnyError);
            }
            isolate.runtime.error_types.insert(kind, value);
        }
        isolate
    }

    pub fn eval(&mut self, source: &str) -> Result<Value, Error> {
        let (forms, errors) = read::read(source);
        if let Some(error) = errors.into_iter().next() {
            return Err(Error::at(error.span, error.message));
        }
        let mut result = Value::Nil;
        for form in &forms {
            let expanded = expand(&mut self.runtime, form)?;
            let code = Rc::new(compile(&mut self.runtime.heap, &expanded)?);
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

    pub fn collect(&mut self) -> usize {
        self.runtime.collect()
    }

    pub fn root(&mut self, value: Value) -> Rooted {
        self.runtime.heap.root(value)
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
    fn an_empty_source_gives_nil() {
        assert_eq!(evaluated(""), "nil");
    }

    #[test]
    fn errors_point_at_the_call_that_failed() {
        let error = Isolate::new().eval("(+ 1\n (/ 1 0))").unwrap_err();

        assert_eq!(error.span, Some(Span::new(6, 13)));
    }

    #[test]
    fn globals_last_between_evaluations() {
        let mut isolate = Isolate::new();

        isolate.eval("(def y 5)").unwrap();

        assert_eq!(isolate.eval("(* y y)"), Ok(Value::Int(25)));
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
    fn collection_keeps_everything_reachable_from_globals() {
        let mut isolate = Isolate::new();
        isolate
            .eval(
                r#"(defrecord P [x]) (def keep {v: [1 (list "s" #{:k 99999999999999999999})] r: (P (atom [2])) f: (let [n (str "capt" "ured")] (fn [] n))})"#,
            )
            .unwrap();
        isolate.collect();
        isolate
            .eval("(def junk (map (fn [i] [i i]) (range 1000))) (def junk nil)")
            .unwrap();
        isolate.collect();

        let kept = isolate.eval("keep").unwrap();
        assert_eq!(
            isolate.write(kept),
            r#"{v: [1 ("s" #{:k 99999999999999999999})] r: #P{x: #<atom [2]>} f: #<fn>}"#
        );
        let captured = isolate.eval("((:f keep))").unwrap();
        assert_eq!(isolate.write(captured), r#""captured""#);
        let record = isolate.eval("(P 5)").unwrap();
        assert_eq!(isolate.write(record), "#P{x: 5}");
    }

    #[test]
    fn garbage_from_a_long_loop_is_collected_while_it_runs() {
        let mut isolate = Isolate::new();
        let before = isolate.runtime.heap.in_use();

        let done = isolate.eval(
            "(def (churn n) (if (< n 1) :done (do [n n n] (str n) (list n n) (churn (- n 1))))) (churn 100000)",
        );

        assert_eq!(done, Ok(Value::keyword("done")));
        assert!(isolate.runtime.heap.in_use() < before + 200_000);
    }

    #[test]
    fn data_built_while_collections_run_survives_them() {
        let mut isolate = Isolate::new();
        isolate
            .eval("(def (build n acc) (if (< n 1) acc (build (- n 1) (assoc acc n [n (str n) (list n)])))) (def big (build 30000 {}))")
            .unwrap();

        let total = isolate
            .eval("(reduce + (map (fn [k] (first (get big k))) (keys big)))")
            .unwrap();
        let one = isolate.eval("(get big 12345)").unwrap();

        assert_eq!(total, Value::Int(450_015_000));
        assert_eq!(isolate.write(one), r#"[12345 "12345" (12345)]"#);
    }

    #[test]
    fn rust_keeps_values_alive_with_rooted_handles() {
        let mut isolate = Isolate::new();
        let value = isolate.eval(r#"(str "he" "ld")"#).unwrap();
        let handle = isolate.root(value);

        isolate.collect();
        assert_eq!(isolate.write(handle.get()), r#""held""#);

        drop(handle);
        isolate.collect();
        assert_eq!(isolate.runtime.heap.text(value), None);
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
