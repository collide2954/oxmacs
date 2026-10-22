use crate::bytecode::{Op, Register};
use crate::error::Error;
use crate::expand::patterns::{Key, entries};
use crate::expand::quote::datum_value;
use crate::heap::Heap;
use crate::read::{Datum, Marks, Syntax};
use crate::span::Span;
use crate::symbol::Sym;
use crate::value::Value;

use super::{Compiler, Local, assigned_in};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Match,
    Bind,
}

pub(super) enum Operand {
    Register(Register),
    Value(Value),
}

pub struct Binder {
    pub(super) name: Sym,
    pub(super) marks: Marks,
    pub(super) register: Register,
}

pub struct Matching {
    mode: Mode,
    fail: Vec<usize>,
    pub binders: Vec<Binder>,
}

impl Matching {
    pub fn new(mode: Mode) -> Self {
        Self {
            mode,
            fail: Vec::new(),
            binders: Vec::new(),
        }
    }
}

fn quoted(syntax: &Syntax) -> Option<&Syntax> {
    match &syntax.datum {
        Datum::List(items) => match items.as_slice() {
            [
                Syntax {
                    datum: Datum::Symbol(head),
                    ..
                },
                inner,
            ] if head == "quote" => Some(inner),
            _ => None,
        },
        _ => None,
    }
}

fn literal(heap: &mut Heap, syntax: &Syntax) -> Result<Value, Error> {
    datum_value(heap, quoted(syntax).unwrap_or(syntax))
}

