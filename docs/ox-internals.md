# Ox internals: the `ox` crate, phase 1

Companion to [ox-design.md](ox-design.md). This is how the language is built. Phase 1 is a standalone crate, `crates/ox`, with no dependency on any oxmacs crate: it builds in seconds and has its own `ox` command-line tool.

## Constraints

- **Safe Rust only.** The workspace forbids `unsafe`, and the VM is no exception. The one exception is `ox-perf`, a developer tool outside the default build, whose allocation counter and stack sampler can't be written without it. Heap objects live in typed arenas addressed by indices, and the interpreter's bounds checks are the price, to be won back by a JIT later.
- **Few dependencies.** `num-bigint` (bignums) is the only planned one; everything else is written here.
- **Speed budgets.**
  - Startup with every bundled plugin: under 100 ms (bytecode cache plus lazy loading).
  - Keystroke to frame: under 4 ms.
  - GC pauses: under 1 ms at the 99th percentile.

## Language details settled for the reader

- `true`, `false` and `nil`. Only `false` and `nil` are false, and `()` is `nil`.
- `name:` in a map literal is the keyword `:name`. `a.b` reads key `:b` of `a`, and the same works inside `"{…}"`.
- `/` divides to a float and `//` floors. Integers grow into bignums. There are no rationals.
- Strings are indexed by character.
- Characters are written `\a`, `\newline`, `\tab`, `\space` and `\u{263A}`.
- Comments are `;` (to the end of the line), `#;` (the next datum) and `#| … |#` (a block).
- Commas are whitespace, so `{"a b" 1, 42 "x"}` reads cleanly. Quasiquote uses a backquote, with `~` (unquote) and `~@` (splicing), as in Clojure.
- Module-qualified names are written `alias/name`.

## Layout

The source is grouped by stage, in the order a form passes through them.

```
crates/ox/src/
  lib.rs            module list and the public re-exports
  isolate.rs        Isolate: the public API (eval, define natives, root, collect)
  span.rs           Span and line and column lookup
  symbol.rs         process-wide interner: Sym for symbols and keywords
  task.rs           TaskId, Report and Interrupter for the host
  portable.rs       values copied between isolates, channels and the wake-up signal
  error.rs          Error with an optional span
  bytecode.rs       instructions, Code objects, constant pools, span tables
  native.rs         native functions and their arity checks
  read/             source text to Syntax, with error recovery
    syntax.rs       Datum and Syntax (and the marks hygiene adds)
  expand/           macro expansion; quote.rs turns quoted forms into data
  compile/          Syntax to bytecode: scopes, captures, register allocation
    parameters.rs   parameter lists: required, optional, rest and keyword
  vm/               the interpreter loop, frames, calls and tail calls
    bind.rs         binding arguments to parameters
    build.rs        building literals, reading fields, calling data
    machine.rs      a machine's stack, frames and handlers; fibers and generators
    tasks.rs        spawning, cancelling, waiting on promises and channels, and the scheduler
    convert.rs      turning values into their portable form and back
    workers.rs      starting worker threads and collecting their results
  heap/             typed arenas and rooted handles
    gc.rs           mark and sweep
    jobs.rs         background work on threads that settles promises
  value/            Value, Ref and Kind, and the data types
    number.rs       Int, Float and BigInt arithmetic and promotion
    vector.rs       persistent vector (32-way trie plus tail)
    map.rs          insertion-ordered persistent map and set (HAMT index)
    record.rs       record types and instances
    equal.rs        structural equality and hashing
    print.rs        display and write
    promise.rs      promise states
    foreign.rs      Rust values held by Ox
  library/          natives, grouped by area, and the Ox prelude
    numbers.rs      arithmetic and comparison
    collections.rs  sequences, vectors, maps, sets, records
    tasks.rs        generators, promises, channels, timers and spawn
    time.rs         ox time: now, durations and parsing
    json.rs         ox json: read and write
    http.rs         ox http: get and percent-encoding
    strings.rs      ox string
    prelude/*.ox    the parts of the core library written in Ox
  bin/ox.rs         `ox file.ox` and the REPL
crates/ox/tests/
  corpus.rs         runs tests/ox/*.ox: each `;=> value` or `;! error` line is checked
  ox/*.ox           the language test corpus
  cli.rs            the `ox` command line
  budgets.rs        benchmarks against the budgets (run in pre-push)
```

