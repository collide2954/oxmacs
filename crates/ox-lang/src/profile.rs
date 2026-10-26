use std::cmp::Reverse;
use std::collections::HashMap;
use std::fmt::Write;
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::bytecode::Code;
use crate::error::Error;
use crate::heap::Heap;
use crate::span::line_column;
use crate::symbol::Sym;
use crate::value::Value;
use crate::vm::machine::Frame;
use crate::vm::modules::{self, Module};

const LIBRARY: &str = "ox";
const SHOWN_SHARE: f64 = 0.5;
const FLAT_ROWS: usize = 40;
const KEPT_TICKS: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Plugin,
    Function,
    Native,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    pub file: String,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub struct Branch {
    pub label: String,
    pub kind: Kind,
    pub total: Duration,
    pub own: Duration,
    pub calls: u64,
    pub place: Option<Place>,
    pub children: Vec<Branch>,
}

#[derive(Debug, Clone)]
pub struct FunctionStats {
    pub label: String,
    pub module: String,
    pub kind: Kind,
    pub total: Duration,
    pub own: Duration,
    pub calls: u64,
    pub place: Option<Place>,
}

#[derive(Debug, Clone)]
pub struct Tick {
    pub kind: String,
    pub took: Duration,
    pub at: Duration,
}

#[derive(Debug, Clone, Default)]
pub struct Profile {
    pub wall: Duration,
    pub busy: Duration,
    pub samples: u64,
    pub tree: Vec<Branch>,
    pub functions: Vec<FunctionStats>,
    pub ticks: Vec<Tick>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Entry {
    Group(usize),
    Function(usize),
    Native(Sym),
}

#[derive(Debug)]
struct Node {
    entry: Option<Entry>,
    own: Duration,
    calls: u64,
    children: Vec<usize>,
    natives: Vec<(Sym, usize)>,
}

#[derive(Debug, Clone)]
struct Stats {
    label: String,
    module: String,
    place: Option<Place>,
    calls: u64,
}

#[derive(Debug, Clone, Copy, Default)]
struct Sums {
    total: Duration,
    own: Duration,
}

#[derive(Debug)]
pub(crate) struct Profiler {
    started: Instant,
    last: Option<Instant>,
    native_time: Duration,
    stacks: HashMap<u64, usize>,
    last_stack: Option<(u64, usize)>,
    busy: Duration,
    samples: u64,
    codes: Vec<Rc<Code>>,
    ids: HashMap<*const Code, usize>,
    functions: Vec<Stats>,
    natives: Vec<Sym>,
    groups: Vec<String>,
    group_ids: HashMap<String, usize>,
    nodes: Vec<Node>,
    index: HashMap<(usize, Entry), usize>,
    ticks: Vec<Tick>,
}

fn plugin_of(module: &str) -> &str {
    module.split('/').next().unwrap_or(module)
}

fn module_name(modules: &[Module], code: &Code) -> String {
    modules
        .get(code.module)
        .map_or_else(|| LIBRARY.to_owned(), |module| module.name.clone())
}

fn place_of(code: &Code) -> Option<Place> {
    let source = code.source.as_ref()?;
    let span = code.spans.first()?;
    Some(Place {
        file: source.name.clone(),
        line: line_column(&source.text, span.start).0,
    })
}

fn stack_key<'a>(codes: impl Iterator<Item = &'a Rc<Code>>) -> u64 {
    codes.fold(0xcbf2_9ce4_8422_2325_u64, |hash, code| {
        let address = u64::try_from(Rc::as_ptr(code).addr()).unwrap_or_default();
        (hash ^ address).wrapping_mul(0x0100_0000_01b3)
    })
}

impl Profiler {
    pub(crate) fn new() -> Self {
        let now = Instant::now();
        Self {
            started: now,
            last: Some(now),
            native_time: Duration::ZERO,
            stacks: HashMap::new(),
            last_stack: None,
            busy: Duration::ZERO,
            samples: 0,
            codes: Vec::new(),
            ids: HashMap::new(),
            functions: Vec::new(),
            natives: Vec::new(),
            groups: Vec::new(),
            group_ids: HashMap::new(),
            nodes: vec![Node {
                entry: None,
                own: Duration::ZERO,
                calls: 0,
                children: Vec::new(),
                natives: Vec::new(),
            }],
            index: HashMap::new(),
            ticks: Vec::new(),
        }
    }

