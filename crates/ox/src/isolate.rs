use std::any::Any;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use crate::compile::compile;
use crate::error::{Error, ErrorKind};
use crate::expand::expand;
use crate::heap::{Heap, Rooted};
use crate::library;
use crate::native::{Control, Native, NativeFn};
use crate::portable::{Failure, Portable};
use crate::read::{self, Datum, Syntax};
use crate::span::{Source, Span};
use crate::symbol::Sym;
use crate::task::{Interrupter, Report, TaskId};
use crate::value::Value;
use crate::value::print;
use crate::value::promise::Promise;
use crate::value::record::Family;
use crate::vm::Runtime;
use crate::vm::build::full;
use crate::vm::modules::{Import, ModuleId, PRELUDE, USER};

const PRELUDE_SOURCE: &str = include_str!("library/prelude/core.ox");

pub(crate) fn work(
    roots: &[PathBuf],
    setup: Option<fn(&mut Isolate)>,
    module: &str,
    function: Sym,
    arguments: &[Portable],
) -> Result<Portable, Failure> {
    let mut isolate = Isolate::new();
    if let Some(setup) = setup {
        setup(&mut isolate);
        isolate.set_worker_setup(setup);
    }
    for root in roots {
        isolate.add_root(root.clone());
    }
    let failed = |isolate: &mut Isolate, error: Error| {
        let value = isolate
            .runtime
            .error_value(&error)
            .ok()
            .and_then(|value| isolate.runtime.portable(value, "from a worker").ok());
        Failure {
            value,
            message: error.message,
        }
    };
    match isolate.job(module, function, arguments) {
        Ok(value) => isolate
            .runtime
            .portable(value, "from a worker")
            .map_err(|error| failed(&mut isolate, error)),
        Err(error) => Err(failed(&mut isolate, error)),
    }
}

#[derive(Debug)]
pub struct Isolate {
    runtime: Runtime,
    loading: Vec<String>,
}

impl Default for Isolate {
    fn default() -> Self {
        Self::new()
    }
}

fn symbol(syntax: &Syntax) -> Option<&str> {
    match &syntax.datum {
        Datum::Symbol(name) => Some(name),
        _ => None,
    }
}

fn header(form: &Syntax) -> Option<&[Syntax]> {
    match &form.datum {
        Datum::List(items) if items.first().and_then(symbol) == Some("module") => Some(&items[1..]),
        _ => None,
    }
}