## Values

```rust
#[derive(Clone, Copy)]
pub enum Value {
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    Char(char),
    Symbol(Sym),
    Keyword(Sym),
    Ref(Ref),
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ref { kind: Kind, index: u32 }
```

- **Size.** A value is 16 bytes and `Copy`. Code outside `value.rs` uses constructors and accessors only, so a NaN-boxed 8-byte form can replace it later.
- **Heap kinds.** `Kind` names the arena: String, Pair, BigInt, Vector, VecNode, Map, MapNode, Record, RecordType, Closure, Code, Atom, Cell (a captured variable that is assigned), Native, Foreign, Generator, Promise, Channel, Identifier.
- **Equality.** `=` is structural for strings, lists, vectors, maps and records, and a list equals a vector with the same items, so `nil`, the empty list, equals `[]`. On floats `=` is an equivalence: NaN equals NaN, and `0.0` differs from `-0.0`. `==` compares numbers by IEEE value. `identical?` compares references.
- **Hashing.** Structural and consistent with `=`, so a list and a vector with the same items hash alike. Strings cache their hash.
- **Symbols and keywords.** Interned once per process (a read-mostly `RwLock` table of leaked names), so a symbol means the same thing in every isolate.

## Heap and collector

- **Arenas.** Each kind has its own arena. An arena holds `Vec<Option<T>>` slots, a free list, mark bits, young bits and a list of young indices. Allocation reuses a free slot or pushes a new one, and records the index as young.
- **Roots.** A heap value is alive while it is reachable from:
  - task stacks (registers and frames);
  - module global slots;
  - the constant pools of loaded code;
  - rooted handles. Rust holds `Rooted`, an `Rc` whose cell the heap tracks through a `Weak`; dropping the `Rc` unroots the value at the next collection with no explicit call.
- **Young collections.** Data is immutable, so a new object can only point at older ones. An old object can point at a young one only through a mutation: an `atom` reset, an assigned captured variable (`Cell`), or a global slot. Those writes go through a barrier that records the old cell in a remembered set when the new value is young.
  - A minor collection marks from the roots and the remembered set, entering young objects only.
  - It then frees unmarked young slots and promotes the survivors by clearing their young bit.
  - Its cost follows the live young objects, not the heap size.
- **Old collections.** These are incremental and interleaved with running code:
  - **Marking.** A gray stack is processed in slices of fixed work (target 0.2 ms).
  - **Barrier.** While marking, the same write barrier also shades the new value (Dijkstra), and new objects are allocated already marked.
  - **Finish.** Task stacks are rescanned atomically, which is cheap because they are shallow.
  - **Sweeping.** Each arena is swept lazily in chunks.
- **Order of work.** Slices 31–34 below build this in steps: first a stop-the-world mark and sweep, then young collections, then incremental old marking. Each step keeps the earlier tests green.

## Strings, numbers, collections