    pub(crate) fn resume(&mut self) {
        self.last = Some(Instant::now());
        self.native_time = Duration::ZERO;
    }

    fn function(&mut self, code: &Rc<Code>, modules: &[Module]) -> usize {
        let pointer = Rc::as_ptr(code);
        if let Some(id) = self.ids.get(&pointer) {
            return *id;
        }
        let label = match code.name {
            Some(name) => modules::display(name).to_owned(),
            None => "fn".to_owned(),
        };
        let id = self.functions.len();
        self.functions.push(Stats {
            label,
            module: module_name(modules, code),
            place: place_of(code),
            calls: 0,
        });
        self.codes.push(Rc::clone(code));
        self.ids.insert(pointer, id);
        id
    }

    fn group(&mut self, stack: &[&Rc<Code>], modules: &[Module]) -> usize {
        let plugin = stack
            .iter()
            .filter(|code| code.source.is_some())
            .map(|code| module_name(modules, code))
            .find(|module| plugin_of(module) != LIBRARY)
            .map_or_else(
                || LIBRARY.to_owned(),
                |module| plugin_of(&module).to_owned(),
            );
        if let Some(id) = self.group_ids.get(&plugin) {
            return *id;
        }
        let id = self.groups.len();
        self.groups.push(plugin.clone());
        self.group_ids.insert(plugin, id);
        id
    }

    fn child(&mut self, parent: usize, entry: Entry) -> usize {
        if let Some(found) = self.index.get(&(parent, entry)) {
            return *found;
        }
        let id = self.nodes.len();
        self.nodes.push(Node {
            entry: Some(entry),
            own: Duration::ZERO,
            calls: 0,
            children: Vec::new(),
            natives: Vec::new(),
        });
        self.nodes[parent].children.push(id);
        self.index.insert((parent, entry), id);
        id
    }

    fn native_child(&mut self, parent: usize, name: Sym) -> usize {
        if let Some((_, found)) = self.nodes[parent]
            .natives
            .iter()
            .find(|(known, _)| *known == name)
        {
            return *found;
        }
        if !self.natives.contains(&name) {
            self.natives.push(name);
        }
        let id = self.child(parent, Entry::Native(name));
        self.nodes[parent].natives.push((name, id));
        id
    }

    fn node(&mut self, stack: &[&Rc<Code>], modules: &[Module]) -> usize {
        let key = stack_key(stack.iter().copied());
        self.keyed(key, stack, modules)
    }

    fn keyed(&mut self, key: u64, stack: &[&Rc<Code>], modules: &[Module]) -> usize {
        if let Some((last, node)) = self.last_stack
            && last == key
        {
            return node;
        }
        if let Some(node) = self.stacks.get(&key).copied() {
            self.last_stack = Some((key, node));
            return node;
        }
        let group = self.group(stack, modules);
        let mut node = self.child(0, Entry::Group(group));
        let frames = stack.iter().filter(|code| code.source.is_some());
        for (depth, code) in frames.enumerate() {
            if depth == 0 && code.name.is_none() {
                continue;
            }
            let id = self.function(code, modules);
            node = self.child(node, Entry::Function(id));
        }
        self.stacks.insert(key, node);
        self.last_stack = Some((key, node));
        node
    }

    pub(crate) fn called(&mut self, code: &Rc<Code>, modules: &[Module]) {
        let id = self.function(code, modules);
        self.functions[id].calls += 1;
    }

    pub(crate) fn native(
        &mut self,
        name: Sym,
        took: Duration,
        frames: &[Frame],
        modules: &[Module],
    ) {
        self.native_time += took;
        let key = stack_key(frames.iter().map(|frame| &frame.code));
        let node = if let Some((last, node)) = self.last_stack
            && last == key
        {
            node
        } else {
            let stack: Vec<&Rc<Code>> = frames.iter().map(|frame| &frame.code).collect();
            self.keyed(key, &stack, modules)
        };
        let leaf = self.native_child(node, name);
        self.nodes[leaf].own += took;
        self.nodes[leaf].calls += 1;
        self.busy += took;
    }

