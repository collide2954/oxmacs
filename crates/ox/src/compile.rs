use std::rc::Rc;

use crate::bytecode::{Capture, Code, Op, Register};
use crate::error::Error;
use crate::heap::Heap;
use crate::reader::{Datum, Syntax};
use crate::span::Span;
use crate::symbol::Sym;
use crate::value::Value;

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
}

#[derive(Debug, Clone, Copy)]
struct Variable {
    place: Place,
    boxed: bool,
}

struct Local {
    name: Sym,
    register: Register,
    boxed: bool,
}

#[derive(Default)]
struct Function {
    code: Code,
    top: Register,
    locals: Vec<Local>,
    captured: Vec<(Sym, bool)>,
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
        Datum::Vector(items) | Datum::Map(items) => items.iter().any(|item| assigns(item, name)),
        _ => false,
    }
}

fn assigned_in(forms: &[Syntax], name: &str) -> bool {
    forms.iter().any(|form| assigns(form, name))
}

impl Compiler<'_> {
    fn function_at(&mut self, depth: usize) -> Option<&mut Function> {
        if depth == self.outer.len() {
            Some(&mut self.current)
        } else {
            self.outer.get_mut(depth)
        }
    }

    fn resolve(&mut self, name: Sym) -> Option<Variable> {
        self.resolve_at(self.outer.len(), name)
    }

    fn resolve_at(&mut self, depth: usize, name: Sym) -> Option<Variable> {
        let function = self.function_at(depth)?;
        if let Some(local) = function
            .locals
            .iter()
            .rev()
            .find(|local| local.name == name)
        {
            return Some(Variable {
                place: Place::Register(local.register),
                boxed: local.boxed,
            });
        }
        if let Some((index, (_, boxed))) = (0_u16..)
            .zip(&function.captured)
            .find(|(_, (captured, _))| *captured == name)
        {
            return Some(Variable {
                place: Place::Captured(index),
                boxed: *boxed,
            });
        }
        let outside = self.resolve_at(depth.checked_sub(1)?, name)?;
        let function = self.function_at(depth)?;
        let index = u16::try_from(function.captured.len()).ok()?;
        function.captured.push((name, outside.boxed));
        function.code.captures.push(match outside.place {
            Place::Register(register) => Capture::Register(register),
            Place::Captured(index) => Capture::Captured(index),
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
        let value = match &form.datum {
            Datum::Symbol(name) => {
                self.variable(Sym::intern(name), target, form.span);
                return Ok(());
            }
            Datum::List(items) => return self.call(items, target, form.span),
            Datum::Nil => Value::Nil,
            Datum::Bool(flag) => Value::Bool(*flag),
            Datum::Int(number) => Value::Int(*number),
            Datum::Float(number) => Value::Float(*number),
            Datum::Char(character) => Value::Char(*character),
            Datum::Keyword(name) => Value::keyword(name),
            Datum::Str(text) => self
                .heap
                .string(text)
                .map_err(|full| Error::at(form.span, full.to_string()))?,
            _ => return Err(Error::at(form.span, "This form is not supported yet")),
        };
        self.load(value, target, form.span)
    }

    fn variable(&mut self, name: Sym, target: Register, span: Span) {
        let Some(variable) = self.resolve(name) else {
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
        if let Some(Op::Jump { to: target } | Op::JumpUnless { to: target, .. }) =
            self.current.code.ops.get_mut(jump)
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
    ) -> Option<Result<(), Error>> {
        let Datum::Symbol(name) = &head.datum else {
            return None;
        };
        match name.as_str() {
            "if" => Some(self.branch(rest, target, span)),
            "do" => Some(self.sequence(rest, target, span)),
            "def" => Some(self.define(rest, target, span)),
            "fn" => Some(self.function(rest, target, span)),
            "let" => Some(self.binding(rest, target, span)),
            "set!" => Some(self.assign(rest, target, span)),
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
                self.lambda(Some(name), parameters, body, target, span)?;
                name
            }
            _ => return Err(malformed()),
        };
        self.emit(Op::DefineGlobal { name, from: target }, span);
        self.load(Value::Symbol(name), target, span)
    }

    fn assign(&mut self, parts: &[Syntax], target: Register, span: Span) -> Result<(), Error> {
        let [
            Syntax {
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
        let Some(variable) = self.resolve(name) else {
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

    fn binding(&mut self, parts: &[Syntax], target: Register, span: Span) -> Result<(), Error> {
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
            let Datum::Symbol(name) = &name.datum else {
                return Err(Error::at(name.span, "Bindings must be names"));
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
                register,
                boxed,
            });
        }
        self.sequence(body, target, span)?;
        self.current.locals.truncate(locals);
        self.release(top);
        Ok(())
    }

    fn function(&mut self, parts: &[Syntax], target: Register, span: Span) -> Result<(), Error> {
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
        self.lambda(None, parameters, body, target, span)
    }

    fn lambda(
        &mut self,
        name: Option<Sym>,
        parameters: &[Syntax],
        body: &[Syntax],
        target: Register,
        span: Span,
    ) -> Result<(), Error> {
        let parent = std::mem::take(&mut self.current);
        self.outer.push(parent);
        self.current.code.name = name;
        for parameter in parameters {
            let Datum::Symbol(parameter_name) = &parameter.datum else {
                return Err(Error::at(parameter.span, "Parameters must be names"));
            };
            let register = self.reserve(1, parameter.span)?;
            self.current.locals.push(Local {
                name: Sym::intern(parameter_name),
                register,
                boxed: assigned_in(body, parameter_name),
            });
        }
        self.current.code.arity = self.current.top;
        let boxed: Vec<Register> = self
            .current
            .locals
            .iter()
            .filter(|local| local.boxed)
            .map(|local| local.register)
            .collect();
        for register in boxed {
            self.emit(Op::MakeCell { register }, span);
        }
        let result = self.reserve(1, span)?;
        self.sequence(body, result, span)?;
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

    fn branch(&mut self, parts: &[Syntax], target: Register, span: Span) -> Result<(), Error> {
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
        self.expression(then, target)?;
        let finish = self.current.code.ops.len();
        self.emit(Op::Jump { to: 0 }, span);
        let alternative = self.here(span)?;
        self.patch(skip, alternative);
        match otherwise {
            Some(otherwise) => self.expression(otherwise, target)?,
            None => self.load(Value::Nil, target, span)?,
        }
        let end = self.here(span)?;
        self.patch(finish, end);
        Ok(())
    }

    fn sequence(&mut self, forms: &[Syntax], target: Register, span: Span) -> Result<(), Error> {
        if forms.is_empty() {
            return self.load(Value::Nil, target, span);
        }
        for form in forms {
            self.expression(form, target)?;
        }
        Ok(())
    }

    fn call(&mut self, items: &[Syntax], target: Register, span: Span) -> Result<(), Error> {
        let Some((callee, arguments)) = items.split_first() else {
            return self.load(Value::Nil, target, span);
        };
        if let Some(done) = self.special(callee, arguments, target, span) {
            return done;
        }
        let count = u16::try_from(arguments.len())
            .map_err(|_| Error::at(span, "This call has too many arguments"))?;
        let base = self.reserve(1 + arguments.len(), span)?;
        self.expression(callee, base)?;
        for (offset, argument) in (1..).zip(arguments) {
            self.expression(argument, base + offset)?;
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
