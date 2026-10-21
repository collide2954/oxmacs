# Ox internals: the `ox` crate, phase 1

Companion to [ox-design.md](ox-design.md). This is how the language is built. Phase 1 is a standalone crate, `crates/ox`, with no dependency on any oxmacs crate: it builds in seconds and has its own `ox` command-line tool.

## Constraints

- **Safe Rust only.** The workspace forbids `unsafe`, and the VM is no exception. Heap objects live in typed arenas addressed by indices, and the interpreter's bounds checks are the price, to be won back by a JIT later.
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

```
crates/ox/src/
  lib.rs          Isolate: the public API (eval, load, define natives, run tasks)
  span.rs         SourceId, Span, SourceMap (line and column on demand)
  lexer.rs        tokens with spans
  reader.rs       tokens to Syntax, with error recovery
  syntax.rs       Syntax objects: datum, span, scope set
  symbol.rs       process-wide interner: Sym for symbols and keywords
  value.rs        Value, Ref, Kind
  heap/           typed arenas, roots, collector (mark, sweep, young list, incremental)
  string.rs       OxString: UTF-8 with character count and sparse index
  vector.rs       persistent vector (32-way trie plus tail)
  map.rs          persistent hash map and set (HAMT)
  number.rs       Int, Float, BigInt arithmetic and promotion
  record.rs       record types and instances
  expand/         scopes, binding table, core forms, macro expansion, syntax-rules
  compile/        IR, closure and assignment conversion, register allocation, codegen
  bytecode.rs     instructions, Code objects, constant pools, span tables
  vm/             interpreter loop, tasks, frames, handlers, scheduler, promises
  module.rs       modules, loader, live redefinition, bytecode cache
  native.rs       native functions: FromOx and IntoOx, arity adapters, foreign values
  prelude/        the core library, mostly written in Ox (.ox files embedded)
  print.rs        display and write
  bin/ox.rs       `ox file.ox` and the REPL
crates/ox/tests/
  corpus.rs       runs tests/ox/*.ox: each `;=> value` or `;! error` line is checked
  ox/*.ox         the language test corpus
  budgets.rs      benchmarks against the budgets (run in pre-push)
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
- **Heap kinds.** `Kind` names the arena: String, Pair, BigInt, Vector, VecNode, Map, MapNode, Record, RecordType, Closure, Code, Atom, Cell (a captured variable that is assigned), Native, Foreign, Task, Promise, Syntax.
- **Equality.** `=` is structural for strings, lists, vectors, maps and records. `identical?` compares references.
- **Hashing.** Structural for the same kinds. Strings cache their hash.
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
- **Maps and sets.** A HAMT with bitmap-indexed nodes (32-way) and collision nodes. A set is a map whose values are `true`. Small maps (up to 8 entries) are flat arrays, which keep records-as-maps and keyword arguments cheap.
- **Records.** `(def-record forecast day high low sky)` creates a `RecordType` (its name and field symbols) and instances with a field array. Records are immutable; `(with r :high 30)` copies one. `r.day` compiles to a field access with an inline cache keyed by record type; on a map it is `get`.
- **Lists.** Immutable pairs. `()` is `nil`.
- **Atoms.** The mutable cells: `atom`, `@a` (deref), `reset!` and `swap!`.

## Reader

- **Lexer.** A hand-written lexer over `&str`, producing tokens with byte spans. The reader builds `Syntax { datum, span, scopes }`.
- **Sugar.** `'x` reads as `(quote x)`; a backquoted form, `~x` and `~@x` read as quasiquote, unquote and unquote-splicing. `@a` reads as `(deref a)`. `a.b.c` reads as `(. a :b :c)`; a symbol made only of dots, such as `...`, stays a symbol. `"Hi {x.name}!"` reads as `(str "Hi " (. x :name) "!")`, and `{{` and `}}` escape the braces.
- **Errors.** Read errors do not stop the reader: unterminated strings, unbalanced brackets and odd map literals are reported with their spans while reading continues, so the editor can show every error at once and still highlight the rest.

## Expander

- **Hygiene.** By sets of scopes (Flatt 2016). Each identifier carries a set of scopes. A binding is recorded as (symbol, scope set) to target, and resolution picks the binding whose scope set is the largest subset of the identifier's.
- **Expansion.** Each macro expansion adds a use-site scope and flips a fresh introduction scope, so names a macro introduces never capture the user's names, and the reverse.
- **Core forms.** After expansion only these remain: `quote`, `def`, `fn`, `let`, `if`, `do`, `set!`, `.`, `try`, `raise`, `await`, `yield`, `match`, `with-param`, `module`, `import` and `export`. Everything else is a macro, most of them in the prelude: `cond`, `when`, `unless`, `and`, `or`, `when-let`, `if-let`, `->`, `->>`, `for`, `def-record`, `defcommand` (in phase 2) and `quasiquote`.
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

- **Encoding.** Instructions are `u32`: an 8-bit opcode with three 8-bit operands, or one 8-bit and one 16-bit operand. Wide variants cover larger register numbers.
- **Code objects.** A `Code` object holds:
  - its name and arity (required, optional, rest and keyword parameters);
  - its register count and instructions;
  - its constants, child codes and capture descriptors;
  - a table from instruction to span, for tracebacks.
