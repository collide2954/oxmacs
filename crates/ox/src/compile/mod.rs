mod parameters;

use parameters::parse_parameters;

use std::cell::Cell;
use std::rc::Rc;

use crate::bytecode::{Arity, Capture, Code, Op, Register};
use crate::error::Error;
use crate::expand::quote::datum_value;
use crate::heap::Heap;
use crate::read::{Datum, Marks, Syntax};
use crate::span::Span;
use crate::symbol::Sym;
use crate::value::Value;
use crate::value::record::RecordType;

pub fn compile(heap: &mut Heap, form: &Syntax) -> Result<Code, Error> {
    let mut compiler = Compiler {
        heap,
        current: Function::default(),
        outer: Vec::new(),
    };
    let target = compiler.reserve(1, form.span)?;
    compiler.expression(form, target)?;
    compiler.emit(Op::Return { from: target }, form.span);
    Ok(compiler.current.code)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    Register(Register),
    Captured(u16),
    Itself,
}

#[derive(Debug, Clone, Copy)]
struct Variable {
    place: Place,
    boxed: bool,
}

struct Local {
    name: Sym,
    marks: Marks,
    register: Register,
    boxed: bool,
}

#[derive(Default)]
struct Function {
    code: Code,
    top: Register,
    locals: Vec<Local>,
    captured: Vec<(Sym, Marks, bool)>,
    itself: Option<(Sym, Marks)>,
}

struct Compiler<'a> {
    heap: &'a mut Heap,
    current: Function,
    outer: Vec<Function>,
}

fn assigns(form: &Syntax, name: &str) -> bool {
    match &form.datum {
        Datum::List(items) => {
            let assigned = matches!(
                items.as_slice(),
                [
                    Syntax {
                        datum: Datum::Symbol(head),
                        ..
                    },
                    Syntax {
                        datum: Datum::Symbol(target),
                        ..
                    },
                    ..
                ] if head == "set!" && target == name
            );
            assigned || items.iter().any(|item| assigns(item, name))
        }
        Datum::Vector(items) | Datum::Map(items) | Datum::Set(items) | Datum::Record(_, items) => {
            items.iter().any(|item| assigns(item, name))
        }
        _ => false,
    }
}

#[derive(Clone, Copy)]
enum Body<'s> {
    Forms(&'s [Syntax]),
    Record { record: Value, fields: u16 },
}

fn assigned_in(forms: &[Syntax], name: &str) -> bool {
    forms.iter().any(|form| assigns(form, name))
}

fn text(syntax: &Syntax) -> &str {
    match &syntax.datum {
        Datum::Symbol(name) => name,
        _ => "",
    }
}

