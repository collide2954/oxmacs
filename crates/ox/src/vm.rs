use crate::bytecode::{Code, Op};
use crate::error::Error;
use crate::value::Value;

pub fn run(code: &Code) -> Result<Value, Error> {
    let mut registers = vec![Value::Nil; usize::from(code.registers)];
    let mut pc = 0;
    loop {
        let op = code
            .ops
            .get(pc)
            .ok_or_else(|| Error::new("The code ended without returning"))?;
        pc += 1;
        match *op {
            Op::Constant { to, index } => {
                registers[usize::from(to)] = code.constants[index as usize];
            }
            Op::Return { from } => return Ok(registers[usize::from(from)]),
        }
    }
}
