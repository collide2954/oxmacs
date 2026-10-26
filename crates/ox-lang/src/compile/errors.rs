use crate::bytecode::{Op, Register};
use crate::error::Error;
use crate::read::{Datum, Syntax};
use crate::span::Span;
use crate::symbol::Sym;
use crate::value::record::Family;

use super::patterns::{Binder, Operand};
use super::{Compiler, text};

struct Catch<'s> {
    kind: &'s Syntax,
    name: &'s Syntax,
    body: &'s [Syntax],
}

struct Clauses<'s> {
    body: Vec<&'s Syntax>,
    catches: Vec<Catch<'s>>,
    finally: Option<&'s [Syntax]>,
}

fn head(syntax: &Syntax) -> Option<&str> {
    match &syntax.datum {
        Datum::List(items) => items.first().map(text),
        _ => None,
    }
}

fn synthetic(name: &str, value: Syntax, span: Span) -> Syntax {
    Syntax::new(
        Datum::List(vec![
            Syntax::new(Datum::Symbol(name.to_owned()), span),
            value,
        ]),
        span,
    )
}

impl Compiler<'_> {
    pub(super) fn error_definition(
        &mut self,
        parts: &[Syntax],
        target: Register,
        span: Span,
    ) -> Result<(), Error> {
        let (name, fields, template) = match parts {
            [
                name @ Syntax {
                    datum: Datum::Symbol(_),
                    ..
                },
                Syntax {
                    datum: Datum::Vector(fields),
                    ..
                },
                rest @ ..,
            ] if rest.len() <= 1 => (text(name), fields, rest.first()),
            _ => {
                return Err(Error::at(
                    span,
                    "deferror takes a name, a field vector and an optional message",
                ));
            }
        };
        let message = template
            .cloned()
            .unwrap_or_else(|| Syntax::new(Datum::Str(name.to_owned()), span));
        let nil = Syntax::new(Datum::Nil, span);
        let mut all = fields.clone();
        if let Some(Syntax {
            datum: Datum::Map(keywords),
            ..
        }) = all.last_mut()
        {
            keywords.extend([
                Syntax::new(Datum::Symbol("message".to_owned()), span),
                message,
                Syntax::new(Datum::Symbol("traceback".to_owned()), span),
                nil,
            ]);
        } else {
            all.extend([
                synthetic("message", message, span),
                synthetic("traceback", nil, span),
            ]);
        }
        self.define_record(name, &all, Family::Error, target, span)
    }

    fn clauses(parts: &[Syntax]) -> Result<Clauses<'_>, Error> {
        let mut body = Vec::new();
        let mut catches = Vec::new();
        let mut finally = None;
        for part in parts {
            match head(part) {
                Some("catch") => {
                    let Datum::List(items) = &part.datum else {
                        continue;
                    };
                    let [_, kind, name, rest @ ..] = items.as_slice() else {
                        return Err(Error::at(
                            part.span,
                            "catch takes a type, a name and a body",
                        ));
                    };
                    if !matches!(kind.datum, Datum::Symbol(_))
                        || !matches!(name.datum, Datum::Symbol(_))
                    {
                        return Err(Error::at(
                            part.span,
                            "catch takes a type, a name and a body",
                        ));
                    }
                    catches.push(Catch {
                        kind,
                        name,
                        body: rest,
                    });
                }
                Some("finally") => {
                    if let Datum::List(items) = &part.datum {
                        finally = Some(&items[1..]);
                    }
                }
                _ => {
                    if !catches.is_empty() || finally.is_some() {
                        return Err(Error::at(
                            part.span,
                            "try's body comes before its catch and finally clauses",
                        ));
                    }
                    body.push(part);
                }
            }
        }
        Ok(Clauses {
            body,
            catches,
            finally,
        })
    }

    fn handle(
        &mut self,
        catch: &Catch<'_>,
        raised: Register,
        target: Register,
        span: Span,
    ) -> Result<usize, Error> {
        let top = self.current.top;
        let locals = self.current.locals.len();
        let flag = self.reserve(1, span)?;
        if text(catch.kind) == "_" {
            self.internal("#catchable?", &[Operand::Register(raised)], flag, span)?;
        } else {
            let kind = self.reserve(1, span)?;
            self.variable(Sym::intern(text(catch.kind)), &catch.kind.marks, kind, span);
            self.internal(
                "#catches?",
                &[Operand::Register(raised), Operand::Register(kind)],
                flag,
                span,
            )?;
        }
        let skip = self.current.code.ops.len();
        self.emit(Op::JumpUnless { test: flag, to: 0 }, span);
        let bound = self.reserve(1, span)?;
        self.emit(
            Op::Move {
                to: bound,
                from: raised,
            },
            span,
        );
        let binder = Binder {
            name: Sym::intern(text(catch.name)),
            marks: catch.name.marks.clone(),
            register: bound,
        };
        self.bind_names(vec![binder], &[catch.body], span);
        let forms: Vec<Syntax> = catch.body.to_vec();
        self.sequence(&forms, target, span, false)?;
        let done = self.current.code.ops.len();
        self.emit(Op::Jump { to: 0 }, span);
        self.current.locals.truncate(locals);
        self.release(top);
        let next = self.here(span)?;
        self.patch(skip, next);
        Ok(done)
    }

    pub(super) fn try_form(
        &mut self,
        parts: &[Syntax],
        target: Register,
        span: Span,
    ) -> Result<(), Error> {
        let Clauses {
            body,
            catches,
            finally,
        } = Self::clauses(parts)?;
        let body: Vec<Syntax> = body.into_iter().cloned().collect();
        if catches.is_empty() && finally.is_none() {
            return self.sequence(&body, target, span, false);
        }
        let top = self.current.top;
        let raised = self.reserve(1, span)?;
        let scratch = self.reserve(1, span)?;
        let push = self.current.code.ops.len();
        self.emit(
            Op::PushHandler {
                register: raised,
                to: 0,
            },
            span,
        );
        self.sequence(&body, target, span, false)?;
        self.emit(Op::PopHandler, span);
        let succeeded = self.current.code.ops.len();
        self.emit(Op::Jump { to: 0 }, span);
        let failed = self.here(span)?;
        self.patch(push, failed);
        let cleanup = match finally {
            Some(_) => {
                let again = self.reserve(1, span)?;
                let push = self.current.code.ops.len();
                self.emit(
                    Op::PushHandler {
                        register: again,
                        to: 0,
                    },
                    span,
                );
                Some((push, again))
            }
            None => None,
        };
        let mut handled = Vec::new();
        for catch in &catches {
            handled.push(self.handle(catch, raised, target, span)?);
        }
        self.emit(Op::Raise { from: raised }, span);
        let caught = self.here(span)?;
        for jump in handled {
            self.patch(jump, caught);
        }
        if let (Some((push, again)), Some(finally)) = (cleanup, finally) {
            self.emit(Op::PopHandler, span);
            let to_end = self.current.code.ops.len();
            self.emit(Op::Jump { to: 0 }, span);
            let failed_again = self.here(span)?;
            self.patch(push, failed_again);
            self.sequence(finally, scratch, span, false)?;
            self.emit(Op::Raise { from: again }, span);
            let end = self.here(span)?;
            self.patch(to_end, end);
            self.patch(succeeded, end);
            self.sequence(finally, scratch, span, false)?;
        } else {
            let end = self.here(span)?;
            self.patch(succeeded, end);
        }
        self.release(top);
        Ok(())
    }
}