- **`OxString`.** Holds `Box<str>`, its character count, an ASCII flag (when set, character and byte positions are equal), a cached hash, and, for non-ASCII text, a sparse index of the byte offset of every 64th character, built lazily. Interpolation compiles to one `str` call over the pieces.
- **Numbers.** `Int(i64)` arithmetic is checked and promotes to a heap `BigInt` (num-bigint) on overflow; a `BigInt` that fits again demotes. `Float` is f64. `/` returns a float, `//` the floor, and `mod` and `rem` follow the floor and truncation rules.
- **Vectors.** Clojure's persistent vector: a header (count, shift, root, tail) over 32-way `VecNode`s, which are ordinary heap objects. `get` and `assoc` take O(log32 n), and `conj` is amortized constant time through the tail. `transient` versions mutate nodes they own while being built.
- **Maps and sets.** Keys keep insertion order, so printing, iteration, JSON round-trips and snapshots are predictable. A map is a persistent vector of entries in insertion order plus a HAMT (bitmap-indexed 32-way nodes and collision nodes) from each key to its entry. Removing a key leaves a hole that iteration skips, and the map is compacted when holes outnumber entries. Keyword keys print with the literal shorthand, `{name: "Ana" age: 3}`. A set is a map whose values are `true`. Small maps (up to 8 entries) are flat arrays scanned in order, which keep records-as-maps and keyword arguments cheap.
- **Records.** Fields are declared with parameter syntax: `(defrecord Point [x y (z 0) {label nil}])` has required, optional and keyword fields, so `(Point 1 2)` and `(Point 1 2 3 label: "a")` both build one. It creates a `RecordType` (its name and fields) and instances with a field array. Records are immutable; `(with r :high 30)` copies one, and `assoc` of a field the record does not declare is an error ("Point has no field color"), so a record always has exactly its fields. A record prints as `#Point{x: 1 y: 2}`, which reads back as a record literal. `r.day` compiles to a field access with an inline cache keyed by record type; on a map it is `get`, and on `nil` it gives `nil`.
- **Lists.** Immutable pairs. `()` is `nil`.
- **Atoms.** The mutable cells: `atom`, `@a` (deref), `reset!` and `swap!`.

## Reader

- **Lexer.** A hand-written lexer over `&str`, producing tokens with byte spans. The reader builds `Syntax { datum, span, scopes }`.
- **Sugar.** `'x` reads as `(quote x)`; a backquoted form, `~x` and `~@x` read as quasiquote, unquote and unquote-splicing. `@a` reads as `(deref a)`. `a.b.c` reads as `(. a :b :c)`; a symbol made only of dots, such as `...`, stays a symbol. `"Hi {x.name}!"` reads as `(str "Hi " (. x :name) "!")`, and `{{` and `}}` escape the braces.
- **Errors.** Read errors do not stop the reader: unterminated strings, unbalanced brackets and odd map literals are reported with their spans while reading continues, so the editor can show every error at once and still highlight the rest.

## Expander

- **Hygiene.** By marks. Each identifier carries a set of marks, empty for code you write. A local binding is recorded with its name and marks, and a reference resolves only to a binding with the same name and the same marks; a reference no local matches is a global. Special form names stay reserved, so a macro's `if` is always `if`.
- **Expansion.** A separate pass before compiling, so the compiler sees only core forms and finds every `set!`. A macro receives its argument forms as data whose names are identifier objects that keep their marks. Plain symbols in what it returns are the names it introduced and get a fresh mark, so they never capture the user's names, and the reverse. Expansion stops with an error 256 levels deep.
- **Core forms.** After expansion only these remain: `quote`, `def`, `fn`, `let`, `if`, `do`, `set!`, `.`, `try`, `raise`, `match`, `with-param`, `module`, `import` and `export`. Everything else is a macro, most of them in the prelude: `cond`, `when`, `unless`, `and`, `or`, `when-let`, `if-let`, `->`, `->>`, `for`, `gen`, `spawn`, `defrecord`, `defcommand` (in phase 2) and `quasiquote`. `cond` takes `[test body…]` clauses ending in `[else …]`, the same shape as `match`.
- **Macros.** `defmacro` defines a procedural macro: a function from syntax to syntax, run at expansion time in the compiling isolate, which may import other modules. `syntax-rules` is a macro that builds such a function from patterns and templates. Ox has no separate run-time and compile-time phases; a macro sees whatever its module imports.

