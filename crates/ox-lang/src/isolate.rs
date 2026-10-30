use std::any::Any;
use std::cell::OnceCell;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use crate::builtins;
use crate::bytecode::Prim;
use crate::check::{self, Diagnostic, Occurrence};
use crate::compile::compile;
use crate::describe::{self, Description, Docs, Kind, ModuleDocs};
use crate::error::{Error, ErrorKind};
use crate::expand::expand;
use crate::forms::Forms;
use crate::heap::{Closure, Heap, Hold, Rooted};
use crate::native::{Control, Native, NativeFn};
use crate::portable::{Failure, Portable};
use crate::profile::Profile;
use crate::read::{self, Datum, Syntax, split_meta};
use crate::sink::Sink;
use crate::span::{Source, Span};
use crate::symbol::Sym;
use crate::task::{Interrupter, Report, TaskId, Wake};
use crate::value::Value;
use crate::value::print;
use crate::value::promise::Promise;
use crate::value::record::Family;
use crate::vm::modules::{Import, Module, ModuleId, PRELUDE, USER};
use crate::vm::{Runtime, Sources};

const LIBRARY_DOCS: &str = include_str!("builtins/docs.ox");
const TASK_SLICE: Duration = Duration::from_millis(50);
const IDLE_POLL: Duration = Duration::from_millis(2);

fn modified(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).and_then(|found| found.modified()).ok()
}

pub(crate) fn shown_name(module: &str, name: &str) -> String {
    match module.rsplit_once('/') {
        _ if module == "ox" => name.to_owned(),
        Some((_, last)) => format!("{last}/{name}"),
        None => format!("{module}/{name}"),
    }
}

pub(crate) fn plain(
    module: &str,
    name: &str,
    least: usize,
    most: Option<usize>,
    function: Rc<NativeFn>,
) -> Native {
    Native {
        name: Sym::intern(&shown_name(module, name)),
        least,
        most,
        function,
        control: None,
        suspends: false,
        outside: false,
        keywords: Box::new([]),
        resumable: None,
        drains: None,
        captured: Box::new([]),
    }
}

pub(crate) fn only_from_ox(name: &str) -> Rc<NativeFn> {
    let message = format!("{name} can only be called from Ox");
    Rc::new(move |_, _, _| Err(Error::new(message.clone())))
}