    pub(crate) fn tick(&mut self, kind: &str, took: Duration) {
        self.ticks.push(Tick {
            kind: kind.to_owned(),
            took,
            at: self.started.elapsed(),
        });
        if self.ticks.len() > KEPT_TICKS * 4 {
            self.ticks.sort_by_key(|tick| Reverse(tick.took));
            self.ticks.truncate(KEPT_TICKS);
        }
    }

    pub(crate) fn sample(&mut self, stack: &[&Rc<Code>], modules: &[Module]) {
        let now = Instant::now();
        let Some(last) = self.last.replace(now) else {
            return;
        };
        let weight = now
            .saturating_duration_since(last)
            .saturating_sub(self.native_time);
        self.native_time = Duration::ZERO;
        self.samples += 1;
        if weight.is_zero() {
            return;
        }
        let node = self.node(stack, modules);
        self.nodes[node].own += weight;
        self.busy += weight;
    }

    pub(crate) fn pause(&mut self, stack: &[&Rc<Code>], modules: &[Module]) {
        self.sample(stack, modules);
        self.last = None;
    }

    fn totals(&self) -> Vec<Duration> {
        let mut totals: Vec<Duration> = self.nodes.iter().map(|node| node.own).collect();
        for (id, node) in self.nodes.iter().enumerate().rev() {
            let below: Duration = node.children.iter().map(|child| totals[*child]).sum();
            totals[id] += below;
        }
        totals
    }

    fn sums(&self) -> HashMap<Entry, Sums> {
        let mut sums: HashMap<Entry, Sums> = HashMap::new();
        let mut active: HashMap<Entry, usize> = HashMap::new();
        let mut walk = vec![(0, false)];
        while let Some((id, leaving)) = walk.pop() {
            let node = &self.nodes[id];
            if leaving {
                if let Some(entry) = node.entry
                    && let Some(count) = active.get_mut(&entry)
                {
                    *count -= 1;
                }
                continue;
            }
            if let Some(entry) = node.entry {
                *active.entry(entry).or_default() += 1;
                sums.entry(entry).or_default().own += node.own;
            }
            if !node.own.is_zero() {
                for (entry, count) in &active {
                    if *count > 0 {
                        sums.entry(*entry).or_default().total += node.own;
                    }
                }
            }
            walk.push((id, true));
            walk.extend(node.children.iter().map(|child| (*child, false)));
        }
        sums
    }

    fn branch(&self, node: usize, totals: &[Duration]) -> Option<Branch> {
        let found = self.nodes.get(node)?;
        let (label, kind, calls, place) = match found.entry? {
            Entry::Group(id) => (self.groups.get(id)?.clone(), Kind::Plugin, 0, None),
            Entry::Function(id) => {
                let stats = self.functions.get(id)?;
                (
                    stats.label.clone(),
                    Kind::Function,
                    stats.calls,
                    stats.place.clone(),
                )
            }
            Entry::Native(name) => (name.name().to_owned(), Kind::Native, found.calls, None),
        };
        Some(Branch {
            label,
            kind,
            total: totals[node],
            own: found.own,
            calls,
            place,
            children: self.branches(&found.children, totals),
        })
    }

    fn branches(&self, nodes: &[usize], totals: &[Duration]) -> Vec<Branch> {
        let mut found: Vec<Branch> = nodes
            .iter()
            .filter_map(|node| self.branch(*node, totals))
            .collect();
        found.sort_by(|a, b| b.total.cmp(&a.total).then_with(|| a.label.cmp(&b.label)));
        found
    }

