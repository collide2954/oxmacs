use crate::bytecode::{Code, Op, Register};
use crate::error::Error;
use crate::heap::Heap;
use crate::reader::{Datum, Syntax};
use crate::span::Span;
use crate::value::Value;

pub fn compile(heap: &mut Heap, form: &Syntax) -> Result<Code, Error> {
    let mut compiler = Compiler {
        heap,
        code: Code::default(),
    };
    let result = compiler.expression(form)?;
    compiler.emit(Op::Return { from: result }, form.span);
    Ok(compiler.code)
}

struct Compiler<'a> {
    heap: &'a mut Heap,
    code: Code,
}

impl Compiler<'_> {
    fn register(&mut self, span: Span) -> Result<Register, Error> {
        let register = self.code.registers;
        self.code.registers = register
            .checked_add(1)
            .ok_or_else(|| Error::at(span, "This function needs too many registers"))?;
        Ok(register)
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

    fn expression(&mut self, form: &Syntax) -> Result<Register, Error> {
        let value = match &form.datum {
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
        let to = self.register(form.span)?;
        let index = self.constant(value, form.span)?;
        self.emit(Op::Constant { to, index }, form.span);
        Ok(to)
    }
}