pub(crate) fn work(
    roots: &[PathBuf],
    sources: Sources,
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
    isolate.runtime.sources = sources;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stubs {
    All,
    Modules,
    Globals,
}

#[derive(Debug)]
pub struct Isolate {
    runtime: Runtime,
    loading: Vec<String>,
    files: HashMap<String, (PathBuf, Option<SystemTime>)>,
    broken: HashSet<String>,
    packages: HashMap<String, Vec<String>>,
    doc_tables: Vec<&'static str>,
    docs: OnceCell<Docs>,
    summaries: Vec<(String, String)>,
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

fn defined_name(form: &Syntax) -> Option<Sym> {
    let Datum::List(items) = &form.datum else {
        return None;
    };
    let (head, rest) = items.split_first()?;
    let target = split_meta(rest).1.first()?;
    if !matches!(
        symbol(head),
        Some("def" | "defonce" | "defrecord" | "deferror")
    ) {
        return None;
    }
    let name = match &target.datum {
        Datum::List(signature) => symbol(signature.first()?)?,
        _ => symbol(target)?,
    };
    Some(Sym::intern(name))
}

impl Isolate {
    pub fn new() -> Self {
        let mut isolate = Self {
            runtime: Runtime::default(),
            loading: Vec::new(),
            files: HashMap::new(),
            broken: HashSet::new(),
            packages: HashMap::new(),
            doc_tables: vec![LIBRARY_DOCS],
            docs: OnceCell::new(),
            summaries: Vec::new(),
        };
        builtins::install(&mut isolate);
        let apply = isolate.runtime.lookup(PRELUDE, Sym::intern("apply"));
        isolate.runtime.apply = apply.unwrap_or(Value::Nil);
        for prim in Prim::ALL {
            let found = isolate
                .runtime
                .lookup(PRELUDE, Sym::intern(prim.name()))
                .filter(|value| {
                    isolate
                        .runtime
                        .heap
                        .native_of(*value)
                        .is_some_and(|native| {
                            native.name.name() == prim.name() && native.control.is_none()
                        })
                });
            isolate.runtime.prims[prim as usize] = found.unwrap_or(Value::Nil);
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
        self.eval_in(USER, "<input>", source, &mut ())
    }

    pub fn eval_with(&mut self, host: &mut dyn Any, source: &str) -> Result<Value, Error> {
        self.eval_in(USER, "<input>", source, host)
    }

    pub fn eval_named(&mut self, name: &str, source: &str) -> Result<Value, Error> {
        self.eval_in(USER, name, source, &mut ())
    }

    pub fn eval_named_with(
        &mut self,
        host: &mut dyn Any,
        name: &str,
        source: &str,
    ) -> Result<Value, Error> {
        self.eval_in(USER, name, source, host)
    }

    pub fn hold(&mut self, value: Value) -> Hold {
        self.runtime.heap.hold(value)
    }

    pub fn held(&self, hold: Hold) -> Option<Value> {
        self.runtime.heap.held(hold)
    }

    pub fn extend_prelude(&mut self, name: &str, source: &str) -> Result<Value, Error> {
        self.eval_in(PRELUDE, name, source, &mut ())
    }

    pub fn release(&mut self, hold: Hold) -> Option<Value> {
        self.runtime.heap.release(hold)
    }

    pub fn add_source(&mut self, module: &str, file: &str, text: &str) {
        Arc::make_mut(&mut self.runtime.sources)
            .insert(module.to_owned(), (file.to_owned(), text.to_owned()));
    }

    pub fn limit_imports(&mut self, package: &str, requires: &[String]) {
        self.packages.insert(package.to_owned(), requires.to_vec());
    }

    fn may_import(&self, importer: ModuleId, name: &str) -> Result<(), String> {
        let Some(importer) = self.runtime.modules.get(importer).map(|found| &found.name) else {
            return Ok(());
        };
        let package = |module: &str| module.split('/').next().unwrap_or(module).to_owned();
        let (from, to) = (package(importer), package(name));
        match self.packages.get(&from) {
            Some(requires)
                if from != to && self.packages.contains_key(&to) && !requires.contains(&to) =>
            {
                Err(format!(
                    "{importer} imports {name} without requiring {to}; add {to} to {from}'s :requires"
                ))
            }
            _ => Ok(()),
        }
    }

    pub fn eval_module_with(
        &mut self,
        host: &mut dyn Any,
        module: &str,
        file: &str,
        source: &str,
    ) -> Result<(), Error> {
        self.evaluate_module(module, file, source, host).map(|_| ())
    }

    pub fn heap(&self) -> &Heap {
        &self.runtime.heap
    }

    pub fn heap_mut(&mut self) -> &mut Heap {
        &mut self.runtime.heap
    }

    pub(crate) fn runtime_mut(&mut self) -> &mut Runtime {
        &mut self.runtime
    }

    pub fn add_root(&mut self, root: impl Into<PathBuf>) {
        let root = root.into();
        if !self.runtime.roots.contains(&root) {
            self.runtime.roots.push(root);
        }
    }

    pub fn set_worker_setup(&mut self, setup: fn(&mut Isolate)) {
        self.runtime.worker_setup = Some(setup);
    }

    pub fn define(&mut self, name: &str, value: Value) {
        self.runtime.define(PRELUDE, Sym::intern(name), value);
    }

    pub fn within<T>(
        &mut self,
        budget: Duration,
        work: impl FnOnce(&mut Self) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let outer = self.runtime.budget;
        let deadline = Instant::now() + budget;
        self.runtime.budget = match outer {
            Some((earlier, _)) if earlier <= deadline => outer,
            _ => Some((deadline, budget)),
        };
        let result = work(self);
        self.runtime.budget = outer;
        result
    }

    pub fn call(&mut self, function: Value, arguments: &[Value]) -> Result<Value, Error> {
        self.call_with(&mut (), function, arguments)
    }

    pub fn call_with(
        &mut self,
        host: &mut dyn Any,
        function: Value,
        arguments: &[Value],
    ) -> Result<Value, Error> {
        self.runtime
            .invoke(function, arguments, Span::new(0, 0), host)
    }

    pub fn foreign<T: Any>(&mut self, name: &str, value: T) -> Result<Value, Error> {
        self.runtime.heap.foreign(name, value).map_err(Error::from)
    }

    pub fn shared<T: Any + Send + Sync>(
        &mut self,
        name: &str,
        value: Arc<T>,
    ) -> Result<Value, Error> {
        self.runtime.heap.shared(name, value).map_err(Error::from)
    }

    fn job(&mut self, module: &str, function: Sym, arguments: &[Portable]) -> Result<Value, Error> {
        let id = self.load_module(module, &mut ())?;
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
        self.load_module(name, &mut ()).map(|_| ())
    }

    fn eval_in(
        &mut self,
        module: ModuleId,
        name: &str,
        source: &str,
        host: &mut dyn Any,
    ) -> Result<Value, Error> {
        let text = Rc::new(Source {
            name: name.to_owned(),
            text: source.to_owned(),
        });
        let (forms, errors) = read::read(source);
        if let Some(error) = errors.into_iter().next() {
            return Err(Error::at(error.span, error.message));
        }
        if let Some(found) = self.runtime.modules.get_mut(module) {
            found.defined.extend(forms.iter().filter_map(defined_name));
        }
        let mut result = Value::Nil;
        for form in &forms {
            if let Some(clauses) = header(form) {
                self.header(module, clauses, host)?;
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
            result = self.runtime.run(&code, host)?;
        }
        Ok(result)
    }

    fn load_module(&mut self, name: &str, host: &mut dyn Any) -> Result<ModuleId, Error> {
        if let Some(start) = self.loading.iter().position(|loading| loading == name) {
            let mut chain = self.loading[start..].to_vec();
            chain.push(name.to_owned());
            return Err(Error::new(format!("Import cycle: {}", chain.join(" → "))));
        }
        if let Some((file, source)) = self.runtime.sources.get(name).cloned() {
            return self.evaluate_module(name, &file, &source, host);
        }
        let path = self
            .runtime
            .roots
            .iter()
            .map(|root| root.join(format!("{name}.ox")))
            .find(|path| path.is_file())
            .ok_or_else(|| Error::new(format!("No module named {name}")))?;
        let changed = modified(&path);
        let source = fs::read_to_string(&path)
            .map_err(|error| Error::new(format!("{}: {error}", path.display())))?;
        let result = self.evaluate_module(name, &path.display().to_string(), &source, host);
        if result.is_ok() {
            self.files.insert(name.to_owned(), (path, changed));
        }
        result
    }

    fn stale(&self, name: &str, seen: &mut HashSet<String>) -> bool {
        if !seen.insert(name.to_owned()) {
            return false;
        }
        if self.broken.contains(name)
            || self
                .files
                .get(name)
                .is_some_and(|(path, changed)| modified(path) != *changed)
        {
            return true;
        }
        let Some(module) = self
            .runtime
            .module_ids
            .get(name)
            .and_then(|id| self.runtime.modules.get(*id))
        else {
            return false;
        };
        module.imports.iter().any(|import| {
            self.runtime
                .modules
                .get(import.module)
                .is_some_and(|found| self.stale(&found.name, seen))
        })
    }

    fn evaluate_module(
        &mut self,
        name: &str,
        file: &str,
        source: &str,
        host: &mut dyn Any,
    ) -> Result<ModuleId, Error> {
        let module = self.runtime.module_named(name);
        if let Some(found) = self.runtime.module_mut(module) {
            found.imports.clear();
            found.exports = None;
        }
        self.loading.push(name.to_owned());
        let result = self.eval_in(module, file, source, host);
        self.loading.pop();
        if result.is_ok() {
            self.broken.remove(name);
        } else {
            self.broken.insert(name.to_owned());
        }
        result.map(|_| module)
    }

    fn header(
        &mut self,
        module: ModuleId,
        clauses: &[Syntax],
        host: &mut dyn Any,
    ) -> Result<(), Error> {
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
                        self.import(module, spec, host)?;
                    }
                }
                Some("export") => {
                    let names: HashSet<Sym> =
                        rest.iter().filter_map(symbol).map(Sym::intern).collect();
                    if let Some(found) = self.runtime.module_mut(module) {
                        found.exports.get_or_insert_with(HashSet::new).extend(names);
                    }
                }
                _ => return Err(malformed()),
            }
        }
        Ok(())
    }

    fn import(&mut self, module: ModuleId, spec: &Syntax, host: &mut dyn Any) -> Result<(), Error> {
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
        let mut exclude = Vec::new();
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
                (Datum::Keyword(word), Datum::Vector(names)) if word == "exclude" => {
                    exclude.extend(names.iter().filter_map(symbol).map(Sym::intern));
                }
                _ => return Err(malformed()),
            }
        }
        let name = segments.join("/");
        self.may_import(module, &name)
            .map_err(|message| Error::at(spec.span, message))?;
        let loaded = self
            .runtime
            .module_ids
            .get(&name)
            .copied()
            .filter(|_| !self.loading.contains(&name));
        let reload = loaded.filter(|_| {
            self.broken.contains(&name)
                || (self.runtime.sandboxed && self.stale(&name, &mut HashSet::new()))
        });
        let id = match loaded {
            Some(id) if reload.is_none() => id,
            _ => {
                if let Some(id) = reload {
                    self.runtime.reset_module(id);
                }
                self.load_module(&name, host)
                    .map_err(|error| error.or_at(spec.span))?
            }
        };
        if let Some(found) = self.runtime.module_mut(module) {
            found
                .imports
                .retain(|import| import.module != id || import.alias != alias);
            found.imports.push(Import {
                module: id,
                alias,
                only,
                exclude,
            });
        }
        Ok(())
    }

    pub(crate) fn define_value_in(&mut self, module: &str, name: &str, value: Value, doc: &str) {
        let place = self.runtime.module_named(module);
        let name = Sym::intern(name);
        self.runtime.define(place, name, value);
        self.runtime
            .document(place, name, Some(doc.to_owned()), None);
    }

    pub(crate) fn define_control(
        &mut self,
        name: &str,
        least: usize,
        most: Option<usize>,
        control: Control,
    ) {
        let native = Native {
            control: Some(control),
            outside: control == Control::Worker,
            ..plain("ox", name, least, most, only_from_ox(name))
        };
        if let Some(value) = self.install("ox", name, native) {
            self.runtime.controls.insert(control, value);
        }
    }

    pub(crate) fn define_expander(
        &mut self,
        name: &str,
        least: usize,
        most: Option<usize>,
        expand: impl Fn(&mut Heap, &[Value]) -> Result<Value, Error> + 'static,
    ) {
        let function: Rc<NativeFn> = Rc::new(move |heap, _, arguments| expand(heap, arguments));
        if let Ok(value) = self
            .runtime
            .heap
            .native(plain("ox", name, least, most, function))
        {
            self.runtime.define_macro(PRELUDE, Sym::intern(name), value);
        }
    }

    pub(crate) fn install(&mut self, module: &str, name: &str, native: Native) -> Option<Value> {
        let place = self.runtime.module_named(module);
        let value = self.runtime.heap.native(native).ok()?;
        self.runtime.define(place, Sym::intern(name), value);
        Some(value)
    }

    pub fn promise(&mut self) -> Result<Value, Error> {
        self.runtime.promise()
    }

    pub fn resolve(&mut self, promise: Value, value: Value) -> bool {
        self.runtime.settle(promise, Promise::Delivered(value))
    }

    pub fn reject(&mut self, promise: Value, error: &Error) -> bool {
        let Ok(error) = self.runtime.error_value(error) else {
            return false;
        };
        self.runtime.settle(promise, Promise::Failed(error))
    }

    pub fn spawn(&mut self, function: Value, arguments: &[Value]) -> Result<TaskId, Error> {
        self.runtime.spawn(function, arguments)
    }

    pub fn run(&mut self, budget: Duration) -> Report {
        self.runtime.run_tasks(budget, &mut ())
    }

    pub fn runnable(&self) -> usize {
        self.runtime.runnable()
    }

    pub fn finish_tasks(&mut self, limit: Option<Duration>) -> Vec<Error> {
        let stop = limit.map(|limit| Instant::now() + limit);
        let mut unhandled = Vec::new();
        while self.pending() > 0 && stop.is_none_or(|stop| Instant::now() < stop) {
            unhandled.extend(self.run(TASK_SLICE).unhandled);
            if self.runnable() > 0 {
                continue;
            }
            let pause = match self.next_wake() {
                Wake::Never => break,
                Wake::Soon => IDLE_POLL,
                Wake::At(when) => when.saturating_duration_since(crate::clock::now()),
            };
            let until = Instant::now() + pause;
            let until = stop.map_or(until, |stop| until.min(stop));
            std::thread::sleep(until.saturating_duration_since(Instant::now()));
        }
        unhandled
    }

    pub fn pending(&self) -> usize {
        self.runtime.pending()
    }

    pub fn next_wake(&self) -> Wake {
        self.runtime.next_wake()
    }

    pub fn set_sink(&mut self, sink: Arc<dyn Sink>) {
        self.runtime.heap.set_sink(sink);
    }

    pub fn wait_for_work(&self, timeout: Duration) -> bool {
        self.runtime.wait_for_work(timeout)
    }

    pub fn run_with(&mut self, host: &mut dyn Any, budget: Duration) -> Report {
        self.runtime.run_tasks(budget, host)
    }

    pub fn cancel(&mut self, task: TaskId) -> bool {
        self.runtime.cancel(task)
    }

    pub fn interrupter(&self) -> Interrupter {
        self.runtime.interrupter.clone()
    }

    fn check_header(
        &mut self,
        module: ModuleId,
        clauses: &[Syntax],
        found: &mut Vec<Diagnostic>,
        unknown: &mut check::Unknown,
        imports: &mut Vec<check::Import>,
    ) -> Result<(), Span> {
        for clause in clauses {
            let specs = match &clause.datum {
                Datum::List(items) if items.first().and_then(symbol) == Some("import") => {
                    &items[1..]
                }
                _ => {
                    if let Err(error) = self.header(module, std::slice::from_ref(clause), &mut ()) {
                        found.push(Diagnostic::error(
                            "import",
                            error.message,
                            error.span.unwrap_or(clause.span),
                        ));
                    }
                    continue;
                }
            };
            for spec in specs {
                match self.import(module, spec, &mut ()) {
                    Ok(()) => imports.extend(check::Import::of(spec)),
                    Err(error) if error.kind == ErrorKind::Interrupt => return Err(spec.span),
                    Err(error) => {
                        if error.kind != ErrorKind::Refused {
                            found.push(Diagnostic::error(
                                "import",
                                error.message,
                                error.span.unwrap_or(spec.span),
                            ));
                        }
                        unknown.add(spec);
                    }
                }
            }
        }
        Ok(())
    }

    pub fn check(&mut self, name: &str, source: &str) -> Vec<Diagnostic> {
        self.check_within(check::BUDGET, name, source)
    }

    pub fn check_within(&mut self, budget: Duration, name: &str, source: &str) -> Vec<Diagnostic> {
        self.examine_within(budget, name, source).0
    }

    pub fn bindings(&mut self, name: &str, source: &str) -> Vec<Occurrence> {
        self.examine_within(check::BUDGET, name, source).1
    }

    pub fn check_with_bindings(
        &mut self,
        name: &str,
        source: &str,
    ) -> (Vec<Diagnostic>, Vec<Occurrence>) {
        self.examine_within(check::BUDGET, name, source)
    }

    fn examine_within(
        &mut self,
        budget: Duration,
        name: &str,
        source: &str,
    ) -> (Vec<Diagnostic>, Vec<Occurrence>) {
        let sandboxed = std::mem::replace(&mut self.runtime.sandboxed, true);
        let found = self.within(budget, |isolate| Ok(isolate.examine(name, source)));
        self.runtime.sandboxed = sandboxed;
        found.unwrap_or_default()
    }

    fn stopped(&self, mut found: Vec<Diagnostic>, span: Span) -> Vec<Diagnostic> {
        let budget = self
            .runtime
            .budget
            .map_or(0, |(_, budget)| budget.as_millis());
        found.push(Diagnostic::warning(
            "budget",
            format!("Checking stopped here after {budget} ms"),
            span,
        ));
        found.sort_by_key(|diagnostic| (diagnostic.span.start, diagnostic.span.end));
        found
    }

    fn examine(&mut self, name: &str, source: &str) -> (Vec<Diagnostic>, Vec<Occurrence>) {
        let module_name = format!("#check:{name}");
        let module = self.runtime.module_named(&module_name);
        self.runtime.reset_module(module);
        let text = Rc::new(Source {
            name: name.to_owned(),
            text: source.to_owned(),
        });
        let (forms, errors) = read::read(source);
        let mut found: Vec<Diagnostic> = errors
            .into_iter()
            .map(|error| Diagnostic::error("syntax", error.message, error.span))
            .collect();
        let broken: Vec<Span> = found.iter().map(|diagnostic| diagnostic.span).collect();
        let mut unknown = check::Unknown::default();
        let mut imports = Vec::new();
        let mut expanded = Vec::new();
        for form in &forms {
            if broken
                .iter()
                .any(|span| form.span.start <= span.start && span.start <= form.span.end)
            {
                continue;
            }
            if let Some(clauses) = header(form) {
                if let Err(span) =
                    self.check_header(module, clauses, &mut found, &mut unknown, &mut imports)
                {
                    return (self.stopped(found, span), Vec::new());
                }
                continue;
            }
            let syntax = match expand(&mut self.runtime, form, module) {
                Ok(syntax) => syntax,
                Err(error) if error.kind == ErrorKind::Interrupt => {
                    return (self.stopped(found, form.span), Vec::new());
                }
                Err(error) => {
                    if error.kind != ErrorKind::Refused {
                        found.push(Diagnostic::error(
                            "macro",
                            error.message,
                            error.span.unwrap_or(form.span),
                        ));
                    }
                    continue;
                }
            };
            let code = match compile(
                &mut self.runtime.heap,
                &syntax,
                Some(Rc::clone(&text)),
                module,
                Sym::intern(&module_name),
            ) {
                Ok(code) => code,
                Err(error) => {
                    found.push(Diagnostic::error(
                        "malformed",
                        error.message,
                        error.span.unwrap_or(form.span),
                    ));
                    continue;
                }
            };
            if check::defines_macro(&syntax)
                && let Err(error) = self.runtime.run(&Rc::new(code), &mut ())
            {
                if error.kind == ErrorKind::Interrupt {
                    return (self.stopped(found, form.span), Vec::new());
                }
                found.push(Diagnostic::error(
                    "macro",
                    error.message,
                    error.span.unwrap_or(form.span),
                ));
            }
            expanded.push(syntax);
        }
        let (named, occurrences) = check::names(&self.runtime, module, source, &expanded, &unknown);
        found.extend(named);
        let body: Vec<&Syntax> = forms.iter().filter(|form| header(form).is_none()).collect();
        found.extend(check::unused_imports(&imports, &body));
        found.extend(check::strings(source, &imports, &body));
        found.sort_by_key(|diagnostic| (diagnostic.span.start, diagnostic.span.end));
        found.dedup();
        (found, occurrences)
    }

    pub fn spawn_eval(&mut self, name: &str, source: &str) -> Result<Option<TaskId>, Error> {
        self.spawn_eval_with(&mut (), name, source)
    }

    pub fn spawn_eval_with(
        &mut self,
        host: &mut dyn Any,
        name: &str,
        source: &str,
    ) -> Result<Option<TaskId>, Error> {
        self.spawn_eval_in(host, "user", name, source)
    }

    pub fn spawn_eval_in(
        &mut self,
        host: &mut dyn Any,
        module: &str,
        name: &str,
        source: &str,
    ) -> Result<Option<TaskId>, Error> {
        let id = self.module_id(module);
        let module = Sym::intern(if id == USER { "user" } else { module });
        let text = Rc::new(Source {
            name: name.to_owned(),
            text: source.to_owned(),
        });
        let (forms, errors) = read::read(source);
        if let Some(error) = errors.into_iter().next() {
            return Err(Error::at(error.span, error.message));
        }
        let mut codes = Vec::new();
        for form in &forms {
            if let Some(clauses) = header(form) {
                self.header(id, clauses, host)?;
                continue;
            }
            let expanded = expand(&mut self.runtime, form, id)?;
            codes.push(compile(
                &mut self.runtime.heap,
                &expanded,
                Some(Rc::clone(&text)),
                id,
                module,
            )?);
        }
        let mut codes = codes.into_iter();
        let Some(code) = codes.next() else {
            return Ok(None);
        };
        if codes.next().is_some() {
            return Err(Error::new("spawn_eval evaluates one form at a time"));
        }
        let function = self.runtime.heap.closure(Closure {
            code: Rc::new(code),
            captured: Box::new([]),
        })?;
        self.runtime.spawn(function, &[]).map(Some)
    }

    pub fn module_names(&self) -> Vec<String> {
        self.runtime
            .modules
            .iter()
            .map(|module| module.name.clone())
            .collect()
    }

    pub fn module_count(&self) -> usize {
        self.runtime.modules.len()
    }

    pub fn add_module(&mut self, name: &str, summary: &str) {
        self.runtime.module_named(name);
        match self.summaries.iter_mut().find(|(known, _)| known == name) {
            Some(entry) => summary.clone_into(&mut entry.1),
            None => self.summaries.push((name.to_owned(), summary.to_owned())),
        }
    }

    pub fn modules(&self) -> Vec<(String, String)> {
        self.summaries.clone()
    }

    pub fn doc(&self, name: &str) -> Option<String> {
        self.runtime.doc(USER, Sym::intern(name)).map(str::to_owned)
    }

    pub fn add_native_docs(&mut self, table: &'static str) {
        if self.doc_tables.contains(&table) {
            return;
        }
        self.doc_tables.push(table);
        self.docs = OnceCell::new();
    }

    fn module_for_alias(&self, alias: &str) -> String {
        self.summaries
            .iter()
            .map(|(name, _)| name)
            .find(|name| name.rsplit('/').next() == Some(alias))
            .cloned()
            .unwrap_or_else(|| format!("ox/{alias}"))
    }

    pub fn define_stubs(&mut self, table: &'static str, which: Stubs) {
        let docs = describe::parse(&[table]);
        for (shown, entry) in docs.entries.iter().filter(|(_, entry)| !entry.special) {
            let (module, name) = match shown.split_once('/') {
                Some(_) if which == Stubs::Globals => continue,
                Some((alias, name)) => (self.module_for_alias(alias), name),
                None if which == Stubs::Modules => continue,
                None => ("ox".to_owned(), shown.as_str()),
            };
            let keywords: Vec<&str> = entry.keywords.iter().map(String::as_str).collect();
            let missing = format!("{shown} isn't available here");
            self.native(&module, name)
                .arity(entry.least, entry.positional)
                .keywords(&keywords)
                .function(move |_, _| Err(Error::new(missing.clone()).of(ErrorKind::Refused)));
        }
        self.add_native_docs(table);
    }

    fn docs(&self) -> &Docs {
        self.docs.get_or_init(|| describe::parse(&self.doc_tables))
    }

    fn module_id(&self, name: &str) -> ModuleId {
        self.runtime.module_ids.get(name).copied().unwrap_or(USER)
    }

    pub fn file_of(&self, function: Value) -> Option<&str> {
        let closure = self.runtime.heap.closure_of(function)?;
        closure
            .code
            .source
            .as_deref()
            .map(|source| source.name.as_str())
    }

    pub fn describe(&self, module: &str, name: &str) -> Option<Description> {
        let module = self.module_id(module);
        let symbol = Sym::intern(name);
        for macros in [false, true] {
            if let Some((value, owner, short)) = self.runtime.resolve(module, symbol, macros) {
                return Some(self.described(name, value, owner, short, macros));
            }
        }
        let entry = self
            .docs()
            .entries
            .get(name)
            .filter(|entry| entry.special)?;
        Some(Description {
            name: name.to_owned(),
            kind: Kind::Special,
            signature: describe::signature(name, &entry.parameters),
            doc: Some(entry.doc.clone()),
            place: None,
        })
    }

    pub fn reference(&self) -> Vec<ModuleDocs> {
        let library = |name: &str| name == "ox" || name.starts_with("ox/");
        let mut found: Vec<ModuleDocs> = self
            .runtime
            .modules
            .iter()
            .filter(|module| library(&module.name))
            .map(|module| {
                let exported = |name: &&Sym| {
                    !name.name().starts_with('#')
                        && module
                            .exports
                            .as_ref()
                            .is_none_or(|exports| exports.contains(*name))
                };
                let mut entries: Vec<Description> = module
                    .globals
                    .keys()
                    .chain(module.macros.keys())
                    .filter(exported)
                    .filter_map(|name| self.describe(&module.name, name.name()))
                    .collect();
                if module.name == "ox" {
                    entries.extend(
                        self.docs()
                            .entries
                            .iter()
                            .filter(|(_, entry)| entry.special)
                            .map(|(name, entry)| Description {
                                name: name.clone(),
                                kind: Kind::Special,
                                signature: describe::signature(name, &entry.parameters),
                                doc: Some(entry.doc.clone()),
                                place: None,
                            }),
                    );
                }
                entries.sort_by(|a, b| a.name.cmp(&b.name));
                entries.dedup_by(|a, b| a.name == b.name);
                ModuleDocs {
                    module: module.name.clone(),
                    entries,
                }
            })
            .collect();
        found.sort_by(|a, b| (a.module != "ox", &a.module).cmp(&(b.module != "ox", &b.module)));
        found
    }

    pub fn docs_value(&mut self, docs: &[ModuleDocs]) -> Result<Value, Error> {
        let key = |name: &str| Portable::Keyword(Sym::intern(name));
        let text = |text: &str| Portable::String(text.into());
        let modules = docs
            .iter()
            .map(|docs| {
                let entries = docs
                    .entries
                    .iter()
                    .map(|entry| {
                        Portable::Map(vec![
                            (key("name"), text(&entry.name)),
                            (key("kind"), key(entry.kind.name())),
                            (key("signature"), text(&entry.signature)),
                            (key("doc"), entry.doc.as_deref().map_or(Portable::Nil, text)),
                        ])
                    })
                    .collect();
                Portable::Map(vec![
                    (key("module"), text(&docs.module)),
                    (key("entries"), Portable::Vector(entries)),
                ])
            })
            .collect();
        self.runtime.rebuild(&Portable::Vector(modules))
    }

    fn described(
        &self,
        name: &str,
        value: Value,
        owner: ModuleId,
        short: Sym,
        macros: bool,
    ) -> Description {
        let heap = &self.runtime.heap;
        let parameters_of = |value: Value| {
            heap.closure_of(value)
                .and_then(|closure| closure.code.parameters.clone())
                .or_else(|| heap.native_of(value)?.parameters().cloned())
        };
        let mut doc = self.runtime.doc(owner, short).map(str::to_owned);
        let built_in = heap
            .native_of(value)
            .filter(|native| native.parameters().is_none());
        let (kind, parameters) = if let Some(native) = built_in {
            let entry = self.docs().entries.get(native.name.name());
            doc = entry.map(|entry| entry.doc.clone());
            let parameters = entry.map_or_else(
                || describe::fallback_with(native.least, native.most, &native.keywords),
                |entry| entry.parameters.clone(),
            );
            let kind = if macros { Kind::Macro } else { Kind::Native };
            (kind, Some(parameters))
        } else if let Some(record) = heap.record_type_of(value) {
            let parameters = parameters_of(record.constructor).map(|shown| shown.to_string());
            (Kind::Record, parameters)
        } else if let Some(parameters) = parameters_of(value) {
            let kind = if macros { Kind::Macro } else { Kind::Function };
            (kind, Some(parameters.to_string()))
        } else {
            (Kind::Value, None)
        };
        Description {
            name: name.to_owned(),
            kind,
            signature: parameters.map_or_else(
                || name.to_owned(),
                |parameters| describe::signature(name, &parameters),
            ),
            doc,
            place: self.runtime.place(owner, short).cloned(),
        }
    }

    pub fn completions(&self, module: &str, prefix: &str) -> Vec<String> {
        let id = self.module_id(module);
        let shown = |name: &&Sym| !name.name().starts_with('#');
        let visible = |module: &Module| -> Vec<String> {
            let names = module.globals.keys().chain(module.macros.keys());
            names
                .filter(shown)
                .map(|name| name.name().to_owned())
                .collect()
        };
        let mut found: Vec<String> = Vec::new();
        if id != PRELUDE
            && let Some(prelude) = self.runtime.modules.get(PRELUDE)
        {
            found.extend(visible(prelude));
        }
        if let Some(current) = self.runtime.modules.get(id) {
            found.extend(visible(current));
            for import in &current.imports {
                let Some(target) = self.runtime.modules.get(import.module) else {
                    continue;
                };
                let exported = |name: &&Sym| {
                    shown(name)
                        && target
                            .exports
                            .as_ref()
                            .is_none_or(|exports| exports.contains(*name))
                };
                let names = target.globals.keys().chain(target.macros.keys());
                found.extend(
                    names
                        .filter(exported)
                        .map(|name| format!("{}/{}", import.alias.name(), name.name())),
                );
                found.extend(import.only.iter().map(|name| name.name().to_owned()));
            }
        }
        found.extend(
            self.docs()
                .entries
                .iter()
                .filter(|(_, entry)| entry.special)
                .map(|(name, _)| name.clone()),
        );
        found.retain(|name| name.starts_with(prefix));
        found.sort();
        found.dedup();
        found
    }

    pub fn forms(&self, module: &str) -> Forms {
        let id = self.module_id(module);
        let modules = &self.runtime.modules;
        let form_of =
            |found: &Module, name: &Sym| found.forms.get(name).copied().unwrap_or_default();
        let mut forms = Forms::special();
        let current = modules.get(id);
        if let Some(prelude) = modules.get(PRELUDE) {
            let excluded = |name: &Sym| {
                current.is_some_and(|current| {
                    current
                        .imports
                        .iter()
                        .any(|import| import.module == PRELUDE && import.exclude.contains(name))
                })
            };
            for name in prelude.macros.keys().filter(|name| !excluded(name)) {
                forms.insert(name.name(), form_of(prelude, name));
            }
        }
        let Some(current) = current.filter(|_| id != PRELUDE) else {
            return forms;
        };
        for name in current.macros.keys() {
            forms.insert(name.name(), form_of(current, name));
        }
        for import in &current.imports {
            let Some(target) = modules
                .get(import.module)
                .filter(|_| import.module != PRELUDE)
            else {
                continue;
            };
            for name in target.macros.keys().filter(|name| target.exports(**name)) {
                let form = form_of(target, name);
                forms.insert(format!("{}/{}", import.alias.name(), name.name()), form);
                if import.only.contains(name) {
                    forms.insert(name.name(), form);
                }
            }
        }
        forms
    }

    pub fn checked_forms(&self, name: &str, source: &str) -> Forms {
        self.forms(&format!("#check:{name}")).scanned(source)
    }

    pub fn forms_for(&mut self, name: &str, source: &str) -> Forms {
        self.check(name, source);
        self.checked_forms(name, source)
    }

    pub fn examples(&self, name: &str) -> Vec<String> {
        let heap = &self.runtime.heap;
        let Some(tests) = self
            .runtime
            .lookup(PRELUDE, Sym::intern("registered-tests"))
            .and_then(|atom| heap.atom_get(atom))
            .and_then(|tests| heap.elements(tests))
        else {
            return Vec::new();
        };
        tests
            .into_iter()
            .filter_map(|test| {
                let map = heap.map_of(test)?;
                let title = heap.text(map.get(heap, Value::keyword("name"))?)?;
                let named = title == name
                    || title
                        .strip_prefix(name)
                        .is_some_and(|rest| rest.starts_with(' '));
                if !named {
                    return None;
                }
                let closure = heap.closure_of(map.get(heap, Value::keyword("run"))?)?;
                let source = closure.code.source.as_ref()?;
                let at = closure
                    .code
                    .spans
                    .iter()
                    .filter(|span| span.end > span.start)
                    .map(|span| span.start)
                    .min()?;
                let (forms, _) = read::read(&source.text);
                let form = forms
                    .iter()
                    .find(|form| form.span.start <= at && at < form.span.end)?;
                source
                    .text
                    .get(form.span.start as usize..form.span.end as usize)
                    .map(str::to_owned)
            })
            .collect()
    }

    pub fn natives(&self) -> Vec<(String, String)> {
        let heap = &self.runtime.heap;
        let mut found: Vec<(String, String)> = self
            .runtime
            .modules
            .iter()
            .flat_map(|module| &module.globals)
            .filter_map(|(_, value)| heap.native_of(*value))
            .filter(|native| native.parameters().is_none())
            .map(|native| {
                let mut keywords = native.keywords.to_vec();
                keywords.sort_by_key(|keyword| keyword.name());
                (
                    native.name.name().to_owned(),
                    describe::fallback_with(native.least, native.most, &keywords),
                )
            })
            .collect();
        found.sort();
        found.dedup();
        found
    }

    pub fn undocumented(&self) -> Vec<String> {
        let docs = self.docs();
        let heap = &self.runtime.heap;
        let mut problems = docs.broken.clone();
        let mut natives = HashSet::new();
        let mut check = |native: &Native, problems: &mut Vec<String>| {
            let shown = native.name.name();
            natives.insert(shown);
            match docs.entries.get(shown) {
                None => problems.push(format!("{shown} has no doc entry")),
                Some(entry) if !entry.fits(native.least, native.most, &native.keywords) => {
                    problems.push(format!(
                        "{shown} takes {} but its doc entry shows {}",
                        describe::fallback_with(native.least, native.most, &native.keywords),
                        entry.parameters
                    ));
                }
                Some(_) => {}
            }
        };
        let built_in = |value: Value| {
            heap.native_of(value)
                .filter(|native| native.parameters().is_none())
        };
        for module in &self.runtime.modules {
            let library = module.name == "ox" || module.name.starts_with("ox/");
            for (name, value) in &module.globals {
                if name.name().starts_with('#') {
                    continue;
                }
                if let Some(native) = built_in(*value) {
                    check(native, &mut problems);
                } else if library
                    && (heap.closure_of(*value).is_some() || heap.native_of(*value).is_some())
                    && !module.docs.contains_key(name)
                {
                    problems.push(format!("{} has no docstring", name.name()));
                }
            }
            if library {
                for (name, value) in &module.macros {
                    if let Some(native) = built_in(*value) {
                        check(native, &mut problems);
                    } else if !module.docs.contains_key(name) {
                        problems.push(format!("the macro {} has no docstring", name.name()));
                    }
                }
            }
        }
        for (name, entry) in &docs.entries {
            if !entry.special && !natives.contains(name.as_str()) {
                problems.push(format!("{name} has a doc entry but is not a native"));
            }
        }
        problems.sort();
        problems
    }

    pub fn profiling(&self) -> bool {
        self.runtime.profiler.is_some()
    }

    pub fn start_profiling(&mut self) -> bool {
        self.runtime.start_profiling()
    }

    pub fn stop_profiling(&mut self) -> Option<Profile> {
        self.runtime.stop_profiling()
    }

    pub fn record_tick(&mut self, kind: &str, took: Duration) {
        if let Some(profiler) = &mut self.runtime.profiler {
            profiler.tick(kind, took);
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
    fn each_global_site_looks_its_name_up_once_while_globals_stay_put() {
        let mut isolate = Isolate::new();
        isolate
            .eval("(def (count-down n) (if (< n 1) :done (count-down (- n 1))))")
            .unwrap();
        let before = isolate.runtime.global_lookups;

        assert_eq!(isolate.eval("(count-down 100)"), Ok(Value::keyword("done")));
        assert!(isolate.runtime.global_lookups - before < 10);
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
        isolate
            .native("ox", "twice")
            .arity(1, Some(1))
            .function(|_, arguments| match arguments {
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