## Compiler

- **Pipeline.** Expanded core forms become an IR in A-normal form. Then the compiler:
  - resolves variables to locals, captured variables (upvalues) or global slots;
  - converts assignment: locals that are both captured and assigned live in a `Cell`;
  - converts closures to flat closures;
  - compiles `match` into a decision tree;
  - compiles destructuring into the same tests as `match`;
  - allocates registers with linear scan over the ANF temporaries;
  - generates code.
- **Tail calls.** A call in tail position becomes `TAILCALL`, which reuses the frame.
- **Types.**
  - **Entry checks.** An annotated parameter is checked at function entry (one tag test), so typed code can trust its types.
  - **Typed arithmetic.** Typed operands get typed instructions such as `ADD_INT`, `ADD_FLOAT` and `LT_FLOAT`, which skip dispatch.
  - **Compile-time errors.** A definite mismatch, such as a typed function called with a literal of the wrong type, is a compile-time error with a span.
  - **Untyped code.** It is unaffected.

## Bytecode and VM

- **Encoding.** Instructions are a compact `Copy` enum, `Op`, with 16-bit register operands and 32-bit constant indices. A `match` on it compiles to a jump table, and no hand-written bit packing can go wrong. Code objects are shared through `Rc`, so an isolate stays on the thread that created it; worker isolates are created on their own threads.
- **Code objects.** A `Code` object holds:
  - its name and arity (required, optional, rest and keyword parameters);
  - its register count and instructions;
  - its constants, child codes and capture descriptors;
  - a table from instruction to span, for tracebacks.
- **Tasks.** A task owns a `Vec<Value>` register stack, a `Vec<Frame>` (code, closure, base and return slot) and a handler stack. Calling Ox never recurses on the Rust stack: deep recursion only grows the task's vectors.
- **Calls.** `CALL a b c` calls `r[a]` with arguments `r[a+1 ..= a+b]` and leaves the result in `r[a]`. Keyword arguments arrive as trailing pairs that the callee's prologue sorts into slots; with none, there is no prologue cost.
- **Inline caches.** Global references are linked to slot indices when a module loads, so there are no hash lookups at run time. Field access caches (record type, field index) per instruction.
- **Preemption.** Every 1,024 instructions the VM collects garbage if the heap has grown enough, and checks the clock and the interrupt flag. When a task's turn is over, it stops with the task left exactly where it was. An interrupt (`C-g`, or `cancel` on one task) raises an `Interrupted` error that no `catch` clause catches, not even `_`, while `finally` blocks still run. `eval` checks the flag too, so runaway code at the REPL can be stopped.
- **Natives.** A native is a Rust function over `&mut Ctx` and its arguments. Higher-order functions that call back into Ox (`map`, `filter`, `reduce`, `sort-by`) are written in the Ox prelude, so the VM never needs a Rust re-entry.

## Errors

- **Raising.** `raise` accepts any value. By convention errors are records: `deferror` defines a record type that also carries a message and the traceback.
- **Catching.** `(try body (catch http-error e …) (catch _ e …))` pushes a handler (catch address and frame depth). `raise` unwinds to the nearest handler whose clause matches; with none, the task fails.
- **Tracebacks.** A traceback is the list of (code, instruction) pairs captured at `raise` and resolved to spans lazily. It prints as `file:line:column` lines that the editor makes clickable.
- **Native errors.** A native returns `Result<Value, Error>`, and an `Err` becomes an Ox error at the call site. A panic in a native is caught at the native boundary and becomes an Ox error too, so it never unwinds through the VM.

## Concurrency