impl Compiler<'_> {
    pub(super) fn internal(
        &mut self,
        name: &str,
        arguments: &[Operand],
        target: Register,
        span: Span,
    ) -> Result<(), Error> {
        let top = self.current.top;
        let count = u16::try_from(arguments.len())
            .map_err(|_| Error::at(span, "This call has too many arguments"))?;
        let base = self.reserve(1 + arguments.len(), span)?;
        self.emit(
            Op::Global {
                to: base,
                name: Sym::intern(name),
            },
            span,
        );
        for (offset, argument) in (1..).zip(arguments) {
            match argument {
                Operand::Register(register) => self.emit(
                    Op::Move {
                        to: base + offset,
                        from: *register,
                    },
                    span,
                ),
                Operand::Value(value) => self.load(*value, base + offset, span)?,
            }
        }
        self.emit(Op::Call { base, count }, span);
        self.emit(
            Op::Move {
                to: target,
                from: base,
            },
            span,
        );
        self.release(top);
        Ok(())
    }

    fn check(
        &mut self,
        name: &str,
        arguments: &[Operand],
        matching: &mut Matching,
        span: Span,
    ) -> Result<(), Error> {
        let top = self.current.top;
        let flag = self.reserve(1, span)?;
        self.internal(name, arguments, flag, span)?;
        matching.fail.push(self.current.code.ops.len());
        self.emit(Op::JumpUnless { test: flag, to: 0 }, span);
        self.release(top);
        Ok(())
    }

    fn take(&mut self, name: &str, arguments: &[Operand], span: Span) -> Result<Register, Error> {
        let register = self.reserve(1, span)?;
        self.internal(name, arguments, register, span)?;
        Ok(register)
    }

    pub(super) fn pattern(
        &mut self,
        pattern: &Syntax,
        value: Register,
        matching: &mut Matching,
    ) -> Result<(), Error> {
        let span = pattern.span;
        match &pattern.datum {
            Datum::Symbol(name) if name == "_" => Ok(()),
            Datum::Symbol(name) if name == "&" => {
                Err(Error::at(span, "& belongs inside a vector pattern"))
            }
            Datum::Symbol(name) => {
                let register = self.reserve(1, span)?;
                self.emit(
                    Op::Move {
                        to: register,
                        from: value,
                    },
                    span,
                );
                matching.binders.push(Binder {
                    name: Sym::intern(name),
                    marks: pattern.marks.clone(),
                    register,
                });
                Ok(())
            }
            Datum::Vector(items) => self.sequence_pattern(items, value, matching, span),
            Datum::Map(items) => self.keyed_pattern(None, items, value, matching, span),
            Datum::Record(name, items) => {
                self.keyed_pattern(Some((name, &pattern.marks)), items, value, matching, span)
            }
            Datum::List(_) | Datum::Set(_) if quoted(pattern).is_none() => {
                Err(Error::at(span, "This is not a pattern"))
            }
            _ => {
                if matching.mode == Mode::Bind {
                    return Err(Error::at(
                        span,
                        "let patterns bind names; use match to test values",
                    ));
                }
                let expected = literal(self.heap, pattern)?;
                self.check(
                    "#=",
                    &[Operand::Register(value), Operand::Value(expected)],
                    matching,
                    span,
                )
            }
        }
    }

    fn sequence_pattern(
        &mut self,
        items: &[Syntax],
        value: Register,
        matching: &mut Matching,
        span: Span,
    ) -> Result<(), Error> {
        let ampersand = items
            .iter()
            .position(|item| matches!(&item.datum, Datum::Symbol(name) if name == "&"));
        let (fixed, rest) = match ampersand {
            Some(at) => {
                let [rest] = &items[at + 1..] else {
                    return Err(Error::at(span, "& takes one pattern for the rest"));
                };
                (&items[..at], Some(rest))
            }
            None => (items, None),
        };
        let count = i64::try_from(fixed.len()).unwrap_or(i64::MAX);
        if matching.mode == Mode::Match {
            self.check(
                "#sequential?",
                &[
                    Operand::Register(value),
                    Operand::Value(Value::Int(count)),
                    Operand::Value(Value::Bool(rest.is_some())),
                ],
                matching,
                span,
            )?;
        }
        for (index, item) in (0_i64..).zip(fixed) {
            let element = self.take(
                "#item",
                &[Operand::Register(value), Operand::Value(Value::Int(index))],
                span,
            )?;
            self.pattern(item, element, matching)?;
        }
        if let Some(rest) = rest {
            let remaining = self.take(
                "#from",
                &[Operand::Register(value), Operand::Value(Value::Int(count))],
                span,
            )?;
            self.pattern(rest, remaining, matching)?;
        }
        Ok(())
    }

    fn keyed_pattern(
        &mut self,
        record: Option<(&str, &Marks)>,
        items: &[Syntax],
        value: Register,
        matching: &mut Matching,
        span: Span,
    ) -> Result<(), Error> {
        let found = entries(items)?;
        if matching.mode == Mode::Match {
            if let Some((name, marks)) = record {
                let kind = self.reserve(1, span)?;
                self.variable(Sym::intern(name), marks, kind, span);
                self.check(
                    "#instance?",
                    &[Operand::Register(value), Operand::Register(kind)],
                    matching,
                    span,
                )?;
            } else {
                self.check("#keyed?", &[Operand::Register(value)], matching, span)?;
            }
        }
        for entry in found {
            let key = match entry.key {
                Key::Shorthand(name) => Value::keyword(name),
                Key::Literal(key) => literal(self.heap, key)?,
            };
            if matching.mode == Mode::Match {
                self.check(
                    "#has?",
                    &[Operand::Register(value), Operand::Value(key)],
                    matching,
                    span,
                )?;
            }
            let element = self.take(
                "#field",
                &[Operand::Register(value), Operand::Value(key)],
                span,
            )?;
            self.pattern(entry.pattern, element, matching)?;
        }
        Ok(())
    }

    pub(super) fn bind_names(&mut self, binders: Vec<Binder>, scope: &[&[Syntax]], span: Span) {
        for binder in binders {
            let boxed = scope
                .iter()
                .any(|forms| assigned_in(forms, binder.name.name()));
            if boxed {
                self.emit(
                    Op::MakeCell {
                        register: binder.register,
                    },
                    span,
                );
            }
            self.current.locals.push(Local {
                name: binder.name,
                marks: binder.marks,
                register: binder.register,
                boxed,
            });
        }
    }

    pub(super) fn match_form(
        &mut self,
        parts: &[Syntax],
        target: Register,
        span: Span,
        tail: bool,
    ) -> Result<(), Error> {
        let Some((scrutinee, clauses)) = parts.split_first() else {
            return Err(Error::at(span, "match takes a value and clauses"));
        };
        let top = self.current.top;
        let value = self.reserve(1, span)?;
        self.expression(scrutinee, value)?;
        let mut ends = Vec::new();
        for clause in clauses {
            let malformed = || Error::at(clause.span, "match clauses are [pattern body…] vectors");
            let Datum::Vector(items) = &clause.datum else {
                return Err(malformed());
            };
            let Some((pattern, rest)) = items.split_first() else {
                return Err(malformed());
            };
            let (guard, body) = match rest {
                [
                    Syntax {
                        datum: Datum::Keyword(word),
                        ..
                    },
                    guard,
                    body @ ..,
                ] if word == "when" => (Some(guard), body),
                _ => (None, rest),
            };
            let clause_top = self.current.top;
            let locals = self.current.locals.len();
            let mut matching = Matching::new(Mode::Match);
            self.pattern(pattern, value, &mut matching)?;
            let guarded = guard.map(std::slice::from_ref).unwrap_or_default();
            let binders = std::mem::take(&mut matching.binders);
            self.bind_names(binders, &[guarded, body], clause.span);
            if let Some(guard) = guard {
                let flag = self.reserve(1, guard.span)?;
                self.expression(guard, flag)?;
                matching.fail.push(self.current.code.ops.len());
                self.emit(Op::JumpUnless { test: flag, to: 0 }, guard.span);
            }
            self.sequence(body, target, clause.span, tail)?;
            ends.push(self.current.code.ops.len());
            self.emit(Op::Jump { to: 0 }, clause.span);
            let next = self.here(clause.span)?;
            for jump in matching.fail {
                self.patch(jump, next);
            }
            self.current.locals.truncate(locals);
            self.release(clause_top);
        }
        self.internal("#no-match", &[Operand::Register(value)], value, span)?;
        let end = self.here(span)?;
        for jump in ends {
            self.patch(jump, end);
        }
        self.release(top);
        Ok(())
    }
}