    pub(crate) fn profile(&self) -> Profile {
        let totals = self.totals();
        let sums = self.sums();
        let measured = |entry: Entry| sums.get(&entry).copied().unwrap_or_default();
        let mut functions: Vec<FunctionStats> = self
            .functions
            .iter()
            .enumerate()
            .map(|(id, stats)| {
                let sums = measured(Entry::Function(id));
                FunctionStats {
                    label: stats.label.clone(),
                    module: stats.module.clone(),
                    kind: Kind::Function,
                    total: sums.total,
                    own: sums.own,
                    calls: stats.calls,
                    place: stats.place.clone(),
                }
            })
            .chain(self.natives.iter().map(|name| {
                let sums = measured(Entry::Native(*name));
                let calls = self
                    .nodes
                    .iter()
                    .filter(|node| node.entry == Some(Entry::Native(*name)))
                    .map(|node| node.calls)
                    .sum();
                FunctionStats {
                    label: name.name().to_owned(),
                    module: LIBRARY.to_owned(),
                    kind: Kind::Native,
                    total: sums.total,
                    own: sums.own,
                    calls,
                    place: None,
                }
            }))
            .collect();
        functions.sort_by(|a, b| {
            b.own
                .cmp(&a.own)
                .then_with(|| b.total.cmp(&a.total))
                .then_with(|| b.calls.cmp(&a.calls))
                .then_with(|| a.label.cmp(&b.label))
        });
        let mut ticks = self.ticks.clone();
        ticks.sort_by_key(|tick| Reverse(tick.took));
        Profile {
            wall: self.started.elapsed(),
            busy: self.busy,
            samples: self.samples,
            tree: self.branches(&self.nodes[0].children, &totals),
            functions,
            ticks,
        }
    }
}
fn milliseconds(time: Duration) -> f64 {
    time.as_secs_f64() * 1000.0
}

fn shown(time: Duration) -> String {
    if time >= Duration::from_secs(1) {
        format!("{:.2} s", time.as_secs_f64())
    } else {
        format!("{:.2} ms", milliseconds(time))
    }
}

fn share(part: Duration, whole: Duration) -> f64 {
    if whole.is_zero() {
        0.0
    } else {
        part.as_secs_f64() * 100.0 / whole.as_secs_f64()
    }
}

fn placed(place: Option<&Place>) -> String {
    place.map_or_else(String::new, |place| {
        let file = place.file.rsplit(['/', '\\']).next().unwrap_or(&place.file);
        format!("  {file}:{}", place.line)
    })
}

fn row(out: &mut String, depth: usize, label: &str, columns: [String; 3], place: &str) {
    let indent = "  ".repeat(depth + 1);
    let width = 40_usize.saturating_sub(indent.len()).max(label.len());
    let [first, second, calls] = columns;
    let _ = writeln!(
        out,
        "{indent}{label:<width$}{first:>9}{second:>9}{calls:>9}{place}"
    );
}

fn percent(value: f64) -> String {
    format!("{value:.1}%")
}

impl Profile {
    pub fn function(&self, label: &str) -> Option<&FunctionStats> {
        self.functions.iter().find(|stats| stats.label == label)
    }

    pub fn branch(&self, path: &[&str]) -> Option<&Branch> {
        let (first, rest) = path.split_first()?;
        let mut found = self.tree.iter().find(|branch| branch.label == *first)?;
        for label in rest {
            found = found
                .children
                .iter()
                .find(|branch| branch.label == *label)?;
        }
        Some(found)
    }

    fn tree_rows(&self, out: &mut String, branches: &[Branch], depth: usize) {
        for branch in branches {
            if share(branch.total, self.busy) < SHOWN_SHARE {
                continue;
            }
            let calls = if branch.kind == Kind::Plugin {
                String::new()
            } else {
                branch.calls.to_string()
            };
            row(
                out,
                depth,
                &branch.label,
                [
                    percent(share(branch.total, self.busy)),
                    percent(share(branch.own, self.busy)),
                    calls,
                ],
                &placed(branch.place.as_ref()),
            );
            self.tree_rows(out, &branch.children, depth + 1);
        }
    }

    pub fn report(&self) -> String {
        let mut out = format!(
            "Profile: {} of Ox in {}, {} samples\n",
            shown(self.busy),
            shown(self.wall),
            self.samples
        );
        if !self.ticks.is_empty() {
            out.push_str("\nSlowest ticks\n");
            for tick in self.ticks.iter().take(10) {
                let _ = writeln!(out, "  {}  {}", shown(tick.took), tick.kind);
            }
        }
        let _ = writeln!(
            out,
            "\n{:<40}{:>9}{:>9}{:>9}",
            "Call tree", "total", "self", "calls"
        );
        self.tree_rows(&mut out, &self.tree, 0);
        let _ = writeln!(
            out,
            "\n{:<40}{:>9}{:>9}{:>9}",
            "Functions by self time", "self", "total", "calls"
        );
        for stats in self.functions.iter().take(FLAT_ROWS) {
            if stats.own.is_zero() && stats.total.is_zero() {
                continue;
            }
            row(
                &mut out,
                0,
                &stats.label,
                [
                    percent(share(stats.own, self.busy)),
                    percent(share(stats.total, self.busy)),
                    stats.calls.to_string(),
                ],
                &placed(stats.place.as_ref()),
            );
        }
        out
    }

