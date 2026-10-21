use crate::bytecode::{Code, Op, Register};
use crate::error::Error;
use crate::heap::Heap;
use crate::reader::{Datum, Syntax};
use crate::span::Span;
use crate::symbol::Sym;
use crate::value::Value;

pub fn compile(heap: &mut Heap, form: &Syntax) -> Result<Code, Error> {
    let mut compiler = Compiler {
        heap,
        code: Code::default(),
        top: 0,
    };
    let target = compiler.reserve(1, form.span)?;
    compiler.expression(form, target)?;
    compiler.emit(Op::Return { from: target }, form.span);
    Ok(compiler.code)
}

struct Compiler<'a> {
    heap: &'a mut Heap,
    code: Code,
    top: Register,
}

impl Compiler<'_> {
    fn reserve(&mut self, count: usize, span: Span) -> Result<Register, Error> {
        let base = self.top;
        self.top = u16::try_from(usize::from(base) + count)
            .map_err(|_| Error::at(span, "This function needs too many registers"))?;
        self.code.registers = self.code.registers.max(self.top);
        Ok(base)
    }

    fn constant(&mut self, value: Value, span: Span) -> Result<u32, Error> {
        let index = u32::try_from(self.code.constants.len())
            .map_err(|_| Error::at(span, "This function has too many constants"))?;
        self.code.constants.push(value);
        Ok(index)
    }

    fn emit(&mut self, op: Op, span: Span) {
        self.code.ops.push(op);
        self.code.spans.push(span);
    }

    fn load(&mut self, value: Value, target: Register, span: Span) -> Result<(), Error> {
        let index = self.constant(value, span)?;
        self.emit(Op::Constant { to: target, index }, span);
        Ok(())
    }

    fn expression(&mut self, form: &Syntax, target: Register) -> Result<(), Error> {
        let value = match &form.datum {
            Datum::Symbol(name) => {
                self.emit(
                    Op::Global {
                        to: target,
                        name: Sym::intern(name),
                    },
                    form.span,
                );
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

    fn here(&self, span: Span) -> Result<u32, Error> {
        u32::try_from(self.code.ops.len()).map_err(|_| Error::at(span, "This function is too long"))
    }

    fn patch(&mut self, jump: usize, to: u32) {
        if let Some(Op::Jump { to: target } | Op::JumpUnless { to: target, .. }) =
            self.code.ops.get_mut(jump)
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
            _ => None,
        }
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
        let skip = self.code.ops.len();
        self.emit(
            Op::JumpUnless {
                test: condition,
                to: 0,
            },
            span,
        );
        self.top = condition;
        self.expression(then, target)?;
        let finish = self.code.ops.len();
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
        self.top = base;
        Ok(())
    }
}