- **Generators.** `(gen …)` is `(generator (fn [] …))`, which gives the function its own machine (stack, frames and handlers). A task runs a fiber: the current machine plus the machines waiting on `next`. `next` pushes the caller and switches to the generator's machine, and `yield` switches back, so `yield` works from any function the generator calls. Switching is a change of machine inside the interpreter loop, not a Rust call. An error that leaves a generator ends it and is raised in the caller.
- **Promises.** `yield`, `next`, `await`, `generator` and `spawn-call` are control natives: ordinary function values that the interpreter handles itself, so they can be passed around and called in tail position. `await` on a pending promise blocks the whole task, and when it is delivered (by `deliver`, by a finished task, or by the host with `isolate.resolve(p, v)` or `isolate.reject(p, message)`) the task becomes runnable again. `eval` runs other tasks while it waits, and raises "Nothing can deliver this promise" when none are left.
- **Green tasks.** `(spawn …)` is `(spawn-call (fn [] …))`, which starts a task and returns a promise of its result. A failure that nothing is waiting for is listed in the report's `unhandled`.
- **Scheduler.** The host starts tasks with `isolate.spawn(f, args)` and runs them with `isolate.run(budget) -> Report { finished, unhandled, pending }`. Tasks take turns, each with an equal share of what is left of the budget, and a task that pauses goes to the back of the queue. The editor calls it once per frame with about 4 ms; the `ox` tool calls it in a loop.
- **Workers.** `(worker 'module 'function args…)` starts a fresh isolate on its own thread, with the same module roots and the host's worker setup (`isolate.set_worker_setup`, which installs natives). It loads the module, calls the function and returns a promise of its result. The worker ends when the function returns, along with any tasks it started, and C-g does not reach it.
- **Messages.** Arguments, results and channel items are deep-copied into a heap-independent `Portable` form, with strings in `Arc<str>`, and rebuilt in the receiving heap. Records come back as the receiver's own type when it has loaded the module that defines it, and otherwise as a stand-in type with the same name and fields. Channels and shared Rust values cross by reference. Functions, atoms, generators, promises and local Rust values can't be copied. A shared frozen arena for large values can come after phase 1 if the benchmarks call for it.
- **Channels.** `(chan n)` holds up to n items (32 by default) behind a mutex, so isolates on different threads can share one. `send` waits while it is full and `receive` while it is empty. After `close`, `receive` gives nil once the items run out, and `done?` turns true. Within one isolate a channel copies its items too, so a channel behaves the same whoever is on the other end.
- **Waiting on other threads.** An isolate with nothing runnable sleeps on a condition variable that channels and finished workers signal. It raises "Nothing can send to this channel" (or the matching message) only when no worker is running and no other isolate holds the channel.

## Modules and the cache

- **Modules.** A file is a module named by its path, introduced by `(module (import …) (export …))`. `(import (ox editor) (http :as h) (json :only [read]))` makes `h/get` and `read` available. Built-in modules (`ox core`, `ox string` and so on) are native or prelude modules. Plugin modules resolve relative to the plugin folder. Cyclic imports are an error that names the cycle.
- **Live reload.** Reloading a module re-runs it into the same global slots, so its importers see the new definitions at once.
- **Bytecode cache.** A compiled module serializes to a small versioned binary format:
  - constants, as data;
  - code objects with their span tables;
  - exports;
  - macro transformers, as compiled code.

  It is keyed by a hash of the source, the compiler version and the hashes of its imports' exports, and stored in the cache folder the host passes in.

## Native interface

```rust
isolate.define("ox string", "upcase", native(|_: &mut Ctx, s: Str| Ok(s.to_uppercase())));
```

- **Conversions.** `FromOx` and `IntoOx` are implemented for `i64`, `f64`, `bool`, `char`, `String`, `Str` (a rooted string handle), `Value`, `Vec<T>`, `Option<T>` and `Result`. Arity adapters for 0 to 6 arguments are generated by `macro_rules`, with no procedural macro and so no build cost.
- **Foreign values.** `isolate.foreign(name, T)` (or `heap.foreign` inside a native) wraps any `T: Any` as an opaque handle that prints as `#<name>`, and `heap.foreign_of::<T>(v)` gets it back with a type check. `isolate.shared(name, Arc<T>)` wraps a `T: Send + Sync` that can also cross to workers, such as a buffer snapshot. Handles compare by identity and drop the Rust value when collected. Fields and methods come with the editor API in phase 2. `isolate.define(name, value)` and `isolate.call(f, args)` pass values between the host and Ox.