    pub fn to_value(&self, heap: &mut Heap) -> Result<Value, Error> {
        let tree = self
            .tree
            .iter()
            .map(|branch| branch_value(heap, branch))
            .collect::<Result<Vec<_>, _>>()?;
        let tree = vector(heap, &tree)?;
        let functions = self
            .functions
            .iter()
            .map(|stats| function_value(heap, stats))
            .collect::<Result<Vec<_>, _>>()?;
        let functions = vector(heap, &functions)?;
        let ticks = self
            .ticks
            .iter()
            .map(|tick| {
                let kind = text(heap, &tick.kind)?;
                heap.map_from(&[
                    Value::keyword("kind"),
                    kind,
                    Value::keyword("took"),
                    Value::Float(milliseconds(tick.took)),
                    Value::keyword("at"),
                    Value::Float(milliseconds(tick.at)),
                ])
            })
            .collect::<Result<Vec<_>, _>>()?;
        let ticks = vector(heap, &ticks)?;
        let report = text(heap, &self.report())?;
        heap.map_from(&[
            Value::keyword("wall"),
            Value::Float(milliseconds(self.wall)),
            Value::keyword("busy"),
            Value::Float(milliseconds(self.busy)),
            Value::keyword("samples"),
            count(self.samples),
            Value::keyword("tree"),
            tree,
            Value::keyword("functions"),
            functions,
            Value::keyword("ticks"),
            ticks,
            Value::keyword("report"),
            report,
        ])
    }
}

fn vector(heap: &mut Heap, items: &[Value]) -> Result<Value, Error> {
    heap.vector_from(items).map_err(Error::from)
}

fn text(heap: &mut Heap, value: &str) -> Result<Value, Error> {
    heap.string(value).map_err(Error::from)
}

fn count(value: u64) -> Value {
    Value::Int(i64::try_from(value).unwrap_or(i64::MAX))
}

fn kind_value(kind: Kind) -> Value {
    Value::keyword(match kind {
        Kind::Plugin => "plugin",
        Kind::Function => "function",
        Kind::Native => "native",
    })
}

fn place_values(heap: &mut Heap, place: Option<&Place>) -> Result<(Value, Value), Error> {
    match place {
        Some(place) => Ok((
            text(heap, &place.file)?,
            count(u64::try_from(place.line).unwrap_or(u64::MAX)),
        )),
        None => Ok((Value::Nil, Value::Nil)),
    }
}

fn branch_value(heap: &mut Heap, branch: &Branch) -> Result<Value, Error> {
    let children = branch
        .children
        .iter()
        .map(|child| branch_value(heap, child))
        .collect::<Result<Vec<_>, _>>()?;
    let children = vector(heap, &children)?;
    let label = text(heap, &branch.label)?;
    let (file, line) = place_values(heap, branch.place.as_ref())?;
    heap.map_from(&[
        Value::keyword("name"),
        label,
        Value::keyword("kind"),
        kind_value(branch.kind),
        Value::keyword("total"),
        Value::Float(milliseconds(branch.total)),
        Value::keyword("self"),
        Value::Float(milliseconds(branch.own)),
        Value::keyword("calls"),
        count(branch.calls),
        Value::keyword("file"),
        file,
        Value::keyword("line"),
        line,
        Value::keyword("children"),
        children,
    ])
}

fn function_value(heap: &mut Heap, stats: &FunctionStats) -> Result<Value, Error> {
    let label = text(heap, &stats.label)?;
    let module = text(heap, &stats.module)?;
    let (file, line) = place_values(heap, stats.place.as_ref())?;
    heap.map_from(&[
        Value::keyword("name"),
        label,
        Value::keyword("module"),
        module,
        Value::keyword("kind"),
        kind_value(stats.kind),
        Value::keyword("total"),
        Value::Float(milliseconds(stats.total)),
        Value::keyword("self"),
        Value::Float(milliseconds(stats.own)),
        Value::keyword("calls"),
        count(stats.calls),
        Value::keyword("file"),
        file,
        Value::keyword("line"),
        line,
    ])
}