- **Tasks.** A task owns a `Vec<Value>` register stack, a `Vec<Frame>` (code, closure, base and return slot) and a handler stack. Calling Ox never recurses on the Rust stack: deep recursion only grows the task's vectors.
- **Calls.** `CALL a b c` calls `r[a]` with arguments `r[a+1 ..= a+b]` and leaves the result in `r[a]`. Keyword arguments arrive as trailing pairs that the callee's prologue sorts into slots; with none, there is no prologue cost.
- **Inline caches.** Global references are linked to slot indices when a module loads, so there are no hash lookups at run time. Field access caches (record type, field index) per instruction.
- **Preemption.** Each task runs with an instruction budget, decremented on calls and backward jumps. When it runs out, `run` returns `Yielded` with the task left exactly where it stopped. An interrupt flag, set by `C-g`, is checked at the same points and raises `interrupted`.
- **Natives.** A native is a Rust function over `&mut Ctx` and its arguments. Higher-order functions that call back into Ox (`map`, `filter`, `reduce`, `sort-by`) are written in the Ox prelude, so the VM never needs a Rust re-entry.

## Errors

- **Raising.** `raise` accepts any value. By convention errors are records: `def-error` defines a record type that also carries a message and the traceback.
- **Catching.** `(try body (catch http-error e …) (catch _ e …))` pushes a handler (catch address and frame depth). `raise` unwinds to the nearest handler whose clause matches; with none, the task fails.
- **Tracebacks.** A traceback is the list of (code, instruction) pairs captured at `raise` and resolved to spans lazily. It prints as `file:line:column` lines that the editor makes clickable.
- **Native errors.** A native returns `Result<Value, Error>`, and an `Err` becomes an Ox error at the call site. A panic in a native is caught at the native boundary and becomes an Ox error too, so it never unwinds through the VM.

## Concurrency

- **Generators.** `(gen … (yield x) …)` creates a task with its own stack, and `next` resumes it. Switching between tasks is a change of the VM's current task, not a Rust call.
- **Promises.** `async` functions return promises. `await` on a pending promise suspends the task, and when the host resolves the promise (`isolate.resolve(p, v)`) the waiting tasks become runnable.
- **Green tasks.** `spawn` creates one.
- **Scheduler.** The isolate's scheduler is `isolate.run(budget) -> Report { ran, idle, next_timer }`. The editor calls it once per frame with about 4 ms; the `ox` tool calls it in a loop.
- **Workers.** `spawn-worker` starts another isolate on a thread. Messages are deep-copied into a heap-independent `Portable` form, with strings in `Arc<str>`, and rebuilt in the receiving heap. A shared frozen arena for large values can come after phase 1 if the benchmarks call for it.

## Modules and the cache

- **Modules.** A file is a module, introduced by `(module name (import …) (export …))`. `(import (ox editor) (http :as h) (json :only [read]))` makes `h/get` and `read` available. Built-in modules (`ox core`, `ox string` and so on) are native or prelude modules. Plugin modules resolve relative to the plugin folder. Cyclic imports are an error that names the cycle.
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
- **Foreign values.** `ctx.foreign(T)` wraps any `T: Any + Send` (in phase 2, editor objects such as buffers and windows), and `ctx.foreign_ref::<T>(v)` gets it back with a type check.

## Testing and benchmarks

- **Unit tests.** Every module has unit tests.
- **Corpus.** The corpus in `tests/ox/*.ox` is the language specification by example. Each top-level form may be followed by `;=> printed-result` or `;! error-message`, and the runner checks them all.
- **Budgets.** `tests/budgets.rs` is ignored by default and run by pre-push. It measures:
  - startup of a cached module set;
  - the cost of a call from Rust into Ox and back;
  - a closure-heavy loop and a float loop;
  - vector and map churn;
  - GC pause percentiles.

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
24. **Add persistent hash maps and sets.** Including collisions and flat small maps.
25. **Read keys with a.b and get, on maps and records.**
26. **Define records with def-record and update them with with.**
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
54. **Report definite type mismatches at compile time.**
55. **Use typed instructions for annotated numbers.**
56. **Measure the budgets and run them in pre-push.**

Phase 1 is done when the corpus passes and `budgets.rs` meets every budget.

## Risks

- **Bounds checks and index indirection cost speed.** Mitigations: typed arenas keep data dense, the hot loop is profiled early (slice 18 onward), and the value API keeps NaN-boxing possible.
- **Hygiene is subtle.** Mitigation: sets of scopes is the best-understood modern algorithm, and the corpus carries the classic hygiene tests from the Racket and R7RS suites, rewritten in Ox.
- **The incremental collector can lose objects.** Mitigation: a debug mode verifies after each collection that nothing reachable was freed, and runs stress tests that collect at every allocation.
- **Scope creep.** Mitigation: the JIT, the frozen shared arena and a richer type system stay out of phase 1.