## Testing and benchmarks

- **Unit tests.** Every module has unit tests.
- **Corpus.** The corpus in `tests/ox/*.ox` is the language specification by example. Each top-level form may be followed by `;=> printed-result` or `;! error-message`, and the runner checks them all.
- **Budgets.** `tests/budgets.rs` is ignored by default and run by pre-push. It measures:
  - startup of a cached module set;
  - the cost of a call from Rust into Ox and back;
  - a closure-heavy loop and a float loop;
  - vector and map churn;
  - GC pause percentiles.
- **ox-perf.** `crates/ox-perf` is a developer tool left out of the default build. `cargo run -p ox-perf --profile profiling -- bench crates/ox-perf/benchmarks/vm.ox` loads a file that ends with a map of names to functions, or to `[count function]` pairs, runs each benchmark for about a second and prints its best time, the time per item and the Rust allocations it made. `sample FILE NAME` samples the native stack while one benchmark runs and ranks self time by function and by line, and total time by function, with inlined functions resolved; it needs x86-64 Windows for now. `--root DIR` adds module roots: `--root crates/oxmacs-ox/plugins` lets `benchmarks/screensavers.ox` draw the screensavers' frames. The sampler copies the stack while the thread is suspended and walks the copy after resuming it, so it never waits on a lock that thread holds.

## Slice plan

Each slice is one commit and starts with a failing test.