impl Compiler<'_> {
    fn function_at(&mut self, depth: usize) -> Option<&mut Function> {
        if depth == self.outer.len() {
            Some(&mut self.current)
        } else {
            self.outer.get_mut(depth)
        }
    }

    fn resolve(&mut self, name: Sym, marks: &Marks) -> Option<Variable> {
        self.resolve_at(self.outer.len(), name, marks)
    }

    fn resolve_at(&mut self, depth: usize, name: Sym, marks: &Marks) -> Option<Variable> {
        let function = self.function_at(depth)?;
        if let Some(local) = function
            .locals
            .iter()
            .rev()
            .find(|local| local.name == name && local.marks == *marks)
        {
            return Some(Variable {
                place: Place::Register(local.register),
                boxed: local.boxed,
            });
        }
        if let Some((index, (_, _, boxed))) = (0_u16..)
            .zip(&function.captured)
            .find(|(_, (captured, captured_marks, _))| *captured == name && captured_marks == marks)
        {
            return Some(Variable {
                place: Place::Captured(index),
                boxed: *boxed,
            });
        }
        if function
            .itself
            .as_ref()
            .is_some_and(|(itself, itself_marks)| *itself == name && itself_marks == marks)
        {
            return Some(Variable {
                place: Place::Itself,
                boxed: false,
            });
        }
        let outside = self.resolve_at(depth.checked_sub(1)?, name, marks)?;
        let function = self.function_at(depth)?;
        let index = u16::try_from(function.captured.len()).ok()?;
        function.captured.push((name, marks.clone(), outside.boxed));
        function.code.captures.push(match outside.place {
            Place::Register(register) => Capture::Register(register),
            Place::Captured(index) => Capture::Captured(index),
            Place::Itself => Capture::Itself,
        });
        Some(Variable {
            place: Place::Captured(index),
            boxed: outside.boxed,
        })
    }

    fn reserve(&mut self, count: usize, span: Span) -> Result<Register, Error> {
        let function = &mut self.current;
        let base = function.top;
        function.top = u16::try_from(usize::from(base) + count)
            .map_err(|_| Error::at(span, "This function needs too many registers"))?;
        function.code.registers = function.code.registers.max(function.top);
        Ok(base)
    }

    fn release(&mut self, top: Register) {
        self.current.top = top;
    }

    fn constant(&mut self, value: Value, span: Span) -> Result<u32, Error> {
        let constants = &mut self.current.code.constants;
        let index = u32::try_from(constants.len())
            .map_err(|_| Error::at(span, "This function has too many constants"))?;
        constants.push(value);
        Ok(index)
    }

    fn emit(&mut self, op: Op, span: Span) {
        self.current.code.ops.push(op);
        self.current.code.spans.push(span);
    }

    fn load(&mut self, value: Value, target: Register, span: Span) -> Result<(), Error> {
        let index = self.constant(value, span)?;
        self.emit(Op::Constant { to: target, index }, span);
        Ok(())
    }

    fn expression(&mut self, form: &Syntax, target: Register) -> Result<(), Error> {
        self.form(form, target, false)
    }

    fn form(&mut self, form: &Syntax, target: Register, tail: bool) -> Result<(), Error> {
        match &form.datum {
            Datum::Symbol(name) => {
                self.variable(Sym::intern(name), &form.marks, target, form.span);
                Ok(())
            }
            Datum::List(items) => self.call(items, target, form.span, tail),
            Datum::Vector(items) => self.vector(items, target, form.span),
            Datum::Map(items) => self.map(items, target, form.span),
            Datum::Set(items) => self.set(items, target, form.span),
            Datum::Record(name, items) => {
                self.record_literal(name, &form.marks, items, target, form.span)
            }
            Datum::Nil
            | Datum::Bool(_)
            | Datum::Int(_)
            | Datum::Float(_)
            | Datum::Char(_)
            | Datum::Keyword(_)
            | Datum::Str(_)
            | Datum::BigInt(_) => {
                let value = datum_value(self.heap, form)?;
                self.load(value, target, form.span)
            }
        }
    }

    fn quote(&mut self, parts: &[Syntax], target: Register, span: Span) -> Result<(), Error> {
        let [form] = parts else {
            return Err(Error::at(span, "quote takes one form"));
        };
        let value = datum_value(self.heap, form)?;
        self.load(value, target, span)
    }

    fn variable(&mut self, name: Sym, marks: &Marks, target: Register, span: Span) {
        let Some(variable) = self.resolve(name, marks) else {
            self.emit(Op::Global { to: target, name }, span);
            return;
        };
        match variable.place {
            Place::Register(register) if variable.boxed => {
                self.emit(
                    Op::CellGet {
                        to: target,
                        cell: register,
                    },
                    span,
                );
                return;
            }
            Place::Register(register) if register == target => {}
            Place::Register(register) => self.emit(
                Op::Move {
                    to: target,
                    from: register,
                },
                span,
            ),
            Place::Captured(index) => self.emit(Op::Captured { to: target, index }, span),
            Place::Itself => self.emit(Op::Itself { to: target }, span),
        }
        if variable.boxed {
            self.emit(
                Op::CellGet {
                    to: target,
                    cell: target,
                },
                span,
            );
        }
    }

    fn here(&self, span: Span) -> Result<u32, Error> {
        u32::try_from(self.current.code.ops.len())
            .map_err(|_| Error::at(span, "This function is too long"))
    }

    fn patch(&mut self, jump: usize, to: u32) {
        if let Some(
            Op::Jump { to: target }
            | Op::JumpUnless { to: target, .. }
            | Op::JumpIfSupplied { to: target, .. },
        ) = self.current.code.ops.get_mut(jump)
        {
            *target = to;
        }
    }

    fn special(
        &mut self,
        head: &Syntax,
        rest: &[Syntax],
        target: Register,
        span: Span,
        tail: bool,
    ) -> Option<Result<(), Error>> {
        let Datum::Symbol(name) = &head.datum else {
            return None;
        };
        match name.as_str() {
            "if" => Some(self.branch(rest, target, span, tail)),
            "do" => Some(self.sequence(rest, target, span, tail)),
            "def" => Some(self.define(rest, target, span)),
            "fn" => Some(self.function(rest, target, span)),
            "let" => Some(self.binding(rest, target, span, tail)),
            "set!" => Some(self.assign(rest, target, span)),
            "." => Some(self.field(rest, target, span)),
            "defrecord" => Some(self.record(rest, target, span)),
            "quote" => Some(self.quote(rest, target, span)),
            "defmacro" => Some(self.macro_definition(rest, target, span)),
            "unquote" => Some(Err(Error::at(
                span,
                "~ is only allowed inside a quasiquote",
            ))),
            "unquote-splicing" => Some(Err(Error::at(
                span,
                "~@ is only allowed inside a quasiquote",
            ))),
            _ => None,
        }
    }

    fn define(&mut self, parts: &[Syntax], target: Register, span: Span) -> Result<(), Error> {
        let malformed = || Error::at(span, "def takes a name and a value");
        let name = match parts {
            [
                Syntax {
                    datum: Datum::Symbol(name),
                    ..
                },
                value,
            ] => {
                self.expression(value, target)?;
                Sym::intern(name)
            }
            [
                Syntax {
                    datum: Datum::List(signature),
                    ..
                },
                body @ ..,
            ] => {
                let Some((
                    Syntax {
                        datum: Datum::Symbol(name),
                        ..
                    },
                    parameters,
                )) = signature.split_first()
                else {
                    return Err(malformed());
                };
                let name = Sym::intern(name);
                self.lambda(
                    Some(name),
                    parameters,
                    Body::Forms(body),
                    None,
                    target,
                    span,
                )?;
                name
            }
            _ => return Err(malformed()),
        };
        self.emit(Op::DefineGlobal { name, from: target }, span);
        self.load(Value::Symbol(name), target, span)
    }

    fn record(&mut self, parts: &[Syntax], target: Register, span: Span) -> Result<(), Error> {
        let [
            Syntax {
                datum: Datum::Symbol(name),
                ..
            },
            Syntax {
                datum: Datum::Vector(fields),
                ..
            },
        ] = parts
        else {
            return Err(Error::at(span, "defrecord takes a name and a field vector"));
        };
        let parameters = parse_parameters(fields)?;
        if parameters.rest.is_some() {
            return Err(Error::at(span, "Records have no rest field"));
        }
        let names: Box<[Sym]> = parameters
            .required
            .iter()
            .copied()
            .chain(parameters.optional.iter().map(|(field, _)| *field))
            .chain(parameters.keywords.iter().map(|(field, _)| *field))
            .map(|field| Sym::intern(text(field)))
            .collect();
        let count = u16::try_from(names.len())
            .map_err(|_| Error::at(span, "This record has too many fields"))?;
        let name = Sym::intern(name);
        let record = self
            .heap
            .record_type(RecordType {
                name,
                fields: names,
                constructor: Value::Nil,
            })
            .map_err(|full| Error::at(span, full.to_string()))?;
        let body = Body::Record {
            record,
            fields: count,
        };
        self.lambda(Some(name), fields, body, None, target, span)?;
        let index = self.constant(record, span)?;
        self.emit(
            Op::DefineRecord {
                record: index,
                constructor: target,
            },
            span,
        );
        self.load(record, target, span)?;
        self.emit(Op::DefineGlobal { name, from: target }, span);
        self.load(Value::Symbol(name), target, span)
    }

    fn macro_definition(
        &mut self,
        parts: &[Syntax],
        target: Register,
        span: Span,
    ) -> Result<(), Error> {
        let [
            Syntax {
                datum: Datum::Symbol(name),
                ..
            },
            Syntax {
                datum: Datum::Vector(parameters),
                ..
            },
            body @ ..,
        ] = parts
        else {
            return Err(Error::at(
                span,
                "defmacro takes a name, a parameter vector and a body",
            ));
        };
        let name = Sym::intern(name);
        self.lambda(
            Some(name),
            parameters,
            Body::Forms(body),
            None,
            target,
            span,
        )?;
        self.emit(Op::DefineMacro { name, from: target }, span);
        self.load(Value::Symbol(name), target, span)
    }

    fn field(&mut self, parts: &[Syntax], target: Register, span: Span) -> Result<(), Error> {
        let Some((object, names)) = parts.split_first().filter(|(_, names)| !names.is_empty())
        else {
            return Err(Error::at(span, ". takes a value and field names"));
        };
        self.expression(object, target)?;
        for name in names {
            let Datum::Keyword(field) = &name.datum else {
                return Err(Error::at(name.span, "Field names must be names"));
            };
            let cache = u16::try_from(self.current.code.caches.len())
                .map_err(|_| Error::at(name.span, "This function reads too many fields"))?;
            self.current.code.caches.push(Cell::new(None));
            self.emit(
                Op::Field {
                    to: target,
                    from: target,
                    name: Sym::intern(field),
                    cache,
                },
                name.span,
            );
        }
        Ok(())
    }

    fn assign(&mut self, parts: &[Syntax], target: Register, span: Span) -> Result<(), Error> {
        let [
            assigned @ Syntax {
                datum: Datum::Symbol(name),
                ..
            },
            value,
        ] = parts
        else {
            return Err(Error::at(span, "set! takes a name and a value"));
        };
        let name = Sym::intern(name);
        self.expression(value, target)?;
        let Some(variable) = self.resolve(name, &assigned.marks) else {
            self.emit(Op::SetGlobal { name, from: target }, span);
            return Ok(());
        };
        match (variable.place, variable.boxed) {
            (Place::Register(register), true) => self.emit(
                Op::CellSet {
                    cell: register,
                    from: target,
                },
                span,
            ),
            (Place::Register(register), false) => self.emit(
                Op::Move {
                    to: register,
                    from: target,
                },
                span,
            ),
            (Place::Itself, _) => {
                return Err(Error::at(span, "set! cannot change a function's own name"));
            }
            (Place::Captured(index), _) => {
                let top = self.current.top;
                let cell = self.reserve(1, span)?;
                self.emit(Op::Captured { to: cell, index }, span);
                self.emit(Op::CellSet { cell, from: target }, span);
                self.release(top);
            }
        }
        Ok(())
    }

    fn binding(
        &mut self,
        parts: &[Syntax],
        target: Register,
        span: Span,
        tail: bool,
    ) -> Result<(), Error> {
        let Some((
            Syntax {
                datum: Datum::Vector(bindings),
                ..
            },
            body,
        )) = parts.split_first()
        else {
            return Err(Error::at(span, "let takes a binding vector and a body"));
        };
        if bindings.len() % 2 == 1 {
            return Err(Error::at(span, "let bindings come in name and value pairs"));
        }
        let top = self.current.top;
        let locals = self.current.locals.len();
        for (index, pair) in bindings.chunks(2).enumerate() {
            let [name, value] = pair else {
                continue;
            };
            let binder = name;
            let Datum::Symbol(name) = &binder.datum else {
                return Err(Error::at(binder.span, "Bindings must be names"));
            };
            let scope = &bindings[(index + 1) * 2..];
            let boxed = assigned_in(scope, name) || assigned_in(body, name);
            let register = self.reserve(1, span)?;
            self.expression(value, register)?;
            if boxed {
                self.emit(Op::MakeCell { register }, span);
            }
            self.current.locals.push(Local {
                name: Sym::intern(name),
                marks: binder.marks.clone(),
                register,
                boxed,
            });
        }
        self.sequence(body, target, span, tail)?;
        self.current.locals.truncate(locals);
        self.release(top);
        Ok(())
    }

    fn function(&mut self, parts: &[Syntax], target: Register, span: Span) -> Result<(), Error> {
        let (name, parts) = match parts {
            [
                named @ Syntax {
                    datum: Datum::Symbol(name),
                    ..
                },
                rest @ ..,
            ] => (Some((Sym::intern(name), named.marks.clone())), rest),
            _ => (None, parts),
        };
        let Some((
            Syntax {
                datum: Datum::Vector(parameters),
                ..
            },
            body,
        )) = parts.split_first()
        else {
            return Err(Error::at(span, "fn takes a parameter vector and a body"));
        };
        let itself = name.as_ref().map(|(_, marks)| marks.clone());
        let name = name.map(|(name, _)| name);
        self.lambda(name, parameters, Body::Forms(body), itself, target, span)
    }

    fn body(&mut self, body: Body<'_>, result: Register, span: Span) -> Result<(), Error> {
        match body {
            Body::Forms(forms) => self.sequence(forms, result, span, true),
            Body::Record { record, fields } => {
                let index = self.constant(record, span)?;
                self.emit(
                    Op::MakeRecord {
                        to: result,
                        record: index,
                        base: 0,
                        count: fields,
                    },
                    span,
                );
                Ok(())
            }
        }
    }

    fn lambda(
        &mut self,
        name: Option<Sym>,
        parameters: &[Syntax],
        body: Body<'_>,
        itself: Option<Marks>,
        target: Register,
        span: Span,
    ) -> Result<(), Error> {
        let parameters = parse_parameters(parameters)?;
        let forms = match body {
            Body::Forms(forms) => forms,
            Body::Record { .. } => &[],
        };
        let parent = std::mem::take(&mut self.current);
        self.outer.push(parent);
        self.current.code.name = name;
        self.current.itself = name.zip(itself);
        let defaults = parameters.optional.len() + parameters.keywords.len();
        if defaults > 64 {
            return Err(Error::at(
                span,
                "This function has too many optional and keyword parameters",
            ));
        }
        let mut order: Vec<(&Syntax, Option<&Syntax>)> = Vec::new();
        order.extend(parameters.required.iter().map(|name| (*name, None)));
        order.extend(parameters.optional.iter().copied());
        order.extend(parameters.rest.iter().map(|name| (*name, None)));
        order.extend(
            parameters
                .keywords
                .iter()
                .map(|(name, default)| (*name, Some(*default))),
        );
        let mut registers = Vec::new();
        for (parameter, _) in &order {
            let register = self.reserve(1, span)?;
            let boxed = assigned_in(forms, text(parameter));
            self.current.locals.push(Local {
                name: Sym::intern(text(parameter)),
                marks: parameter.marks.clone(),
                register,
                boxed,
            });
            registers.push((register, boxed));
        }
        self.current.code.arity = Arity {
            required: u16::try_from(parameters.required.len()).unwrap_or(u16::MAX),
            optional: u16::try_from(parameters.optional.len()).unwrap_or(u16::MAX),
            rest: parameters.rest.is_some(),
            keywords: parameters
                .keywords
                .iter()
                .map(|(name, _)| Sym::intern(text(name)))
                .collect(),
        };
        let first_default = parameters.required.len();
        let rest_slot = parameters
            .rest
            .map(|_| first_default + parameters.optional.len());
        let mut bit = 0_u8;
        for (index, ((_, default), (register, boxed))) in order.iter().zip(registers).enumerate() {
            let defaulted = index >= first_default && Some(index) != rest_slot;
            if defaulted {
                let skip = self.current.code.ops.len();
                self.emit(Op::JumpIfSupplied { bit, to: 0 }, span);
                match default {
                    Some(default) => self.expression(default, register)?,
                    None => self.load(Value::Nil, register, span)?,
                }
                let after = self.here(span)?;
                self.patch(skip, after);
                bit += 1;
            }
            if boxed {
                self.emit(Op::MakeCell { register }, span);
            }
        }
        let result = self.reserve(1, span)?;
        self.body(body, result, span)?;
        self.emit(Op::Return { from: result }, span);
        let parent = self
            .outer
            .pop()
            .ok_or_else(|| Error::at(span, "The compiler lost its enclosing function"))?;
        let inner = std::mem::replace(&mut self.current, parent);
        let functions = &mut self.current.code.functions;
        let index = u32::try_from(functions.len())
            .map_err(|_| Error::at(span, "This function holds too many functions"))?;
        functions.push(Rc::new(inner.code));
        self.emit(
            Op::Closure {
                to: target,
                function: index,
            },
            span,
        );
        Ok(())
    }

    fn branch(
        &mut self,
        parts: &[Syntax],
        target: Register,
        span: Span,
        tail: bool,
    ) -> Result<(), Error> {
        let (test, then, otherwise) = match parts {
            [test, then] => (test, then, None),
            [test, then, otherwise] => (test, then, Some(otherwise)),
            _ => {
                return Err(Error::at(
                    span,
                    "if takes a test, a then form and an optional else form",
                ));
            }
        };
        let condition = self.reserve(1, span)?;
        self.expression(test, condition)?;
        let skip = self.current.code.ops.len();
        self.emit(
            Op::JumpUnless {
                test: condition,
                to: 0,
            },
            span,
        );
        self.release(condition);
        self.form(then, target, tail)?;
        let finish = self.current.code.ops.len();
        self.emit(Op::Jump { to: 0 }, span);
        let alternative = self.here(span)?;
        self.patch(skip, alternative);
        match otherwise {
            Some(otherwise) => self.form(otherwise, target, tail)?,
            None => self.load(Value::Nil, target, span)?,
        }
        let end = self.here(span)?;
        self.patch(finish, end);
        Ok(())
    }

    fn sequence(
        &mut self,
        forms: &[Syntax],
        target: Register,
        span: Span,
        tail: bool,
    ) -> Result<(), Error> {
        if forms.is_empty() {
            return self.load(Value::Nil, target, span);
        }
        let last = forms.len() - 1;
        for (index, form) in forms.iter().enumerate() {
            self.form(form, target, tail && index == last)?;
        }
        Ok(())
    }

    fn gather(&mut self, items: &[Syntax], span: Span) -> Result<(Register, u16), Error> {
        let count = u16::try_from(items.len())
            .map_err(|_| Error::at(span, "This literal has too many items"))?;
        let base = self.reserve(items.len(), span)?;
        for (offset, item) in (0..).zip(items) {
            self.expression(item, base + offset)?;
        }
        Ok((base, count))
    }

    fn record_literal(
        &mut self,
        name: &str,
        marks: &Marks,
        items: &[Syntax],
        target: Register,
        span: Span,
    ) -> Result<(), Error> {
        let count = u16::try_from(items.len() + 1)
            .map_err(|_| Error::at(span, "This literal has too many items"))?;
        let base = self.reserve(items.len() + 1, span)?;
        self.variable(Sym::intern(name), marks, base, span);
        for (offset, item) in (1..).zip(items) {
            self.expression(item, base + offset)?;
        }
        self.emit(
            Op::MakeRecordFrom {
                to: target,
                base,
                count,
            },
            span,
        );
        self.release(base);
        Ok(())
    }

    fn set(&mut self, items: &[Syntax], target: Register, span: Span) -> Result<(), Error> {
        let (base, count) = self.gather(items, span)?;
        self.emit(
            Op::MakeSet {
                to: target,
                base,
                count,
            },
            span,
        );
        self.release(base);
        Ok(())
    }

    fn map(&mut self, items: &[Syntax], target: Register, span: Span) -> Result<(), Error> {
        let (base, count) = self.gather(items, span)?;
        self.emit(
            Op::MakeMap {
                to: target,
                base,
                count,
            },
            span,
        );
        self.release(base);
        Ok(())
    }

    fn vector(&mut self, items: &[Syntax], target: Register, span: Span) -> Result<(), Error> {
        let (base, count) = self.gather(items, span)?;
        self.emit(
            Op::MakeVector {
                to: target,
                base,
                count,
            },
            span,
        );
        self.release(base);
        Ok(())
    }

    fn call(
        &mut self,
        items: &[Syntax],
        target: Register,
        span: Span,
        tail: bool,
    ) -> Result<(), Error> {
        let Some((callee, arguments)) = items.split_first() else {
            return self.load(Value::Nil, target, span);
        };
        if let Some(done) = self.special(callee, arguments, target, span, tail) {
            return done;
        }
        let count = u16::try_from(arguments.len())
            .map_err(|_| Error::at(span, "This call has too many arguments"))?;
        let base = self.reserve(1 + arguments.len(), span)?;
        self.expression(callee, base)?;
        for (offset, argument) in (1..).zip(arguments) {
            self.expression(argument, base + offset)?;
        }
        if tail {
            self.emit(Op::TailCall { base, count }, span);
            self.release(base);
            return Ok(());
        }
        self.emit(Op::Call { base, count }, span);
        if base != target {
            self.emit(
                Op::Move {
                    to: target,
                    from: base,
                },
                span,
            );
        }
        self.release(base);
        Ok(())
    }
}