impl Isolate {
    pub fn new() -> Self {
        let mut isolate = Self {
            runtime: Runtime::default(),
            loading: Vec::new(),
        };
        library::install(&mut isolate);
        let apply = isolate.runtime.lookup(PRELUDE, Sym::intern("apply"));
        isolate.runtime.apply = apply.unwrap_or(Value::Nil);
        if let Err(error) = isolate.eval_in(PRELUDE, "prelude/core.ox", PRELUDE_SOURCE) {
            panic!("The Ox prelude failed to load: {error}");
        }
        let found: Vec<(ErrorKind, Value)> = ErrorKind::NAMES
            .iter()
            .filter_map(|(kind, name)| {
                let value = isolate.runtime.lookup(PRELUDE, Sym::intern(name))?;
                Some((*kind, value))
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
        self.eval_in(USER, "<input>", source)
    }

    pub fn eval_named(&mut self, name: &str, source: &str) -> Result<Value, Error> {
        self.eval_in(USER, name, source)
    }

    pub fn add_root(&mut self, root: impl Into<PathBuf>) {
        self.runtime.roots.push(root.into());
    }

    pub fn set_worker_setup(&mut self, setup: fn(&mut Isolate)) {
        self.runtime.worker_setup = Some(setup);
    }

    pub fn define(&mut self, name: &str, value: Value) {
        self.runtime.define(PRELUDE, Sym::intern(name), value);
    }

    pub fn call(&mut self, function: Value, arguments: &[Value]) -> Result<Value, Error> {
        self.runtime.invoke(function, arguments, Span::new(0, 0))
    }

    pub fn foreign<T: Any>(&mut self, name: &str, value: T) -> Result<Value, Error> {
        self.runtime.heap.foreign(name, value).map_err(full)
    }

    pub fn shared<T: Any + Send + Sync>(
        &mut self,
        name: &str,
        value: Arc<T>,
    ) -> Result<Value, Error> {
        self.runtime.heap.shared(name, value).map_err(full)
    }

    fn job(&mut self, module: &str, function: Sym, arguments: &[Portable]) -> Result<Value, Error> {
        let id = self.load_module(module)?;
        let callee = self
            .runtime
            .modules
            .get(id)
            .and_then(|found| found.globals.get(&function))
            .copied()
            .ok_or_else(|| {
                Error::new(format!(
                    "{module} has no function named {}",
                    function.name()
                ))
            })?;
        let arguments = arguments
            .iter()
            .map(|argument| self.runtime.rebuild(argument))
            .collect::<Result<Vec<_>, Error>>()?;
        self.call(callee, &arguments)
    }

    pub fn load(&mut self, name: &str) -> Result<(), Error> {
        self.load_module(name).map(|_| ())
    }

    fn eval_in(&mut self, module: ModuleId, name: &str, source: &str) -> Result<Value, Error> {
        let text = Rc::new(Source {
            name: name.to_owned(),
            text: source.to_owned(),
        });
        let (forms, errors) = read::read(source);
        if let Some(error) = errors.into_iter().next() {
            return Err(Error::at(error.span, error.message));
        }
        let mut result = Value::Nil;
        for form in &forms {
            if let Some(clauses) = header(form) {
                self.header(module, clauses)?;
                result = Value::Nil;
                continue;
            }
            let expanded = expand(&mut self.runtime, form, module)?;
            let module_name = self
                .runtime
                .modules
                .get(module)
                .map_or_else(|| Sym::intern("user"), |found| Sym::intern(&found.name));
            let code = Rc::new(compile(
                &mut self.runtime.heap,
                &expanded,
                Some(Rc::clone(&text)),
                module,
                module_name,
            )?);
            result = self.runtime.run(&code)?;
        }
        Ok(result)
    }

    fn load_module(&mut self, name: &str) -> Result<ModuleId, Error> {
        if let Some(start) = self.loading.iter().position(|loading| loading == name) {
            let mut chain = self.loading[start..].to_vec();
            chain.push(name.to_owned());
            return Err(Error::new(format!("Import cycle: {}", chain.join(" → "))));
        }
        let path = self
            .runtime
            .roots
            .iter()
            .map(|root| root.join(format!("{name}.ox")))
            .find(|path| path.is_file())
            .ok_or_else(|| Error::new(format!("No module named {name}")))?;
        let source = fs::read_to_string(&path)
            .map_err(|error| Error::new(format!("{}: {error}", path.display())))?;
        let module = self.runtime.module_named(name);
        if let Some(found) = self.runtime.modules.get_mut(module) {
            found.imports.clear();
            found.exports = None;
        }
        self.loading.push(name.to_owned());
        let result = self.eval_in(module, &path.display().to_string(), &source);
        self.loading.pop();
        result.map(|_| module)
    }

    fn header(&mut self, module: ModuleId, clauses: &[Syntax]) -> Result<(), Error> {
        for clause in clauses {
            let malformed = || {
                Error::at(
                    clause.span,
                    "module takes (import …) and (export …) clauses",
                )
            };
            let Datum::List(items) = &clause.datum else {
                return Err(malformed());
            };
            let Some((head, rest)) = items.split_first() else {
                return Err(malformed());
            };
            match symbol(head) {
                Some("import") => {
                    for spec in rest {
                        self.import(module, spec)?;
                    }
                }
                Some("export") => {
                    let names: HashSet<Sym> =
                        rest.iter().filter_map(symbol).map(Sym::intern).collect();
                    if let Some(found) = self.runtime.modules.get_mut(module) {
                        found.exports.get_or_insert_with(HashSet::new).extend(names);
                    }
                }
                _ => return Err(malformed()),
            }
        }
        Ok(())
    }

    fn import(&mut self, module: ModuleId, spec: &Syntax) -> Result<(), Error> {
        let malformed = || Error::at(spec.span, "import takes (module name …) lists");
        let Datum::List(items) = &spec.datum else {
            return Err(malformed());
        };
        let segments: Vec<&str> = items.iter().map_while(symbol).collect();
        let Some(last) = segments.last() else {
            return Err(malformed());
        };
        let mut alias = Sym::intern(last);
        let mut only = Vec::new();
        let mut options = items[segments.len()..].iter();
        while let Some(option) = options.next() {
            let value = options.next().ok_or_else(malformed)?;
            match (&option.datum, &value.datum) {
                (Datum::Keyword(word), Datum::Symbol(name)) if word == "as" => {
                    alias = Sym::intern(name);
                }
                (Datum::Keyword(word), Datum::Vector(names)) if word == "only" => {
                    only.extend(names.iter().filter_map(symbol).map(Sym::intern));
                }
                _ => return Err(malformed()),
            }
        }
        let name = segments.join("/");
        let loaded = self.runtime.module_ids.get(&name).copied();
        let id = match loaded {
            Some(id) if !self.loading.contains(&name) => id,
            _ => self
                .load_module(&name)
                .map_err(|error| error.or_at(spec.span))?,
        };
        if let Some(found) = self.runtime.modules.get_mut(module) {
            found.imports.retain(|import| import.module != id);
            found.imports.push(Import {
                module: id,
                alias,
                only,
            });
        }
        Ok(())
    }

    pub fn define_native(
        &mut self,
        name: &str,
        least: usize,
        most: Option<usize>,
        function: impl Fn(&mut Heap, &[Value]) -> Result<Value, Error> + 'static,
    ) {
        self.install_native(name, least, most, Rc::new(function), None);
    }

    pub(crate) fn define_control(
        &mut self,
        name: &str,
        least: usize,
        most: Option<usize>,
        control: Control,
    ) {
        let message = format!("{name} can only be called from Ox");
        let function: Rc<NativeFn> = Rc::new(move |_, _| Err(Error::new(message.clone())));
        self.install_native(name, least, most, function, Some(control));
    }

    fn install_native(
        &mut self,
        name: &str,
        least: usize,
        most: Option<usize>,
        function: Rc<NativeFn>,
        control: Option<Control>,
    ) {
        let name = Sym::intern(name);
        if let Ok(value) = self.runtime.heap.native(Native {
            name,
            least,
            most,
            function,
            control,
        }) {
            self.runtime.define(PRELUDE, name, value);
        }
    }

    pub fn promise(&mut self) -> Result<Value, Error> {
        self.runtime.promise()
    }

    pub fn resolve(&mut self, promise: Value, value: Value) -> bool {
        self.runtime.settle(promise, Promise::Delivered(value))
    }

    pub fn reject(&mut self, promise: Value, message: &str) -> bool {
        let Ok(error) = self.runtime.error_value(&Error::new(message)) else {
            return false;
        };
        self.runtime.settle(promise, Promise::Failed(error))
    }

    pub fn spawn(&mut self, function: Value, arguments: &[Value]) -> Result<TaskId, Error> {
        self.runtime.spawn(function, arguments)
    }

    pub fn run(&mut self, budget: Duration) -> Report {
        self.runtime.run_tasks(budget)
    }

    pub fn cancel(&mut self, task: TaskId) -> bool {
        self.runtime.cancel(task)
    }

    pub fn interrupter(&self) -> Interrupter {
        self.runtime.interrupter.clone()
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
    fn uncaught_errors_carry_their_frames() {
        let mut isolate = Isolate::new();

        let error = isolate
            .eval_named("demo.ox", "(def (f)\n  (/ 1 0))\n(f)")
            .unwrap_err();

        let frames: Vec<String> = error.traceback.iter().map(ToString::to_string).collect();
        assert_eq!(frames, ["f at demo.ox:2:3", "top level at demo.ox:3:1"]);
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