1. **Add the ox crate and read integers, floats and symbols with their spans.** Reading `42 -7 3.5 foo` gives those four data with exact byte spans.
2. **Read strings, characters and keywords.** Escapes, `\u{…}`, `\newline`, and `:key` as a keyword.
3. **Read lists, vectors and maps, with `name:` keys.** `{a: 1 :b 2}` reads as a map datum of two keyword keys.
4. **Read true, false, nil, quote forms and the three kinds of comments.**
5. **Read field access and string interpolation.** `a.b` reads as `(. a :b)`, and `"x{y}"` as `(str "x" y)`.
6. **Report read errors with spans and keep reading.** Two errors in one source are both reported, and the valid data are still read.
7. **Represent values and allocate strings and pairs in typed arenas.**
8. **Intern symbols and keywords for the whole process.** The same name gives the same `Sym` across isolates.
9. **Print values the way they are read.** Printing reads back to equal data for every literal kind.
10. **Evaluate constants through the compiler and VM.** `42` evaluates to 42.
11. **Call native arithmetic and comparison.** `(+ 1 2)`, `(/ 7 2)` = 3.5 and `(// 7 2)` = 3. The native registration API arrives here.
12. **Evaluate if and do, where only false and nil are false.**
13. **Define globals with def and read them back.**
14. **Make closures with fn and call them, with `(def (f x) …)` sugar.**
15. **Bind locals with let, including destructuring-free sequential bindings.**
16. **Run .ox files and a REPL from the ox command line.**
17. **Capture variables in closures and assign them with set!**
18. **Recurse deeply without the Rust stack, and loop with proper tail calls.** A loop of 10⁶ tail calls and a non-tail recursion 10⁵ deep both work.
19. **Support optional, rest and keyword parameters.**
20. **Grow integers into bignums and shrink them back.**
21. **Index strings by character and build them by interpolation.**
22. **Compare and hash values structurally.**
23. **Add persistent vectors with conj, get, assoc and pop.** 100,000 elements, with older versions left intact.
24. **Add persistent hash maps and sets in insertion order.** Including collisions, removal holes and flat small maps.
25. **Read keys with a.b and get, on maps and records.**
26. **Define records with defrecord and update them with with.**
27. **Cache field access per record type.**
28. **Make atoms the mutable cell, with deref, reset! and swap!**
29. **Quasiquote with unquote and splicing.**
30. **Load the prelude written in Ox.** `map`, `filter`, `reduce`, `cond`, `when`, `and`, `or`, `->`.
31. **Collect unreachable objects with mark and sweep.**
32. **Keep values alive that Rust holds through rooted handles.**
33. **Collect young objects on their own, with a remembered set for mutated cells.**
34. **Mark the old generation incrementally in bounded slices.** Mutating during marking loses nothing.
35. **Give syntax scope sets and resolve identifiers through them.**
36. **Define procedural macros with defmacro.**
37. **Define hygienic macros with syntax-rules.** The classic swap and `or` capture tests pass.
38. **Match values against patterns with destructuring.**
39. **Destructure in let and fn parameters.**
40. **Raise and catch errors by record type.**
41. **Give errors tracebacks with source spans.**
42. **Turn native errors and panics into Ox errors.**
43. **Load a file as a module with imports and exports.**
44. **Redefine a module's definitions in place when it reloads.**
45. **Refuse cyclic imports by naming the cycle.**
46. **Cache compiled modules and load them without compiling.**
47. **Pause a running task after an instruction budget and resume it.**
48. **Interrupt a runaway task.**
49. **Yield values from generators.**
50. **Await promises the host resolves, and spawn green tasks.**
51. **Run worker isolates and send immutable values between them.**
52. **Wrap Rust values as foreign values.**
53. **Check type annotations at function entry.**
54. **Infer types through unannotated code and report definite mismatches.**
55. **Make nil explicit: T never holds nil, T? may, and nil tests narrow.**
56. **Check generic vectors, maps and functions per use.**
57. **Check map shapes like record fields.**
58. **Name types with deftype, including unions.**
59. **Require match to handle every case of a union, record or T?.**
60. **Run ox check from the command line.**
61. **Use typed instructions for annotated numbers.**
62. **Run inline test blocks with ox test, skipping them on load.**
63. **Explain failed expectations with the values of each sub-expression.**
64. **Run doc examples as tests.**
65. **Generate and shrink values for property tests.**
66. **Compare output with stored snapshots and offer to update them.**
67. **Keep comments and their spans when reading for tools.**
68. **Format Ox in one canonical style with ox fmt.**
69. **Serve reader and compile diagnostics to other editors with ox lsp.**
70. **Format documents and list their definitions over LSP.**
71. **Complete names and show signatures and documentation over LSP.**
72. **Go to definitions, find references and rename over LSP.**
73. **Show inferred types and type errors over LSP.**
74. **Measure the budgets and run them, ox check and ox test in pre-push.**

Phase 1 is done when the corpus passes, `ox check`, `ox test` and `ox fmt --check` are clean, `ox lsp` serves its features, and `budgets.rs` meets every budget. `ox lsp` lives in its own crate, `crates/ox-lsp`. Its first slices (69 and 70) can come as soon as the reader and compiler are stable, ahead of the order above, and later ones follow the features they expose. The editor's `*tests*` runner comes with the embedding in phase 2.

## Risks

- **Bounds checks and index indirection cost speed.** Mitigations: typed arenas keep data dense, the hot loop is profiled early (slice 18 onward), and the value API keeps NaN-boxing possible.
- **Hygiene is subtle.** Mitigation: sets of scopes is the best-understood modern algorithm, and the corpus carries the classic hygiene tests from the Racket and R7RS suites, rewritten in Ox.
- **The incremental collector can lose objects.** Mitigation: a debug mode verifies after each collection that nothing reachable was freed, and runs stress tests that collect at every allocation.
- **Scope creep.** Mitigation: the JIT, the frozen shared arena and a richer type system stay out of phase 1.
